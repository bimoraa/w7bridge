/*! owner가 pin한 서명 key와 HTTPS artifact만 자동 갱신해. */

use crate::filesystem::digest;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;
use tokio_util::sync::CancellationToken;

type Failure = Box<dyn std::error::Error + Send + Sync>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {

    version: u32,
    manifest_url: String,
    public_key_base64: String,
    installed_executable: PathBuf,
    #[serde(default)]
    service: bool,
    #[serde(default = "default_interval")]
    interval_seconds: u64,

}

fn default_interval( ) -> u64 {

    3600

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Signed {

    payload_base64: String,
    signature_base64: String,

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Release {

    sequence: u64,
    version: String,
    target: String,
    artifact_url: String,
    sha256: String,
    bytes: usize,
    protocol_min: u32,
    protocol_max: u32,

}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {

    sequence: u64,
    #[serde(default)]
    pending: Option<Pending>,
    #[serde(default)]
    last_error: Option<String>,
    #[serde(default)]
    last_check_unix: u64,

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pending {

    previous_hash: String,
    next_hash: String,
    sequence: u64,
    phase: String,
    was_running: bool,

}

pub(crate) async fn run( args: &[OsString], ) -> Result<(),Failure> {

    if matches!(args,[flag] if flag=="--help" || flag=="-h") {

        println!(
            "사용법: w7bridge update --config <owner TOML> [--once | --status]\n자동 실행 설치: w7bridge update install --config <owner TOML>\nrelease 서명: w7bridge update sign --release <JSON> --key <32-byte key 파일> --output <manifest JSON>\nHTTPS와 pin한 Ed25519 key가 필요합니다. project MCP client는 update 설정을 바꾸지 못합니다."
        );
        return Ok(());

    }
    if args.first().is_some_and(|arg| arg == "sign") {

        return sign(&args[1..]);

    }
    if let [install, flag, path] = args
        && install == "install"
        && flag == "--config"
    {

        let path = PathBuf::from(path).canonicalize()?;
        let settings = load(&path)?;
        return install_host(&path, &settings);

    }
    let (path, mode) = match args {

        [flag, path] if flag == "--config" => (PathBuf::from(path), "watch"),
        [flag, path, mode] if flag == "--config" && matches!(mode.to_str(), Some("--once" | "--status")) => {

            (PathBuf::from(path), mode.to_str().ok_or("UTF-8 옵션이 필요합니다")?)

        }
        _ => return Err("update --help로 인자를 확인하세요".into()),

    };
    let settings = load(&path)?;
    let directory = settings.installed_executable.parent().ok_or("설치 root가 없습니다")?.join(".w7bridge-update");
    regular_directory(&directory, true)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("update.lock"))?;
    lock.try_lock_exclusive().map_err(|_| "다른 updater가 실행 중입니다")?;
    let mut state = read_state(&directory)?;
    if mode == "--status" {

        println!("{}", serde_json::to_string(&state)?);
        return Ok(());

    }
    recover(&settings, &directory, &mut state).await?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()?;
    let cancellation = CancellationToken::new();
    loop {

        let result = tokio::select! {
            result=cycle(&settings,&directory,&client,&mut state,cancellation.clone())=>result,
            _=tokio::signal::ctrl_c()=>{cancellation.cancel();break;},
        };
        state.last_check_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
        state.last_error = result.as_ref().err().map(ToString::to_string);
        save_state(&directory, &state)?;
        if let Err(error) = result {

            eprintln!("자동 update를 완료하지 못했습니다: {error}");
            if mode == "--once" {

                return Err(error);

            }

        }
        if mode == "--once" {

            return Ok(());

        }
        tokio::select! { _=tokio::signal::ctrl_c()=>break,_=tokio::time::sleep(Duration::from_secs(settings.interval_seconds))=>{} }

    }
    Ok(())

}

fn load( path: &Path, ) -> Result<Settings,Failure> {

    regular_file(path)?;
    let settings: Settings = toml::from_str(&fs::read_to_string(path)?)?;
    if settings.version != 1
        || !(60..=86400).contains(&settings.interval_seconds)
        || !settings.installed_executable.is_absolute()
        || settings
            .installed_executable
            .file_name()
            .is_none_or(|name| name != if cfg!(windows) { "w7bridge.exe" } else { "w7bridge" })
        || settings.service && !cfg!(windows)
    {

        return Err("update 설정의 version, 설치 경로와 interval을 확인하세요".into());

    }
    https(&settings.manifest_url)?;
    key(&settings.public_key_base64)?;
    regular_file(&settings.installed_executable)?;
    if settings.installed_executable.canonicalize()? == std::env::current_exe()?.canonicalize()? {

        return Err("updater helper는 갱신할 binary와 별도로 실행해야 합니다. update install을 사용하세요".into());

    }
    Ok(settings)

}

fn key( encoded: &str, ) -> Result<VerifyingKey,Failure> {

    let bytes: [u8; 32] =
        STANDARD.decode(encoded)?.try_into().map_err(|_| "Ed25519 public key는 32 byte여야 합니다")?;
    Ok(VerifyingKey::from_bytes(&bytes)?)

}

fn verify( bytes: &[u8], settings: &Settings, previous: u64, ) -> Result<Release,Failure> {

    if bytes.len() > 65536 {

        return Err("update manifest 한도를 초과했습니다".into());

    }
    let signed: Signed = serde_json::from_slice(bytes)?;
    let payload = STANDARD.decode(signed.payload_base64)?;
    let signature = Signature::from_slice(&STANDARD.decode(signed.signature_base64)?)?;
    key(&settings.public_key_base64)?
        .verify_strict(&payload, &signature)
        .map_err(|_| "update signature가 pin한 key와 다릅니다")?;
    let release: Release = serde_json::from_slice(&payload)?;
    let target = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    if release.sequence < previous
        || release.sequence == 0
        || release.target != target
        || release.bytes == 0
        || release.bytes > 128 * 1024 * 1024
        || release.sha256.len() != 64
        || !release.sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || release.version.is_empty()
        || release.version.len() > 64
        || !release.version.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
        || release.protocol_min > 2
        || release.protocol_max < 1
        || release.protocol_min > release.protocol_max
    {

        return Err("update version, target, hash 또는 protocol이 맞지 않습니다".into());

    }
    https(&release.artifact_url)?;
    Ok(release)

}

fn https( url: &str, ) -> Result<(),Failure> {

    let url = reqwest::Url::parse(url)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {

        return Err("credential 없는 HTTPS update URL이 필요합니다".into());

    }
    Ok(())

}

async fn download( client: &reqwest::Client, url: &str, limit: usize, cancellation: CancellationToken, ) -> Result<Vec<u8>,Failure> {

    let work = async {

        let mut response = client.get(url).send().await?.error_for_status()?;
        if response.content_length().is_some_and(|bytes| bytes > limit as u64) {

            return Err::<Vec<u8>, Failure>("update 다운로드 한도를 초과했습니다".into());

        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {

            if bytes.len() + chunk.len() > limit {

                return Err("update 다운로드 한도를 초과했습니다".into());

            }
            bytes.extend_from_slice(&chunk);

        }
        Ok(bytes)

    };
    tokio::select! { result=work=>result,_=cancellation.cancelled()=>Err("update 다운로드가 취소되었습니다".into()) }

}

async fn cycle( settings: &Settings, directory: &Path, client: &reqwest::Client, state: &mut State, cancellation: CancellationToken, ) -> Result<(),Failure> {

    if state.pending.is_some() {

        recover(settings, directory, state).await?;

    }
    let manifest = download(client, &settings.manifest_url, 65536, cancellation.clone()).await?;
    let release = verify(&manifest, settings, state.sequence)?;
    let current = digest(&fs::read(&settings.installed_executable)?);
    if release.sha256 == current {

        state.sequence = state.sequence.max(release.sequence);
        return Ok(());

    }
    if release.sequence == state.sequence {

        return Err("같은 update sequence에 다른 binary가 있습니다".into());

    }
    let bytes = download(client, &release.artifact_url, release.bytes, cancellation.clone()).await?;
    if bytes.len() != release.bytes || digest(&bytes) != release.sha256 {

        return Err("update artifact SHA-256이 다릅니다".into());

    }
    let candidate = directory.join(if cfg!(windows) { "candidate.exe" } else { "candidate" });
    write(&candidate, &bytes)?;
    fs::set_permissions(&candidate, fs::metadata(&settings.installed_executable)?.permissions())?;
    if version(&candidate, cancellation).await? != format!("w7bridge {}", release.version) {

        return Err("candidate binary version이 manifest와 다릅니다".into());

    }
    match apply(settings, directory, state, &release, &candidate).await {

        Ok(()) => Ok(()),
        Err(error) => {

            recover(settings, directory, state).await?;
            Err(error)

        }

    }

}

async fn version( executable: &Path, cancellation: CancellationToken, ) -> Result<String,Failure> {

    use process_wrap::tokio::CommandWrap;
    let mut command = CommandWrap::with_new(executable, |command| {

        command
            .arg("--version")
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        for key in ["SystemRoot", "WINDIR", "PATH"] {

            if let Some(value) = std::env::var_os(key) {

                command.env(key, value);

            }

        }

    });
    crate::platform::configure(&mut command);
    let child = command.spawn()?;
    let mut guard = crate::execution::process::ChildGuard { child, active: true };
    let output = guard.child.stdout().take().ok_or("candidate stdout이 없습니다")?;
    let mut bytes = Vec::new();
    use tokio::io::AsyncReadExt;
    let result = tokio::select! {
        _=cancellation.cancelled()=>Err("candidate 확인이 취소되었습니다".into()),
        result=tokio::time::timeout(Duration::from_secs(10),async {tokio::try_join!(guard.child.wait(),async {output.take(1025).read_to_end(&mut bytes).await})})=>match result {
            Ok(Ok((exit,_))) if exit.success() && bytes.len()<=1024 => Ok(String::from_utf8(bytes)?.trim().to_owned()),
            _=>Err::<String,Failure>("candidate binary 확인이 실패했습니다".into()),
        },
    };
    if result.is_err() {

        crate::execution::process::stop(&mut *guard.child).await?;

    }
    guard.active = false;
    result

}

async fn apply( settings: &Settings, directory: &Path, state: &mut State, release: &Release, candidate: &Path, ) -> Result<(),Failure> {

    let previous_hash = digest(&fs::read(&settings.installed_executable)?);
    let was_running = service_running(settings)?;
    let backup = directory.join(if cfg!(windows) { "previous.exe" } else { "previous" });
    write(&backup, &fs::read(&settings.installed_executable)?)?;
    fs::set_permissions(&backup, fs::metadata(&settings.installed_executable)?.permissions())?;
    state.pending = Some(Pending {

        previous_hash,
        next_hash: release.sha256.clone(),
        sequence: release.sequence,
        phase: "prepared".into(),
        was_running,

    });
    save_state(directory, state)?;
    if was_running {

        service_control(settings, false).await?;

    }
    if let Err(error) = replace(candidate, &settings.installed_executable) {

        if was_running {

            service_control(settings, true).await?;

        }
        return Err(error);

    }
    if let Some(pending) = state.pending.as_mut() {

        pending.phase = "replaced".into();

    }
    save_state(directory, state)?;
    let verified = version(&settings.installed_executable, CancellationToken::new()).await?
        == format!("w7bridge {}", release.version);
    if !verified {

        return recover(settings, directory, state).await;

    }
    if was_running && let Err(error) = service_control(settings, true).await {

        recover(settings, directory, state).await?;
        return Err(error);

    }
    restart_sync(settings)?;
    state.sequence = release.sequence;
    state.pending = None;
    save_state(directory, state)?;
    Ok(())

}

async fn recover( settings: &Settings, directory: &Path, state: &mut State, ) -> Result<(),Failure> {

    let Some(pending) = state.pending.as_ref() else { return Ok(()) };
    let current = digest(&fs::read(&settings.installed_executable)?);
    if current != pending.previous_hash && current != pending.next_hash {

        return Err("update 복구 중 새 binary 변경을 발견했습니다. 덮어쓰지 않습니다".into());

    }
    if current == pending.next_hash {

        let backup = directory.join(if cfg!(windows) { "previous.exe" } else { "previous" });
        regular_file(&backup)?;
        if digest(&fs::read(&backup)?) != pending.previous_hash {

            return Err("update backup hash가 다릅니다".into());

        }
        if service_running(settings)? {

            service_control(settings, false).await?;

        }
        let restored = directory.join(if cfg!(windows) { "restore.exe" } else { "restore" });
        write(&restored, &fs::read(&backup)?)?;
        fs::set_permissions(&restored, fs::metadata(&backup)?.permissions())?;
        replace(&restored, &settings.installed_executable)?;

    }
    if pending.was_running && !service_running(settings)? {

        service_control(settings, true).await?;

    }
    state.pending = None;
    save_state(directory, state)?;
    Ok(())

}

fn replace( source: &Path, destination: &Path, ) -> Result<(),Failure> {

    regular_file(source)?;
    regular_file(destination)?;
    fs::rename(source, destination)?;
    Ok(())

}

fn regular_file( path: &Path, ) -> Result<(),Failure> {

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || crate::filesystem::paths::redirected(&metadata) {

        return Err("update 경로는 reparse point가 아닌 일반 파일이어야 합니다".into());

    }
    Ok(())

}

fn regular_directory( path: &Path, create: bool, ) -> Result<(),Failure> {

    if create && !path.exists() {

        fs::create_dir(path)?;

    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || crate::filesystem::paths::redirected(&metadata) {

        return Err("update 경로는 일반 디렉터리여야 합니다".into());

    }
    Ok(())

}

fn write( path: &Path, bytes: &[u8], ) -> Result<(),Failure> {

    if path.exists() {

        regular_file(path)?;

    }
    let mut temporary = NamedTempFile::new_in(path.parent().ok_or("update 부모 경로가 없습니다")?)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())

}

fn read_state( directory: &Path, ) -> Result<State,Failure> {

    let path = directory.join("state.json");
    if !path.exists() {

        return Ok(State::default());

    }
    regular_file(&path)?;
    Ok(serde_json::from_slice(&fs::read(path)?)?)

}

fn save_state( directory: &Path, state: &State, ) -> Result<(),Failure> {

    write(&directory.join("state.json"), &serde_json::to_vec(state)?)

}

fn service_running( settings: &Settings, ) -> Result<bool,Failure> {

    if !settings.service {

        return Ok(false);

    }
    #[cfg(windows)]
    {

        use windows_service::{
            service::ServiceAccess,
            service_manager::{ServiceManager, ServiceManagerAccess},
        };
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service("w7bridge", ServiceAccess::QUERY_STATUS)?;
        Ok(service.query_status()?.current_state == windows_service::service::ServiceState::Running)

    }
    #[cfg(not(windows))]
    {

        Err("Windows service update 설정을 확인하세요".into())

    }

}

async fn service_control( settings: &Settings, start: bool, ) -> Result<(),Failure> {

    if !settings.service {

        return Ok(());

    }
    #[cfg(windows)]
    {

        use windows_service::{
            service::{ServiceAccess, ServiceState},
            service_manager::{ServiceManager, ServiceManagerAccess},
        };
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service = manager
            .open_service("w7bridge", ServiceAccess::START | ServiceAccess::STOP | ServiceAccess::QUERY_STATUS)?;
        if start {

            service.start::<&str>(&[])?;

        } else {

            service.stop()?;

        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {

            if service.query_status()?.current_state
                == if start { ServiceState::Running } else { ServiceState::Stopped }
            {

                return Ok(());

            }
            if tokio::time::Instant::now() >= deadline {

                return Err("update service 상태 전환 시간이 초과되었습니다".into());

            }
            tokio::time::sleep(Duration::from_millis(200)).await;

        }

    }
    #[cfg(not(windows))]
    {

        let _ = start;
        Err("Windows service update 설정을 확인하세요".into())

    }

}

fn restart_sync( settings: &Settings, ) -> Result<(),Failure> {

    #[cfg(target_os = "macos")]
    {

        let installed = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Application Support/w7bridge/w7bridge"));
        if installed.as_ref().is_some_and(|path| path == &settings.installed_executable) {

            let id = std::process::Command::new("/usr/bin/id").arg("-u").output()?;
            let uid = String::from_utf8(id.stdout)?;
            if !std::process::Command::new("/bin/launchctl")
                .args(["kickstart", "-k", &format!("gui/{}/com.w7bridge.sync", uid.trim())])
                .status()?
                .success()
            {

                return Err("Mac sync daemon 재시작을 확인할 수 없습니다".into());

            }

        }

    }
    #[cfg(not(target_os = "macos"))]
    let _ = settings;
    Ok(())

}

fn install_host( config: &Path, settings: &Settings, ) -> Result<(),Failure> {

    let directory = settings.installed_executable.parent().ok_or("update root가 없습니다")?.join(".w7bridge-update");
    regular_directory(&directory, true)?;
    let helper = directory.join(if cfg!(windows) { "helper.exe" } else { "helper" });
    if helper.exists() {

        return Err("기존 update helper를 덮어쓰지 않습니다".into());

    }
    let source = std::env::current_exe()?;
    write(&helper, &fs::read(&source)?)?;
    fs::set_permissions(&helper, fs::metadata(source)?.permissions())?;
    let private_config = directory.join("owner.toml");
    if private_config.exists() {

        return Err("기존 updater 설정을 덮어쓰지 않습니다".into());

    }
    write(&private_config, &fs::read(config)?)?;
    #[cfg(unix)]
    {

        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&private_config, fs::Permissions::from_mode(0o600))?;

    }
    install_schedule(&private_config, &helper)

}

fn install_schedule( config: &Path, helper: &Path, ) -> Result<(),Failure> {

    #[cfg(windows)]
    {

        let command = format!("\"{}\" update --config \"{}\"", helper.display(), config.display());
        let username = format!("{}\\{}", std::env::var("USERDOMAIN")?, std::env::var("USERNAME")?);
        let output = std::process::Command::new("schtasks.exe")
            .args([
                "/Create",
                "/TN",
                "w7bridge-update",
                "/SC",
                "ONLOGON",
                "/RU",
                &username,
                "/IT",
                "/RL",
                "LIMITED",
                "/TR",
                &command,
            ])
            .output()?;
        if !output.status.success() {

            return Err("현재 사용자 updater task를 설치할 수 없습니다. 기존 task는 보존합니다".into());

        }
        if !std::process::Command::new("schtasks.exe").args(["/Run", "/TN", "w7bridge-update"]).status()?.success() {

            return Err("updater task 시작을 확인할 수 없습니다".into());

        }
        Ok(())

    }
    #[cfg(target_os = "macos")]
    {

        let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME이 없습니다")?);
        let plist = home.join("Library/LaunchAgents/com.w7bridge.update.plist");
        if plist.exists() {

            return Err("기존 updater LaunchAgent를 덮어쓰지 않습니다".into());

        }
        let escape = |path: &Path| {

            path.to_string_lossy()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")

        };
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>Label</key><string>com.w7bridge.update</string><key>ProgramArguments</key><array><string>{}</string><string>update</string><string>--config</string><string>{}</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>ThrottleInterval</key><integer>30</integer><key>StandardOutPath</key><string>{}</string><key>StandardErrorPath</key><string>{}</string></dict></plist>",
            escape(helper),
            escape(config),
            escape(&helper.with_extension("log")),
            escape(&helper.with_extension("log"))
        );
        let mut file = fs::OpenOptions::new().create_new(true).write(true).open(&plist)?;
        file.write_all(xml.as_bytes())?;
        file.sync_all()?;
        let id = std::process::Command::new("/usr/bin/id").arg("-u").output()?;
        let uid = String::from_utf8(id.stdout)?;
        if !std::process::Command::new("/bin/launchctl")
            .args(["bootstrap", &format!("gui/{}", uid.trim())])
            .arg(&plist)
            .status()?
            .success()
        {

            return Err("updater LaunchAgent 시작을 확인할 수 없습니다".into());

        }
        Ok(())

    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {

        let _ = (config, helper);
        Err("updater 자동 시작은 Windows와 Mac에서 지원합니다".into())

    }

}

fn sign( args: &[OsString], ) -> Result<(),Failure> {

    use ed25519_dalek::{Signer, SigningKey};
    let [release_flag, release, key_flag, key_path, output_flag, output] = args else {

        return Err("update sign 인자를 확인하세요".into());

    };
    if release_flag != "--release" || key_flag != "--key" || output_flag != "--output" {

        return Err("update sign 인자를 확인하세요".into());

    }
    let payload = fs::read(release)?;
    let _: Release = serde_json::from_slice(&payload)?;
    let bytes: [u8; 32] = fs::read(key_path)?.try_into().map_err(|_| "서명 key는 32 byte여야 합니다")?;
    let key = SigningKey::from_bytes(&bytes);
    let signed = Signed {

        payload_base64: STANDARD.encode(&payload),
        signature_base64: STANDARD.encode(key.sign(&payload).to_bytes()),

    };
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(output)?;
    file.write_all(&serde_json::to_vec(&signed)?)?;
    file.sync_all()?;
    eprintln!("release 서명이 끝났습니다. public key: {}", STANDARD.encode(key.verifying_key().to_bytes()));
    Ok(())

}

#[cfg(test)]
#[path = "../../tests/unit/update.rs"]
mod tests;

/*! server host와 pairing 작업의 종료를 기다려. */

use crate::{Bridge, Config, config::SyncSettings, protocol::types::Failure, serve_stdio};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;
pub(super) async fn serve(path: &Path) -> Result<(), Failure> {

    let shutdown = CancellationToken::new();
    let bridge = Bridge::new(Config::load(path)?, shutdown.clone())?;
    let serving = serve_stdio(bridge, shutdown.clone());
    tokio::pin!(serving);

    tokio::select! {
        result = &mut serving => result.map_err(Into::into),
        signal = tokio::signal::ctrl_c() => {
            shutdown.cancel();
            serving.await?;
            signal.map_err(|_| "종료 신호를 읽을 수 없습니다".into())
        }
    }

}

pub(super) async fn run_sync(args: &[OsString]) -> Result<(), Failure> {

    if matches!(args, [flag] if flag == "--help" || flag == "-h") {

        println!(
            "사용법: w7bridge sync --config <pairing TOML> [--once | --status]\nMac 자동 시작: w7bridge sync install --config <pairing TOML>\nMac host 제어: sync start | stop | background-status | uninstall\n기본 모드는 계속 실행하고 연결 실패 후 자동 재연결합니다. --status는 마지막 관찰 시각도 표시합니다."
        );
        return Ok(());

    }
    #[cfg(target_os = "macos")]
    if let [action, flag, path] = args
        && action == "install"
        && flag == "--config"
    {

        let settings: SyncSettings = toml::from_str(&fs::read_to_string(PathBuf::from(path))?)?;
        validate_sync_settings(&settings)?;
        return crate::platform::macos::process::daemon::install(&PathBuf::from(path));

    }
    #[cfg(target_os = "macos")]
    if let [action] = args
        && matches!(action.to_str(), Some("start" | "stop" | "background-status" | "uninstall"))
    {

        return crate::platform::macos::process::daemon::control(action.to_str().ok_or("UTF-8 옵션이 필요합니다")?);

    }
    let (path, mode) = match args {

        [flag, path] if flag == "--config" => (PathBuf::from(path), "watch"),
        [flag, path, mode] if flag == "--config" && (mode == "--once" || mode == "--status") => {

            (PathBuf::from(path), mode.to_str().ok_or("UTF-8 옵션이 필요합니다")?)

        }
        _ => return Err("sync --help로 인자를 확인하세요".into()),

    };
    let settings: SyncSettings = toml::from_str(&fs::read_to_string(path)?)?;
    validate_sync_settings(&settings)?;
    let mut roots = BTreeSet::new();
    let mut peers = BTreeSet::new();
    for pair in &settings.pairs {

        pair.options()?;
        if !peers.insert(serde_json::to_string(&(pair.options()?.ssh_args(), &pair.remote_project))?) {

            return Err("같은 remote project를 여러 local root에 연결할 수 없습니다".into());

        }
        if mode != "--status" {

            fs::create_dir_all(&pair.local_root)?;

        }
        let root = pair.local_root.canonicalize()?;
        if roots.iter().any(|other: &PathBuf| root != *other && (root.starts_with(other) || other.starts_with(&root))) {

            return Err("서로 다른 pairing root는 서로 포함할 수 없습니다".into());

        }
        roots.insert(root);

    }
    let once = mode == "--once";
    let status = mode == "--status";
    let shutdown = CancellationToken::new();
    let mut tasks = tokio::task::JoinSet::new();
    for pair in settings.pairs {

        let token = shutdown.clone();
        let interval = settings.interval_seconds;
        tasks.spawn(async move { crate::filesystem::watcher::watch(pair, interval, once, status, token).await });

    }
    let result = loop {

        tokio::select! {
            result = tasks.join_next() => match result {
                Some(Ok(Ok(()))) => {},
                Some(Ok(Err(error))) => break Err(error),
                Some(Err(error)) => break Err(error.into()),
                None => break Ok(()),
            },
            _ = tokio::signal::ctrl_c() => break Ok(()),
        }

    };
    shutdown.cancel();
    while tasks.join_next().await.is_some() {}
    result

}

pub(super) fn validate_sync_settings(settings: &SyncSettings) -> Result<(), Failure> {

    if settings.version != 1
        || !(1..=60).contains(&settings.interval_seconds)
        || settings.pairs.is_empty()
        || settings.pairs.len() > 16
    {

        return Err("version은 1, interval_seconds는 1..=60, pairing은 1..=16개여야 합니다".into());

    }
    for pair in &settings.pairs {

        pair.options()?;
        if let Some(config) = &pair.screenshot_config {

            let mut capture = pair.clone();
            capture.service = false;
            capture.config = Some(config.clone());
            capture.options()?;

        }
        if pair.bandwidth_bytes_per_second > 1_073_741_824 {

            return Err("bandwidth_bytes_per_second는 0..=1073741824여야 합니다".into());

        }
        if pair.git_executable.as_ref().is_some_and(|path| !path.is_absolute() || !path.is_file())
            || pair.expected_device_id.as_ref().is_some_and(|id| {

                id.is_empty()
                    || id.len() > 64
                    || !id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))

            })
        {

            return Err("git_executable은 승인한 절대 파일, expected_device_id는 영문·숫자 ID여야 합니다".into());

        }
        if !pair.local_root.is_absolute()
            || pair.remote_project.is_empty()
            || pair.remote_project.len() > 64
            || !pair
                .remote_project
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
        {

            return Err("절대 local_root와 등록된 remote_project ID가 필요합니다".into());

        }

    }
    Ok(())

}

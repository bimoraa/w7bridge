/*! macOS를 포함한 Unix process group 수명을 구성해. */

use process_wrap::tokio::{CommandWrap, ProcessGroup};
pub(crate) fn configure(command: &mut CommandWrap) {

    command.wrap(ProcessGroup::leader());

}

#[cfg(target_os = "macos")]
pub(crate) mod daemon {

    #![allow(non_upper_case_globals)]
    /*! 사용자 LaunchAgent로 Mac sync daemon을 로그인 후 다시 시작해. 다른 job은 변경하지 않아. */

    use std::{
        fs,
        io::{self, Write},
        path::{Path, PathBuf},
        process::Command,
    };
    use tempfile::NamedTempFile;

    type Failure = Box<dyn std::error::Error + Send + Sync>;
    const label: &str = "com.w7bridge.sync";

    fn paths() -> Result<(PathBuf, PathBuf), Failure> {

        let home = std::env::var_os("HOME").ok_or("사용자 HOME 경로가 없습니다")?;
        let home = PathBuf::from(home);
        if !home.is_absolute() {

            return Err("절대 HOME 경로가 필요합니다".into());

        }
        Ok((
            home.join("Library/LaunchAgents/com.w7bridge.sync.plist"),
            home.join("Library/Application Support/w7bridge"),
        ))

    }
    fn domain() -> Result<String, Failure> {

        let output = Command::new("/usr/bin/id").arg("-u").output()?;
        if !output.status.success() {

            return Err("사용자 ID를 읽을 수 없습니다".into());

        }
        let uid: u32 = std::str::from_utf8(&output.stdout)?.trim().parse()?;
        Ok(format!("gui/{uid}"))

    }
    fn escape(value: &str) -> String {

        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")

    }
    fn string(path: &Path) -> Result<String, Failure> {

        Ok(escape(path.to_str().ok_or("UTF-8 경로가 필요합니다")?))

    }
    fn reject_link(path: &Path) -> Result<(), Failure> {

        match fs::symlink_metadata(path) {

            Ok(metadata) if metadata.is_symlink() => Err("LaunchAgent 경로의 symlink를 사용할 수 없습니다".into()),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),

        }

    }

    pub fn install(config: &Path) -> Result<(), Failure> {

        let config = config.canonicalize()?;
        let (plist, directory) = paths()?;
        reject_link(&plist)?;
        reject_link(&directory)?;
        if plist.exists() {

            return Err("같은 LaunchAgent가 이미 있습니다. 기존 job과 설정을 먼저 확인하세요".into());

        }
        fs::create_dir_all(plist.parent().ok_or("LaunchAgent 부모 경로가 없습니다")?)?;
        fs::create_dir_all(&directory)?;
        let executable = directory.join("w7bridge");
        reject_link(&executable)?;
        if executable.exists() {

            return Err("설치된 daemon binary가 이미 있습니다. 기존 job을 보존합니다".into());

        }
        let mut binary = NamedTempFile::new_in(&directory)?;
        let source = std::env::current_exe()?;
        io::copy(&mut fs::File::open(&source)?, &mut binary)?;
        binary.as_file().set_permissions(fs::metadata(&source)?.permissions())?;
        binary.as_file().sync_all()?;
        binary.persist_noclobber(&executable).map_err(|error| error.error)?;
        let log = directory.join("sync.log");
        reject_link(&log)?;
        let content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{label}</string>
<key>ProgramArguments</key><array><string>{}</string><string>sync</string><string>--config</string><string>{}</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
<key>ThrottleInterval</key><integer>5</integer><key>ProcessType</key><string>Background</string>
<key>StandardOutPath</key><string>{}</string><key>StandardErrorPath</key><string>{}</string>
</dict></plist>
"#,
            string(&executable)?,
            string(&config)?,
            string(&log)?,
            string(&log)?
        );
        let mut temporary = NamedTempFile::new_in(plist.parent().ok_or("LaunchAgent 부모 경로가 없습니다")?)?;
        temporary.write_all(content.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&plist).map_err(|error| error.error)?;
        let status = Command::new("/bin/launchctl").args(["bootstrap", &domain()?]).arg(&plist).status()?;
        if !status.success() {

            return Err("LaunchAgent 파일은 설치되었지만 bootstrap에 실패했습니다. launchctl 오류와 로그인 session을 확인하세요".into());

        }
        Ok(())

    }

    pub fn control(action: &str) -> Result<(), Failure> {

        let (plist, directory) = paths()?;
        reject_link(&plist)?;
        reject_link(&directory)?;
        let text = fs::read_to_string(&plist)?;
        if !text.contains(&format!("<key>Label</key><string>{label}</string>"))
            || !text.contains(&format!(
                "<array><string>{}</string><string>sync</string>",
                string(&directory.join("w7bridge"))?
            ))
        {

            return Err("이 w7bridge가 설치한 LaunchAgent 형식이 아닙니다".into());

        }
        let domain = domain()?;
        let mut command = Command::new("/bin/launchctl");
        match action {

            "start" => {

                command.args(["bootstrap", &domain]).arg(plist);

            }
            "stop" => {

                command.args(["bootout", &format!("{domain}/{label}")]);

            }
            "background-status" => {

                command.args(["print", &format!("{domain}/{label}")]);

            }
            "uninstall" => {

                let loaded = Command::new("/bin/launchctl")
                    .args(["print", &format!("{domain}/{label}")])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()?
                    .success();
                if loaded
                    && !Command::new("/bin/launchctl")
                        .args(["bootout", &format!("{domain}/{label}")])
                        .status()?
                        .success()
                {

                    return Err("LaunchAgent 종료에 실패했습니다. 설치 파일을 보존합니다".into());

                }
                fs::remove_file(&plist)?;
                let executable = directory.join("w7bridge");
                reject_link(&executable)?;
                fs::remove_file(executable)?;
                return Ok(());

            }
            _ => return Err("지원하지 않는 LaunchAgent 작업입니다".into()),

        }
        if !command.status()?.success() {

            return Err("LaunchAgent 작업에 실패했습니다. launchctl 출력을 확인하세요".into());

        }
        Ok(())

    }

}

/*! CLI 진입과 설치 옵션을 dispatch해. */

use crate::{
    InstallError,
    config::paths::InstallOptions,
    filesystem::copy::{file_error, install_files},
};
use std::{ffi::OsString, path::PathBuf};
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "install") {

        return run_install(&args[1..]).map_err(Into::into);

    }
    if args.first().is_some_and(|arg| arg == "connect") {

        return crate::connection::client::run(&args[1..]).await.map_err(Into::into);

    }
    if args.first().is_some_and(|arg| arg == "sync") {

        return super::lifecycle::run_sync(&args[1..]).await;

    }
    if args.first().is_some_and(|arg| arg == "service") {

        return run_service(&args[1..]);

    }
    if matches!(args.as_slice(), [flag] if flag == "relay") {

        #[cfg(windows)]
        {

            return crate::server::runtime::service::relay().await;

        }
        #[cfg(not(windows))]
        {

            return Err("relay는 Windows service에서만 지원합니다".into());

        }

    }
    let path = match args.as_slice() {

        [flag] if flag == "--help" || flag == "-h" => {

            println!(concat!(
                "w7bridge — Windows 프로젝트용 MCP 서버\n\n",
                "사용법: w7bridge --config <설정 경로>\n",
                "        w7bridge install [--dir <Windows 설치 경로>]\n",
                "        w7bridge connect --host <SSH 대상>\n\n",
                "  --config <경로>  로컬 TOML 설정으로 stdio 서버 실행\n",
                "  install          Windows에 실행 파일과 설정 영구 설치\n",
                "  connect          Windows MCP 연결 확인 후 Codex에 등록\n",
                "  sync             프로젝트 source와 context 파일을 양방향 sync\n",
                "  service          Windows 자동 시작 service 설치/실행\n",
                "  relay            SSH stdio를 Windows service에 전달\n",
                "  --help           사용법 표시\n  --version        버전 표시\n\n",
                "세부 옵션은 install --help, connect --help, sync --help로 확인하세요.\n",
                "서버 모드 stdout은 MCP 메시지 전용입니다."
            ));
            return Ok(());

        }
        [flag] if flag == "--version" || flag == "-V" => {

            println!("w7bridge {}", env!("CARGO_PKG_VERSION"));
            return Ok(());

        }
        [flag, path] if flag == "--config" && !path.is_empty() => PathBuf::from(path),
        _ => return Err("인자가 올바르지 않습니다. --help로 사용법을 확인하세요".into()),

    };

    super::lifecycle::serve(&path).await

}

fn run_install(args: &[OsString]) -> Result<(), InstallError> {

    if matches!(args, [flag] if flag == "--help" || flag == "-h") {

        println!(concat!(
            "Windows 영구 설치 — 실행 파일과 설정을 고정 경로에 배치\n\n",
            "사용법: w7bridge install [옵션]\n\n",
            "  --dir <경로>     설치 디렉터리 (기본: C:/w7bridge)\n",
            "  --config <경로>  최초 설치에 사용할 로컬 TOML (생략하면 빈 registry)\n",
            "  --update         기존 실행 파일 갱신, 기존 설정 보존\n\n",
            "Windows에서 실행하세요. 설치 경로는 공백 없는 절대 drive 경로여야 합니다.\n",
            "SSH 연결마다 서버가 실행됩니다. Windows service와 자동 시작 작업은 만들지 않습니다.\n",
            "OpenSSH, 계정 권한과 firewall은 별도로 준비하세요. PATH와 시스템 설정은 변경하지 않습니다."
        ));
        return Ok(());

    }
    let options = InstallOptions::parse(args)?;
    if !cfg!(windows) {

        return Err(InstallError::Platform);

    }
    let source = std::env::current_exe().map_err(|source| file_error("현재 실행 파일 확인", source))?;
    install_files(&options, &source)?;
    eprintln!("영구 설치가 끝났습니다: {}", options.directory.display());
    eprintln!("설정: {}", options.directory.join("w7bridge.toml").display());
    eprintln!("SSH 계정에 설치 파일 읽기·실행 권한과 등록된 프로젝트 권한을 부여하세요");
    eprintln!("Mac에서 w7bridge connect --host <SSH 대상>으로 연결하세요");
    Ok(())

}

fn run_service(args: &[OsString]) -> Result<(), crate::protocol::types::Failure> {

    if matches!(args, [flag] if flag == "--help" || flag == "-h") {

        println!(
            "사용법: w7bridge service install --config <Windows 설정>\nSCM 실행: w7bridge service --config <Windows 설정>\nservice.allowed_sid와 service 계정의 프로젝트·toolchain 접근 권한을 먼저 준비하세요"
        );
        return Ok(());

    }
    #[cfg(windows)]
    {

        match args {

            [install, flag, path] if install == "install" && flag == "--config" => {

                crate::server::runtime::service::install(&PathBuf::from(path))

            }
            [flag, path] if flag == "--config" => crate::server::runtime::service::dispatch(PathBuf::from(path)),
            _ => Err("service --help로 인자를 확인하세요".into()),

        }

    }
    #[cfg(not(windows))]
    {

        Err("Windows service는 Windows에서 설치/실행하세요".into())

    }

}

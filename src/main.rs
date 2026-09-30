use std::{path::PathBuf, process::ExitCode};

use tokio_util::sync::CancellationToken;
use w7bridge::{Bridge, Config, serve_stdio};

#[tokio::main]
async fn main() -> ExitCode {

    match run().await {

        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {

            eprintln!("오류: {error}");
            ExitCode::FAILURE

        }

    }

}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let path = match args.as_slice() {

        [flag] if flag == "--help" || flag == "-h" => {

            println!(
                "w7bridge — Windows 프로젝트용 MCP 서버\n\n사용법: w7bridge --config <설정 경로>\n\n  --config <경로>  로컬 TOML 설정으로 stdio 서버 실행\n  --help           사용법 표시\n  --version        버전 표시\n\n서버 모드 stdout은 MCP 메시지 전용입니다."
            );
            return Ok(());

        }
        [flag] if flag == "--version" || flag == "-V" => {

            println!("w7bridge {}", env!("CARGO_PKG_VERSION"));
            return Ok(());

        }
        [flag, path] if flag == "--config" && !path.is_empty() => PathBuf::from(path),
        _ => return Err("인자가 올바르지 않습니다. --help로 사용법을 확인하세요".into()),

    };

    let shutdown = CancellationToken::new();
    let bridge = Bridge::new(Config::load(&path)?, shutdown.clone())?;
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

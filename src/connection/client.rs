/*! 연결 확인 후 로컬 Codex에 등록해. */

use super::{session::probe, transport::Options};
use crate::ConnectError;
use serde::Deserialize;
use std::{ffi::OsString, process::Stdio, time::Duration};
use tokio::process::Command;
pub(crate) async fn run(args: &[OsString]) -> Result<(), ConnectError> {

    if matches!(args, [flag] if flag == "--help" || flag == "-h") {

        println!(concat!(
            "Connect MCP — Windows 연결 확인 후 Codex에 등록\n\n",
            "사용법: w7bridge connect --host <SSH alias 또는 user@host> [옵션]\n\n",
            "  --executable <경로>  Windows .exe (기본: C:/w7bridge/w7bridge.exe)\n",
            "  --config <경로>      Windows TOML (기본: C:/w7bridge/w7bridge.toml)\n",
            "  --name <이름>        Codex MCP 이름 (기본: w7bridge)\n",
            "  --port <번호>        SSH port (생략하면 SSH 설정 사용)\n",
            "  --identity <경로>    로컬 SSH key (생략하면 SSH 설정 사용)\n",
            "  --timeout <초>       연결 확인 한도 1..=120초 (기본: 30)\n",
            "  --check              연결만 확인, Codex 설정 유지\n",
            "  --service            Windows service의 local pipe에 연결\n",
            "  --print-config       연결 없이 Codex TOML만 출력\n\n",
            "기본 모드는 codex CLI가 필요합니다. 기존 이름은 덮어쓰지 않습니다.\n",
            "Windows OpenSSH 기본 shell은 cmd.exe, 원격 경로는 공백 없이 준비하세요.\n",
            "SSH key 인증과 신뢰한 host key를 먼저 준비하세요. 등록 후 Codex에서 새 채팅을 여세요."
        ));
        return Ok(());

    }
    let options = Options::parse(args)?;
    if options.print_config {

        print!("{}", options.codex_config()?);
        return Ok(());

    }
    if !options.check {

        ensure_available(&options.name).await?;

    }
    let mut ssh = Command::new("ssh");
    ssh.args(options.ssh_args());
    eprintln!("Windows MCP 연결을 확인합니다: {}", options.host);
    probe(ssh, Duration::from_secs(options.timeout_seconds)).await?;
    eprintln!("MCP 초기화, 도구 발견과 list_projects 호출을 확인했습니다");
    if !options.check {

        // 연결을 확인한 뒤에만 client 설정에 등록해. 원격 명령 실행 권한은 추가하지 않아.
        let status = Command::new("codex")
            .args(["mcp", "add", &options.name, "--", "ssh"])
            .args(options.ssh_args())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .status()
            .await
            .map_err(|source| ConnectError::Process { operation: "Codex MCP 등록", source })?;
        if !status.success() {

            return Err(ConnectError::CodexAdd);

        }
        eprintln!("Codex MCP 등록이 끝났습니다. 새 채팅에서 연결하세요: {}", options.name);

    }
    Ok(())

}

async fn ensure_available(name: &str) -> Result<(), ConnectError> {

    let output = Command::new("codex")
        .args(["mcp", "list", "--json"])
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|source| ConnectError::Process { operation: "Codex MCP 목록", source })?;
    if !output.status.success() {

        return Err(ConnectError::CodexList);

    }
    #[derive(Deserialize)]
    struct Entry {

        name: String,

    }
    let entries: Vec<Entry> = serde_json::from_slice(&output.stdout).map_err(|_| ConnectError::CodexList)?;
    if entries.iter().any(|entry| entry.name == name) {

        return Err(ConnectError::Exists);

    }
    Ok(())

}

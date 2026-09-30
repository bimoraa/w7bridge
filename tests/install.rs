use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {

    std::env::var_os("W7BRIDGE_TEST_BINARY").map(PathBuf::from).unwrap_or_else(|| env!("CARGO_BIN_EXE_w7bridge").into())

}

#[test]
fn install_help_does_not_require_windows_or_write_files() {

    let output = Command::new(binary()).args(["install", "--help"]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("C:/w7bridge"));
    assert!(output.stderr.is_empty());

}

#[cfg(not(windows))]
#[test]
fn install_is_rejected_on_other_platforms() {

    let output = Command::new(binary()).arg("install").output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Windows에서 실행하세요"));

}

#[cfg(windows)]
#[tokio::test]
async fn native_install_runs_mcp_and_update_preserves_config() {

    use rmcp::{ServiceExt, model::CallToolRequestParams};
    use std::fs;
    use tokio::{
        process::Command as AsyncCommand,
        time::{Duration, timeout},
    };

    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("installed");
    let args = ["install", "--dir", directory.to_str().unwrap()];
    let output = Command::new(binary()).args(args).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let executable = directory.join("w7bridge.exe");
    let config = directory.join("w7bridge.toml");
    let contents = "# native custom config\r\nversion = 1\r\nprojects = []\r\n[codex]\r\nenabled = false\r\n";
    fs::write(&config, contents).unwrap();
    timeout(Duration::from_secs(10), async {

        let mut child = AsyncCommand::new(&executable)
            .arg("--config")
            .arg(&config)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let client = ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap())).await.unwrap();
        let listing = client.call_tool(CallToolRequestParams::new("list_projects")).await.unwrap();
        assert_eq!(listing.structured_content.unwrap()["projects"], serde_json::json!([]));
        client.cancel().await.unwrap();
        assert!(child.wait().await.unwrap().success());

    })
    .await
    .unwrap();
    let repeated = Command::new(binary()).args(args).output().unwrap();
    assert!(!repeated.status.success());
    let updated = Command::new(binary()).args(args).arg("--update").output().unwrap();
    assert!(updated.status.success(), "{}", String::from_utf8_lossy(&updated.stderr));
    assert_eq!(fs::read_to_string(config).unwrap(), contents);
    assert!(Command::new(executable).arg("--version").output().unwrap().status.success());

}

use serde_json::{Value, json};
use tempfile::tempdir;

#[path = "fixtures/client.rs"]
mod client;
#[path = "fixtures/process.rs"]
mod fixture;

use client::Client;

#[tokio::test]
async fn discovery_execution_and_input_policy_work_over_stdio() {

    let root = tempdir().unwrap();
    let mut client = Client::start(root.path(), "output", json!({})).await;
    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await;
    let discovery = client.response(2).await;
    let tools = discovery["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "list_projects");
    assert_eq!(tools[1]["name"], "run_command");
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
    assert_eq!(tools[1]["annotations"]["destructiveHint"], true);
    assert_eq!(tools[1]["inputSchema"]["additionalProperties"], false);

    let listing = client.call(3, "list_projects", json!({})).await;
    assert_eq!(
        listing["result"]["structuredContent"],
        json!({ "projects": [{ "id": "sample", "commands": ["probe", "test"] }] })
    );
    let execution = client.call(4, "run_command", json!({ "project_id": "sample", "command": "test" })).await;
    assert_eq!(execution["result"]["isError"], false);
    let output = &execution["result"]["structuredContent"];
    assert_eq!(output["exit_code"], 0);
    assert_eq!(output["status"], "completed");
    let stdout = output["stdout"].as_str().unwrap();
    let data: Value = serde_json::from_str(stdout.lines().find(|line| line.starts_with('{')).unwrap()).unwrap();
    let cwd: std::path::PathBuf = serde_json::from_value(data["cwd"].clone()).unwrap();
    assert_eq!(cwd.canonicalize().unwrap(), root.path().canonicalize().unwrap());
    assert_eq!(data["configured_env"], "output");
    assert_eq!(data["secret_inherited"], false);
    assert!(output["stderr"].as_str().unwrap().contains("오류 출력 확인"));

    for (id, tool, args) in [
        (5, "run_command", json!({ "project_id": "sample", "command": "test", "args": ["임의 인자"] })),
        (6, "run_command", json!({ "project_id": "sample" })),
        (7, "list_projects", json!({ "root": "임의 경로" })),
        (8, "missing", json!({})),
    ] {

        assert_eq!(client.call(id, tool, args).await["error"]["code"], -32602);

    }

    let unknown = client.call(9, "run_command", json!({ "project_id": "sample", "command": "missing" })).await;
    assert_eq!(unknown["result"]["isError"], true);
    client.close().await;

}

#[test]
fn cli_diagnostics_are_korean_and_do_not_pollute_protocol_stdout() {

    let help = std::process::Command::new(env!("CARGO_BIN_EXE_w7bridge")).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout).unwrap().contains("사용법"));
    let invalid = std::process::Command::new(env!("CARGO_BIN_EXE_w7bridge")).arg("--unknown").output().unwrap();
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8(invalid.stderr).unwrap().contains("인자가 올바르지 않습니다"));

}

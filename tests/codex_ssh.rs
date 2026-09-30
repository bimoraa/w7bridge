#![cfg(unix)]

use std::{process::Stdio, time::Duration};

use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::json;
use tokio::{process::Command, time::timeout};

#[tokio::test]
#[ignore = "실제 Windows SSH host와 MCP command를 지정한 읽기 전용 검증"]
async fn live_codex_projects_are_returned_over_ssh_mcp() {

    let host = std::env::var("W7BRIDGE_LIVE_SSH_HOST").expect("SSH host가 필요해");
    let command = std::env::var("W7BRIDGE_LIVE_SSH_COMMAND").expect("MCP command가 필요해");
    let mut process = Command::new("ssh")
        .args(["-T", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes", &host, &command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let input = process.stdout.take().unwrap();
    let output = process.stdin.take().unwrap();
    timeout(Duration::from_secs(30), async {

        let client = ().serve((input, output)).await.unwrap();
        let tools = client.list_tools(None).await.unwrap();
        assert!(tools.tools.iter().any(|tool| tool.name == "list_projects"));
        let listing = client
            .call_tool(CallToolRequestParams::new("list_projects").with_arguments(Default::default()))
            .await
            .unwrap();
        assert_ne!(listing.is_error, Some(true));
        let content = listing.structured_content.unwrap();
        assert_eq!(content["codex"]["status"], "ready");
        assert_eq!(content["codex"]["source"], "codex_database");
        let projects = content["projects"].as_array().unwrap();
        assert!(projects.iter().any(|project| project["name"] == "fatomic"));
        assert!(projects.iter().all(|project| project["commands"] == json!([])));
        println!("{}", json!({"count": projects.len(), "projects": projects}));
        client.cancel().await.unwrap();
        assert!(process.wait().await.unwrap().success());

    })
    .await
    .expect("Windows MCP 검증 시간이 초과됐어");

}

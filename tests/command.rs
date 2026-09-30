use std::{path::Path, time::Duration};

use serde_json::{Value, json};
use tempfile::tempdir;
use tokio::time::{sleep, timeout};

#[path = "fixtures/client.rs"]
mod client;
#[path = "fixtures/process.rs"]
mod fixture;

use client::Client;

#[tokio::test]
async fn nonzero_exit_and_output_truncation_are_visible() {

    for mode in ["failure", "flood"] {

        let root = tempdir().unwrap();
        let mut client = Client::start(root.path(), mode, json!({ "output_bytes": 1024 })).await;
        let response = client.call(2, "run_command", json!({ "project_id": "sample", "command": "test" })).await;
        let output = &response["result"]["structuredContent"];

        if mode == "failure" {

            assert_eq!(response["result"]["isError"], true);
            assert_eq!(output["exit_code"], 7);
            assert_eq!(output["success"], false);

        } else {

            assert_eq!(response["result"]["isError"], false);
            assert_eq!(output["stdout"].as_str().unwrap().len(), 1024);
            assert_eq!(output["stderr"].as_str().unwrap().len(), 1024);
            assert_eq!(output["stdout_truncated"], true);
            assert_eq!(output["stderr_truncated"], true);

        }

        client.close().await;

    }

}

async fn ready(root: &Path) {

    timeout(Duration::from_secs(10), async {

        while !root.join("ready").exists() {

            sleep(Duration::from_millis(10)).await;

        }

    })
    .await
    .expect("프로세스 트리가 시작되지 않았어");

}

#[tokio::test]
async fn timeout_cancellation_and_disconnect_stop_descendants() {

    for mode in ["timeout", "cancel", "disconnect"] {

        let root = tempdir().unwrap();
        let limits = if mode == "timeout" { json!({ "timeout_seconds": 1 }) } else { json!({}) };
        let mut client = Client::start(root.path(), "tree", limits).await;
        client
            .send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "run_command", "arguments": { "project_id": "sample", "command": "test" }
        } }))
            .await;
        ready(root.path()).await;

        if mode == "timeout" {

            let output = client.response(2).await;
            assert_eq!(output["result"]["isError"], true);
            assert_eq!(output["result"]["structuredContent"]["status"], "timed_out");
            assert_eq!(output["result"]["structuredContent"]["exit_code"], Value::Null);

        } else if mode == "cancel" {

            let busy = client.call(3, "run_command", json!({ "project_id": "sample", "command": "test" })).await;
            assert_eq!(busy["result"]["isError"], true);
            client
                .send(json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": {
                "requestId": 2, "reason": "테스트 취소"
            } }))
                .await;
            // SDK는 취소한 request의 응답을 버려. 새 명령 실행으로 slot 반환을 확인해.
            timeout(Duration::from_secs(10), async {

                for id in 4.. {

                    let probe =
                        client.call(id, "run_command", json!({ "project_id": "sample", "command": "probe" })).await;
                    if probe["result"]["isError"] == false {

                        assert_eq!(probe["result"]["structuredContent"]["success"], true);
                        return;

                    }
                    assert!(probe["result"]["content"][0]["text"].as_str().unwrap().contains("실행 한도"));
                    sleep(Duration::from_millis(10)).await;

                }

            })
            .await
            .expect("취소 후 실행 slot이 반환되지 않았어");

        }

        client.close().await;
        sleep(Duration::from_millis(2300)).await;
        assert!(!root.path().join("survived").exists(), "자식 프로세스가 살아 있어: {mode}");

    }

}

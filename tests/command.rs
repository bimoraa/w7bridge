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
async fn run_command_yields_a_live_handle_and_cursor_before_completion( ) {

    let root = tempdir().unwrap();
    let mut client = Client::start(root.path(), "stream", json!({})).await;
    let response = client.call(2, "run_command", json!({"project_id":"sample","command":"test"})).await;
    assert_eq!(response["result"]["isError"], false);
    let output = &response["result"]["structuredContent"];
    assert_eq!(output["status"], "running");
    assert!(!root.path().join("stream_done").exists());
    let id = output["process_id"].as_str().unwrap().to_owned();
    let mut cursor = output["next_cursor"].as_u64().unwrap();
    let mut text =
        output["events"].as_array().unwrap().iter().filter_map(|event| event["text"].as_str()).collect::<String>();
    for request in 3.. {

        let response = client
            .call(
                request,
                "read_process_output",
                json!({"project_id":"sample","process_id":id,"cursor":cursor,"wait_seconds":30}),
            )
            .await;
        let output = &response["result"]["structuredContent"];
        assert_eq!(response["result"]["isError"], false);
        for event in output["events"].as_array().unwrap() {

            assert!(event["cursor"].as_u64().unwrap() > cursor);
            text.push_str(event["text"].as_str().unwrap());

        }
        cursor = output["next_cursor"].as_u64().unwrap();
        if output["status"] == "stopped" {

            assert_eq!(output["result"]["exit_code"], 0);
            break;

        }

    }
    for marker in ["OUT_1", "OUT_2", "ERR_1", "ERR_2"] {

        assert!(text.contains(marker), "{text}");

    }
    client.close().await;

}

#[tokio::test]
async fn waiting_command_streams_unterminated_stdout_and_stderr_before_its_result( ) {

    let root = tempdir().unwrap();
    let mut client = Client::start(root.path(), "stream", json!({})).await;
    client.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_command","arguments":{"project_id":"sample","command":"test","wait":true},"_meta":{"progressToken":"command-stream"}}})).await;
    let mut progress = 0.0;
    let mut text = String::new();
    let mut early_stdout = false;
    let mut early_stderr = false;
    let result = timeout(Duration::from_secs(10), async {

        loop {

            let message = client.message().await;
            if message["id"] == 2 {

                break message;

            }
            assert_eq!(message["method"], "notifications/progress");
            assert_eq!(message["params"]["progressToken"], "command-stream");
            let next = message["params"]["progress"].as_f64().unwrap();
            assert!(next > progress);
            progress = next;
            let data = &message["params"]["_meta"]["io.w7bridge/output"];
            assert!(data["process_id"].is_string());
            assert_eq!(data["project_id"], "sample");
            for event in data["events"].as_array().unwrap() {

                let chunk = event["text"].as_str().unwrap();
                text.push_str(chunk);
                if !root.path().join("stream_done").exists() {

                    early_stdout |= chunk.contains("OUT_1");
                    early_stderr |= chunk.contains("ERR_1");

                }

            }

        }

    })
    .await
    .unwrap();
    assert!(early_stdout && early_stderr, "출력이 종료 전에 도착하지 않았어: {text}");
    for marker in ["OUT_1", "OUT_2", "ERR_1", "ERR_2"] {

        assert!(text.contains(marker), "{text}");

    }
    assert_eq!(result["result"]["structuredContent"]["exit_code"], 0);
    client.close().await;

}

#[tokio::test]
async fn nonzero_exit_and_output_truncation_are_visible() {

    for mode in ["failure", "flood"] {

        let root = tempdir().unwrap();
        let mut client = Client::start(root.path(), mode, json!({ "output_bytes": 1024 })).await;
        let response =
            client.call(2, "run_command", json!({ "project_id": "sample", "command": "test", "wait": true })).await;
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
            "name": "run_command", "arguments": { "project_id": "sample", "command": "test", "wait": true }
        } }))
            .await;
        ready(root.path()).await;

        if mode == "timeout" {

            let output = client.response(2).await;
            assert_eq!(output["result"]["isError"], true);
            assert_eq!(output["result"]["structuredContent"]["status"], "timed_out");
            assert_eq!(output["result"]["structuredContent"]["exit_code"], Value::Null);

        } else if mode == "cancel" {

            let busy =
                client.call(3, "run_command", json!({ "project_id": "sample", "command": "test", "wait": true })).await;
            assert_eq!(busy["result"]["isError"], true);
            client
                .send(json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": {
                "requestId": 2, "reason": "테스트 취소"
            } }))
                .await;
            // SDK는 취소한 request의 응답을 버려. 새 명령 실행으로 slot 반환을 확인해.
            timeout(Duration::from_secs(10), async {

                for id in 4.. {

                    let probe = client
                        .call(id, "run_command", json!({ "project_id": "sample", "command": "probe", "wait": true }))
                        .await;
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

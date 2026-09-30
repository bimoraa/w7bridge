use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ContentBlock},
};
use serde_json::json;
use std::{process::Stdio, time::Duration};
use tempfile::tempdir;
use tokio::{process::Command, time::timeout};
use tokio_util::sync::CancellationToken;
use w7bridge::{Bridge, Config};

#[tokio::test]
async fn screenshot_optin_and_input_policy_are_enforced_over_mcp() {

    let config = Config::parse("version = 1\nprojects = []").unwrap();
    let bridge = Bridge::new(config, CancellationToken::new()).unwrap();
    let (client, server) = tokio::io::duplex(8192);
    let task = tokio::spawn(async move { bridge.serve(server).await.unwrap().waiting().await.unwrap() });
    let client = ().serve(client).await.unwrap();
    assert!(!client.list_tools(None).await.unwrap().tools.iter().any(|tool| tool.name == "capture_screenshot"));
    let result = client
        .call_tool(CallToolRequestParams::new("capture_screenshot").with_arguments(Default::default()))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.structured_content.unwrap()["code"], "disabled");
    for arguments in [json!({ "path": "/tmp/user-controlled.png" }), json!({ "display": 0 }), json!({ "display": 17 })]
    {

        assert!(
            client
                .call_tool(
                    CallToolRequestParams::new("capture_screenshot")
                        .with_arguments(arguments.as_object().unwrap().clone())
                )
                .await
                .is_err()
        );

    }
    client.cancel().await.unwrap();
    timeout(Duration::from_secs(3), task).await.unwrap().unwrap();

}

#[cfg(any(target_os = "macos", windows))]
#[tokio::test]
#[ignore = "로그인한 native desktop과 screen recording 권한이 필요해"]
async fn native_screenshot_returns_png_over_real_stdio() {

    let root = tempdir().unwrap();
    let config = root.path().join("capture.toml");
    std::fs::write(&config, "version = 1\nprojects = []\n[screenshots]\nenabled = true\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_w7bridge"))
        .arg("--config")
        .arg(&config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let input = child.stdout.take().unwrap();
    let output = child.stdin.take().unwrap();
    let client = timeout(Duration::from_secs(5), ().serve((input, output))).await.unwrap().unwrap();
    assert!(client.list_tools(None).await.unwrap().tools.iter().any(|tool| tool.name == "capture_screenshot"));
    let result = timeout(
        Duration::from_secs(40),
        client.call_tool(
            CallToolRequestParams::new("capture_screenshot")
                .with_arguments(json!({ "display": 1 }).as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_ne!(result.is_error, Some(true), "{:?}", result.structured_content);
    let metadata = result.structured_content.unwrap();
    assert!(metadata["width"].as_u64().unwrap() > 0);
    assert!(metadata["height"].as_u64().unwrap() > 0);
    let ContentBlock::Image(image) = &result.content[0] else {

        panic!("PNG content block이 필요해");

    };
    assert_eq!(image.mime_type, "image/png");
    let bytes = STANDARD.decode(&image.data).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert!(bytes.len() > 100);
    assert!(bytes.len() <= 8 * 1024 * 1024);
    client.cancel().await.unwrap();
    assert!(timeout(Duration::from_secs(5), child.wait()).await.unwrap().unwrap().success());

}

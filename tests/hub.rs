use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::json;
use std::{process::Stdio, time::Duration};
use tempfile::tempdir;

#[tokio::test]
async fn local_hub_reports_offline_devices_and_preserves_unpaired_files( ) {

    let root = tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("source.rs"), b"offline source").unwrap();
    let settings = json!({"version":1,"interval_seconds":1,"pairs":[{"local_root":project,"remote_project":"sample","host":"127.0.0.1","port":9,"service":true,"expected_device_id":"offline-pc"}]});
    let config = root.path().join("pair.toml");
    std::fs::write(&config, toml::to_string(&settings).unwrap()).unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_w7bridge"))
        .args(["hub", "--config"])
        .arg(config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let input = child.stdout.take().unwrap();
    let output = child.stdin.take().unwrap();
    let client = ().serve((input, output)).await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        client.call_tool(
            CallToolRequestParams::new("list_projects").with_arguments(json!({}).as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let project = &result.structured_content.unwrap()["projects"][0];
    assert_eq!(project["id"], "offline-pc__sample");
    assert_eq!(project["status"]["device_online"], false);
    let result = client
        .call_tool(
            CallToolRequestParams::new("project_status")
                .with_arguments(json!({"project_id":"offline-pc__sample"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
    assert_eq!(result.structured_content.unwrap()["device_online"], false);
    let result =
        client
            .call_tool(CallToolRequestParams::new("run_command").with_arguments(
                json!({"project_id":"offline-pc__sample","command":"build"}).as_object().unwrap().clone(),
            ))
            .await
            .unwrap();
    assert_eq!(result.is_error, Some(true));
    let result = client
        .call_tool(
            CallToolRequestParams::new("project_status")
                .with_arguments(json!({"project_id":"unregistered"}).as_object().unwrap().clone()),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(root.path().join("project/source.rs")).unwrap(), b"offline source");
    client.cancel().await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), child.wait()).await.unwrap().unwrap();

}

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::tempdir;
use w7bridge::filesystem::digest;

#[path = "fixtures/client.rs"]
mod client;
use client::Client;

fn config( root: &Path, enabled: bool, ) -> PathBuf {

    let value = json!({"version":1, "codex":{"enabled":false}, "projects":[{
        "id":"sample", "root":root, "files":{"enabled":enabled, "context_files":["notes/context.md"]}
    }]});
    let path = root.join("settings.toml");
    fs::write(&path, toml::to_string(&value).unwrap()).unwrap();
    path

}

#[tokio::test]
async fn memory_stdio_round_trip_conflicts_and_restart_share_file_contents() {

    let root = tempdir().unwrap();
    let path = config(root.path(), true);
    let mut client = Client::start_config(&path).await;
    let missing = client.call(2, "read_memory", json!({"project_id":"sample"})).await;
    let data = &missing["result"]["structuredContent"];
    assert_eq!(data["exists"], false);
    assert_eq!(data["content"], json!(null));
    assert_eq!(data["sha256"], json!(null));
    assert!(!root.path().join("MEMORY.md").exists());
    let content = "결정: 파일 공유\r\nlanjut 🪟\r\n";
    let created =
        client.call(3, "update_memory", json!({"project_id":"sample", "content":content, "expected_hash":null})).await;
    assert_eq!(created["result"]["isError"], false);
    assert_eq!(created["result"]["structuredContent"]["sha256"], digest(content.as_bytes()));
    assert_eq!(created["result"]["structuredContent"]["bytes"], content.len());
    let read = client.call(4, "read_memory", json!({"project_id":"sample"})).await;
    assert_eq!(read["result"]["structuredContent"]["content"], content);
    let file = client.call(5, "read_file", json!({"project_id":"sample", "path":"MEMORY.md"})).await;
    let file = &file["result"]["structuredContent"];
    assert_eq!(file["sha256"], read["result"]["structuredContent"]["sha256"]);
    assert_eq!(STANDARD.decode(file["content_base64"].as_str().unwrap()).unwrap(), content.as_bytes());
    fs::write(root.path().join("MEMORY.md"), "editor changed this").unwrap();
    let stale = client
        .call(
            6,
            "update_memory",
            json!({"project_id":"sample", "content":"stale",
        "expected_hash":created["result"]["structuredContent"]["sha256"]}),
        )
        .await;
    assert_eq!(stale["result"]["structuredContent"]["code"], "conflict");
    assert_eq!(fs::read(root.path().join("MEMORY.md")).unwrap(), b"editor changed this");
    let fresh = client.call(7, "read_memory", json!({"project_id":"sample"})).await;
    let updated = client
        .call(
            8,
            "update_memory",
            json!({"project_id":"sample", "content":"saved context",
        "expected_hash":fresh["result"]["structuredContent"]["sha256"]}),
        )
        .await;
    assert_eq!(updated["result"]["isError"], false);
    let custom = client
        .call(
            9,
            "update_memory",
            json!({"project_id":"sample", "path":"notes/context.md",
        "content":"custom context", "expected_hash":null}),
        )
        .await;
    assert_eq!(custom["result"]["isError"], false);
    client.close().await;
    let mut restarted = Client::start_config(&path).await;
    let persisted = restarted.call(2, "read_memory", json!({"project_id":"sample"})).await;
    assert_eq!(persisted["result"]["structuredContent"]["content"], "saved context");
    let custom = restarted.call(3, "read_memory", json!({"project_id":"sample", "path":"notes/context.md"})).await;
    assert_eq!(custom["result"]["structuredContent"]["content"], "custom context");
    restarted.close().await;

}

#[tokio::test]
async fn memory_stdio_denies_unregistered_paths_invalid_data_and_missing_revision() {

    let root = tempdir().unwrap();
    let path = config(root.path(), true);
    let mut client = Client::start_config(&path).await;
    for (id, path) in [(2, "../MEMORY.md"), (3, "src/main.rs"), (4, "settings.toml"), (5, "target/MEMORY.md")] {

        let rejected = client
            .call(
                id,
                "update_memory",
                json!({"project_id":"sample", "path":path,
            "content":"blocked", "expected_hash":null}),
            )
            .await;
        assert_eq!(rejected["result"]["structuredContent"]["code"], "path");

    }
    let omitted = client.call(6, "update_memory", json!({"project_id":"sample", "content":"no version"})).await;
    assert_eq!(omitted["error"]["code"], -32602);
    let unknown = client.call(7, "read_memory", json!({"project_id":"unknown"})).await;
    assert_eq!(unknown["result"]["structuredContent"]["code"], "disabled");
    fs::write(root.path().join("MEMORY.md"), [0xff, 0xfe]).unwrap();
    let invalid = client.call(8, "read_memory", json!({"project_id":"sample"})).await;
    assert_eq!(invalid["result"]["structuredContent"]["code"], "data");
    assert_eq!(fs::read(root.path().join("MEMORY.md")).unwrap(), [0xff, 0xfe]);
    client.close().await;

}

#[tokio::test]
async fn memory_stdio_requires_explicit_file_permission() {

    let root = tempdir().unwrap();
    let mut client = Client::start(root.path(), "output", json!({})).await;
    for (id, tool, arguments) in [
        (2, "read_memory", json!({"project_id":"sample"})),
        (3, "update_memory", json!({"project_id":"sample", "content":"blocked", "expected_hash":null})),
    ] {

        let rejected = client.call(id, tool, arguments).await;
        assert_eq!(rejected["result"]["structuredContent"]["code"], "disabled");

    }
    assert!(!root.path().join("MEMORY.md").exists());
    client.close().await;

}

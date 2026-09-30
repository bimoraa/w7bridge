use super::*;
use crate::Config;
use std::fs;
use tempfile::tempdir;

fn registry(root: &std::path::Path, enabled: bool) -> Policy {

    let config =
        json!({ "version": 1, "projects": [{ "id": "sample", "root": root, "files": { "enabled": enabled } }] });
    Policy::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap().projects).unwrap()

}

async fn invoke(policy: &Policy, tool: &str, arguments: Value) -> Result<CallToolResult, ErrorData> {

    call(policy, Arc::new(Semaphore::new(2)), tool, arguments.as_object().unwrap().clone(), CancellationToken::new())
        .await

}

#[tokio::test]
async fn file_tools_require_versions_preserve_binary_contents_and_return_typed_failures() {

    let root = tempdir().unwrap();
    let policy = registry(root.path(), true);
    let content = [0, 255, 12, 128];
    let write = invoke(
        &policy,
        "write_file",
        json!({ "project_id": "sample", "path": "MEMORY.md",
        "content_base64": STANDARD.encode(content), "expected_hash": null }),
    )
    .await
    .unwrap();
    assert_eq!(write.is_error, Some(false));
    let hash = write.structured_content.unwrap()["sha256"].as_str().unwrap().to_owned();
    let read = invoke(&policy, "read_file", json!({ "project_id": "sample", "path": "MEMORY.md" })).await.unwrap();
    assert_eq!(read.structured_content.unwrap()["content_base64"], STANDARD.encode(content));
    let stale = invoke(
        &policy,
        "write_file",
        json!({ "project_id": "sample", "path": "MEMORY.md",
        "content_base64": null, "expected_hash": null }),
    )
    .await
    .unwrap();
    assert_eq!(stale.structured_content.unwrap()["code"], "conflict");
    assert_eq!(fs::read(root.path().join("MEMORY.md")).unwrap(), content);
    for arguments in [
        json!({ "project_id": "sample", "path": "new.md", "content_base64": "YQ==" }),
        json!({ "project_id": "sample", "path": "new.md", "expected_hash": null }),
    ] {

        assert!(invoke(&policy, "write_file", arguments).await.is_err());

    }
    let missing = invoke(&policy, "read_file", json!({ "project_id": "sample", "path": "missing.md" })).await.unwrap();
    assert_eq!(missing.structured_content.unwrap()["code"], "not_found");
    let deletion = invoke(
        &policy,
        "write_file",
        json!({ "project_id": "sample", "path": "MEMORY.md",
        "content_base64": null, "expected_hash": hash }),
    )
    .await
    .unwrap();
    assert_eq!(deletion.structured_content.unwrap()["sha256"], Value::Null);
    let disabled = registry(root.path(), false);
    assert_eq!(
        invoke(&disabled, "list_files", json!({ "project_id": "sample" })).await.unwrap().structured_content.unwrap()["code"],
        "disabled"
    );

}

#[tokio::test]
async fn active_server_config_is_excluded_from_listing_reading_and_writing() {

    let root = tempdir().unwrap();
    let path = root.path().join("server.toml");
    let config =
        json!({ "version": 1, "projects": [{ "id": "sample", "root": root.path(), "files": { "enabled": true } }] });
    let source = toml::to_string(&config).unwrap();
    fs::write(&path, &source).unwrap();
    fs::write(root.path().join("MEMORY.md"), "context").unwrap();
    let policy = Policy::new(Config::load(&path).unwrap().projects).unwrap();
    let listing =
        invoke(&policy, "list_files", json!({ "project_id": "sample" })).await.unwrap().structured_content.unwrap();
    assert_eq!(listing["files"].as_array().unwrap().len(), 1);
    assert_eq!(listing["files"][0]["path"], "MEMORY.md");
    assert_eq!(listing["root_key"].as_str().unwrap().len(), 64);
    for (tool, arguments) in [
        ("read_file", json!({ "project_id": "sample", "path": "server.toml" })),
        (
            "write_file",
            json!({ "project_id": "sample", "path": "server.toml", "content_base64": "", "expected_hash": null }),
        ),
    ] {

        let result = invoke(&policy, tool, arguments).await.unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.structured_content.unwrap()["code"], "path");

    }
    assert_eq!(fs::read_to_string(&path).unwrap(), source);

}

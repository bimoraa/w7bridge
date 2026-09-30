use super::*;
use crate::Config;
use std::{fs, path::Path};
use tempfile::tempdir;

fn policy( root: &Path, ) -> Policy {

    let config = json!({"version": 1, "projects": [{"id": "sample", "root": root, "files": {"enabled": true}}]});
    Policy::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap().projects).unwrap()

}

async fn invoke( policy: &Policy, name: &str, args: Value, ) -> Result<CallToolResult, ErrorData> {

    call(policy, Arc::new(Semaphore::new(2)), name, args.as_object().unwrap().clone(), CancellationToken::new()).await

}

#[tokio::test]
async fn strict_arguments_require_an_explicit_revision_and_utf8_content() {

    let root = tempdir().unwrap();
    let registry = policy(root.path());
    let schema = serde_json::to_value(definitions()).unwrap();
    assert_eq!(schema[1]["inputSchema"]["required"], json!(["project_id", "content", "expected_hash"]));
    assert_eq!(schema[1]["inputSchema"]["additionalProperties"], false);
    assert_eq!(schema[0]["annotations"]["readOnlyHint"], true);
    assert_eq!(schema[1]["annotations"]["destructiveHint"], true);
    for args in [
        json!({"project_id":"sample", "content":"no revision"}),
        json!({"project_id":"sample", "content":null, "expected_hash":null}),
        json!({"project_id":"sample", "content":"bad revision", "expected_hash":"bad"}),
        json!({"project_id":"sample", "content":"bad revision", "expected_hash":4}),
        json!({"project_id":"sample", "content":"no extra", "expected_hash":null, "root":"/"}),
    ] {

        assert!(invoke(&registry, "update_memory", args).await.is_err());

    }
    assert!(!root.path().join("MEMORY.md").exists());
    let large = invoke(
        &registry,
        "update_memory",
        json!({"project_id":"sample", "content":"x".repeat(1_048_577), "expected_hash":null}),
    )
    .await
    .unwrap();
    assert_eq!(large.structured_content.unwrap()["code"], "limit");

}

#[tokio::test]
async fn shared_file_capacity_cancellation_and_conflicts_preserve_content() {

    let root = tempdir().unwrap();
    let registry = policy(root.path());
    let slots = Arc::new(Semaphore::new(1));
    let permit = slots.clone().acquire_owned().await.unwrap();
    let args = json!({"project_id":"sample", "content":"first", "expected_hash":null});
    let busy =
        call(&registry, slots.clone(), "update_memory", args.as_object().unwrap().clone(), CancellationToken::new())
            .await
            .unwrap();
    assert_eq!(busy.structured_content.unwrap()["code"], "busy");
    drop(permit);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let cancelled =
        call(&registry, slots.clone(), "update_memory", args.as_object().unwrap().clone(), cancellation).await.unwrap();
    assert_eq!(cancelled.structured_content.unwrap()["code"], "cancelled");
    assert_eq!(slots.available_permits(), 1);
    assert!(!root.path().join("MEMORY.md").exists());
    let first = invoke(&registry, "update_memory", args).await.unwrap().structured_content.unwrap();
    fs::write(root.path().join("MEMORY.md"), "newer editor").unwrap();
    let stale = invoke(
        &registry,
        "update_memory",
        json!({"project_id":"sample", "content":"stale", "expected_hash":first["sha256"]}),
    )
    .await
    .unwrap();
    assert_eq!(stale.structured_content.unwrap()["code"], "conflict");
    assert_eq!(fs::read(root.path().join("MEMORY.md")).unwrap(), b"newer editor");

}

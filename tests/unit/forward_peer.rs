use super::*;
use crate::{
    Bridge, Config,
    filesystem::{FileSettings, FileStore, digest, hashes},
    sync::Checkpoint,
};
use std::{
    process::Stdio,
    sync::atomic::{AtomicBool, AtomicU64},
};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn forwarded_command_waits_for_its_own_peer_checkpoint( ) {

    let root = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let config = json!({"version":1,"codex":{"enabled":false},"projects":[{"id":"sample","root":root.path(),"requires_sync":true,"files":{"enabled":true},"commands":{"check":{"executable":executable,"args":["--list"]}}}]});
    let bridge =
        Bridge::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap(), CancellationToken::new()).unwrap();
    let coordinator = bridge.coordinator.clone();
    let (client, server) = tokio::io::duplex(65536);
    let server = tokio::spawn(async move { bridge.serve(server).await.unwrap().waiting().await.unwrap() });
    let (input, output) = tokio::io::split(client);
    let progress = ProgressClient::default();
    let notifications = progress.notifications.clone();
    let client = progress.serve((input, output)).await.unwrap();
    let child = tokio::process::Command::new(executable)
        .arg("--list")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let peer = "a".repeat(64);
    let remote = Remote {

        client,
        notifications,
        child,
        project: "sample".into(),
        root_key: None,
        generation: AtomicU64::new(0),
        lease: AtomicU64::new(6),
        last_heartbeat: std::sync::Mutex::new(std::time::Instant::now()),
        chunk_sync: true,
        peer_sync: AtomicBool::new(true),
        sync_peer: peer.clone(),
        bandwidth: 0,
        next_transfer: std::sync::Mutex::new(tokio::time::Instant::now()),
        expected_device: None,
        git_executable: None,
        git_baseline_name: String::new(),
        rpc_latency_ms: AtomicU64::new(0),

    };
    let files = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    {

        let (output, mut notifications) = tokio::sync::mpsc::channel(8);
        let forward = remote.forward_with_output(
            "run_command",
            json!({"command":"check","wait":true}).as_object().unwrap().clone(),
            CancellationToken::new(),
            Some(output),
            Some("windows__sample"),
        );
        tokio::pin!(forward);
        assert!(timeout(Duration::from_millis(30), &mut forward).await.is_err());
        assert_eq!(coordinator.status_peer("sample", &files, Some(&peer)).unwrap()["requested_generation"], 1);
        let hash = digest(&serde_json::to_vec(&hashes(&files.list().unwrap())).unwrap());
        coordinator
            .checkpoint_peer(
                "sample",
                &files,
                Checkpoint {

                    generation: 1,
                    status: "synced",
                    hash: &hash,
                    conflicts: vec![],
                    lease_seconds: 6,
                    latency_ms: None,

                },
                Some(&peer),
            )
            .unwrap();
        let result = timeout(Duration::from_secs(2), &mut forward).await.unwrap().unwrap();
        assert_ne!(result.is_error, Some(true));
        let result = result.structured_content.unwrap();
        assert!(result["process_id"].is_string());
        let notification = notifications.try_recv().expect("SDK token에 해당하는 progress가 전달되어야 해");
        assert_eq!(notification["project_id"], "windows__sample");
        assert_eq!(notification["process_id"], result["process_id"]);

    }
    let invalid = remote
        .forward("start_process", json!({"bad_argument":true}).as_object().unwrap().clone(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(invalid.structured_content.unwrap()["code"], "remote_rpc_error");
    remote.close().await;
    server.await.unwrap();

}

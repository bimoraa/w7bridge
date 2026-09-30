use super::coordination::*;
use crate::filesystem::FileSettings;
use crate::filesystem::FileStore;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn another_mac_cannot_confirm_requested_peers_sync_and_conflict_blocks_build( ) {

    let root = tempfile::tempdir().unwrap();
    let files = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    files.write("source.rs", Some(b"source"), None).unwrap();
    let hash = manifest(&files).unwrap();
    let coordinator = Coordinator::new();
    let first = "a".repeat(64);
    let second = "b".repeat(64);
    let checkpoint = |generation, status, conflicts| Checkpoint {

        generation,
        status,
        hash: &hash,
        conflicts,
        lease_seconds: 6,
        latency_ms: None,

    };
    coordinator.checkpoint_peer("p", &files, checkpoint(0, "synced", vec![]), Some(&first)).unwrap();
    let waiting = coordinator.wait_peer("p", &files, 2, CancellationToken::new(), Some(&second));
    tokio::pin!(waiting);
    assert!(tokio::time::timeout(Duration::from_millis(20), &mut waiting).await.is_err());
    coordinator.checkpoint_peer("p", &files, checkpoint(0, "synced", vec![]), Some(&first)).unwrap();
    assert!(tokio::time::timeout(Duration::from_millis(20), &mut waiting).await.is_err());
    coordinator.checkpoint_peer("p", &files, checkpoint(1, "synced", vec![]), Some(&second)).unwrap();
    assert_eq!(waiting.await.unwrap()["confirmed_generation"], 1);
    coordinator
        .checkpoint_peer("p", &files, checkpoint(0, "conflict", vec!["source.rs".into()]), Some(&first))
        .unwrap();
    assert_eq!(coordinator.status("p", &files).unwrap()["status"], "conflict");
    assert!(
        coordinator
            .wait_peer("p", &files, 1, CancellationToken::new(), Some(&second))
            .await
            .unwrap_err()
            .contains("conflict")
    );
    assert!(coordinator.wait_peer("p", &files, 1, CancellationToken::new(), None).await.is_err());

}

#[tokio::test]
async fn wait_cannot_create_more_peers_than_checkpoint_limit( ) {

    let root = tempfile::tempdir().unwrap();
    let files = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    let hash = manifest(&files).unwrap();
    let coordinator = Coordinator::new();
    for index in 0..16 {

        coordinator
            .checkpoint_peer(
                "p",
                &files,
                Checkpoint {

                    generation: 0,
                    status: "syncing",
                    hash: &hash,
                    conflicts: vec![],
                    lease_seconds: 6,
                    latency_ms: None,

                },
                Some(&format!("{index:064x}")),
            )
            .unwrap();

    }
    let error =
        coordinator.wait_peer("p", &files, 1, CancellationToken::new(), Some(&"f".repeat(64))).await.unwrap_err();
    assert!(error.contains("16"));

}

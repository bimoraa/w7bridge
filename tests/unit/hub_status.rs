use super::*;
use crate::filesystem::{FileSettings, FileStore};

#[tokio::test]
async fn online_device_keeps_local_git_conflict_and_watcher_error_visible( ) {

    let root = tempfile::tempdir().unwrap();
    let pair: Pair = serde_json::from_value(
        json!({"local_root":root.path(),"remote_project":"sample","host":"windows","service":true}),
    )
    .unwrap();
    let files = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    let mut session = Session::open_pair(files, pair.binding().unwrap()).unwrap();
    session.git_conflict("Git handoff conflict").unwrap();
    let peer =
        Peer { pair, remote: Mutex::new(None), status: Mutex::new(json!({"sync_host_error":"watcher stopped"})) };
    let hub = Hub { peers: Arc::new(BTreeMap::new()), tools: Arc::new(Vec::new()), shutdown: CancellationToken::new() };
    let mut status = json!({"device_online":true,"last_error":null});
    hub.local_status(&peer, &mut status).await;
    assert_eq!(status["device_online"], true);
    assert_eq!(status["last_local_sync"]["status"], "conflict");
    assert_eq!(status["sync_host_error"], "watcher stopped");
    assert_eq!(status["last_error"], "Git handoff conflict");

}

use super::*;
use crate::FileError;
use crate::filesystem::{FileSettings, FileStore, digest};
use std::fs;
use tempfile::tempdir;

struct LocalPeer(FileStore);
impl Peer for LocalPeer {

    async fn list(&self) -> Result<Vec<FileEntry>, SyncError> {

        Ok(self.0.list()?)

    }
    async fn read(&self, path: &str) -> Result<Vec<u8>, SyncError> {

        Ok(self.0.read(path)?)

    }
    async fn write(&self, path: &str, content: Option<&[u8]>, expected: Option<&str>) -> Result<(), SyncError> {

        self.0.write(path, content, expected)?;
        Ok(())

    }

}
fn store(root: &std::path::Path) -> FileStore {

    FileStore::new(root, FileSettings { enabled: true, ..Default::default() }).unwrap()

}

#[tokio::test]
async fn approved_source_policy_survives_watcher_restart_without_resetting_baseline( ) {

    let root = tempdir().unwrap();
    let peer = tempdir().unwrap();
    let pair: crate::config::Pair = serde_json::from_value(serde_json::json!({
        "local_root": root.path(), "remote_project":"sample", "host":"windows", "service":true
    }))
    .unwrap();
    let settings = FileSettings { enabled: true, source_dirs: vec!["apps/desktop/build".into()], ..Default::default() };
    let local = FileStore::new(root.path(), settings.clone()).unwrap();
    let remote = LocalPeer(FileStore::new(peer.path(), settings).unwrap());
    local.write("apps/desktop/build/source.rs", Some(b"source"), None).unwrap();
    let mut session = Session::open_pair(local.clone(), pair.binding().unwrap()).unwrap();
    session.bind_peer(digest(b"verified peer")).unwrap();
    crate::filesystem::watcher::cache_policy(&pair, &local).unwrap();
    assert_eq!(session.round(&remote).await.unwrap().status, Status::Synced);
    drop(session);
    let resumed = crate::filesystem::watcher::local_files(&pair).unwrap();
    let mut session = Session::open_pair(resumed.clone(), pair.binding().unwrap()).unwrap();
    assert_eq!(session.report().status, Status::Synced);
    assert!(session.bind_peer(digest(b"different peer")).is_err());
    resumed.write("apps/desktop/build/source.rs", Some(b"updated"), Some(&digest(b"source"))).unwrap();
    assert_eq!(session.round(&remote).await.unwrap().status, Status::Synced);
    assert_eq!(remote.0.read("apps/desktop/build/source.rs").unwrap(), b"updated");
    let name = format!("policy-{}.json", digest(pair.binding().unwrap().as_bytes()));
    local.save_metadata(&name, b"corrupt").unwrap();
    assert!(crate::filesystem::watcher::local_files(&pair).is_err());
    assert_eq!(fs::read(root.path().join("apps/desktop/build/source.rs")).unwrap(), b"updated");

}

#[tokio::test]
async fn initial_import_bidirectional_context_and_deletion_survive_reopen() {

    let mac = tempdir().unwrap();
    let windows = tempdir().unwrap();
    let local = store(mac.path());
    let remote = LocalPeer(store(windows.path()));
    remote.0.write("MEMORY.md", Some(b"windows context"), None).unwrap();
    remote.0.write("AGENTS.md", Some(b"rules"), None).unwrap();
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    assert_eq!(session.round(&remote).await.unwrap().status, Status::Synced);
    assert_eq!(local.read("MEMORY.md").unwrap(), b"windows context");
    let hash = digest(b"windows context");
    local.write("MEMORY.md", Some(b"mac context"), Some(&hash)).unwrap();
    session.round(&remote).await.unwrap();
    assert_eq!(remote.0.read("MEMORY.md").unwrap(), b"mac context");
    drop(session);
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    remote.0.write("AGENTS.md", None, Some(&digest(b"rules"))).unwrap();
    session.round(&remote).await.unwrap();
    assert!(!mac.path().join("AGENTS.md").exists());
    assert!(Session::open(local, "different peer".into()).is_err());

}

#[tokio::test]
async fn divergent_initial_files_and_simultaneous_edit_delete_preserve_both_sides() {

    let mac = tempdir().unwrap();
    let windows = tempdir().unwrap();
    let local = store(mac.path());
    let remote = LocalPeer(store(windows.path()));
    local.write("existing.md", Some(b"mac"), None).unwrap();
    remote.0.write("existing.md", Some(b"windows"), None).unwrap();
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    let report = session.round(&remote).await.unwrap();
    assert_eq!(report.status, Status::Conflict);
    let snapshot = report.conflicts[0].snapshot.clone();
    assert_eq!(local.load_metadata(&format!("{snapshot}-local.bin")).unwrap().unwrap(), b"mac");
    assert_eq!(local.load_metadata(&format!("{snapshot}-remote.bin")).unwrap().unwrap(), b"windows");
    remote.0.write("existing.md", Some(b"mac"), Some(&digest(b"windows"))).unwrap();
    assert_eq!(session.round(&remote).await.unwrap().status, Status::Synced);
    local.write("existing.md", None, Some(&digest(b"mac"))).unwrap();
    remote.0.write("existing.md", Some(b"new windows"), Some(&digest(b"mac"))).unwrap();
    assert_eq!(session.round(&remote).await.unwrap().status, Status::Conflict);
    assert!(!mac.path().join("existing.md").exists());
    assert_eq!(remote.0.read("existing.md").unwrap(), b"new windows");

}

struct LostReply {

    files: FileStore,
    lose: std::sync::atomic::AtomicBool,

}
impl Peer for LostReply {

    async fn list(&self) -> Result<Vec<FileEntry>, SyncError> {

        Ok(self.files.list()?)

    }
    async fn read(&self, path: &str) -> Result<Vec<u8>, SyncError> {

        Ok(self.files.read(path)?)

    }
    async fn write(&self, path: &str, content: Option<&[u8]>, expected: Option<&str>) -> Result<(), SyncError> {

        self.files.write(path, content, expected)?;
        if self.lose.swap(false, std::sync::atomic::Ordering::SeqCst) { Err(SyncError::Offline) } else { Ok(()) }

    }

}

#[tokio::test]
async fn lost_write_reply_then_source_changes_recovers_without_false_conflict() {

    let mac = tempdir().unwrap();
    let windows = tempdir().unwrap();
    let local = store(mac.path());
    let peer = LostReply { files: store(windows.path()), lose: true.into() };
    local.write("context.md", Some(b"first"), None).unwrap();
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    assert!(matches!(session.round(&peer).await, Err(SyncError::Offline)));
    drop(session);
    local.write("context.md", Some(b"second"), Some(&digest(b"first"))).unwrap();
    let mut session = Session::open(local, "pair".into()).unwrap();
    assert_eq!(session.round(&peer).await.unwrap().status, Status::Synced);
    assert_eq!(peer.files.read("context.md").unwrap(), b"second");

}

#[tokio::test]
async fn excluded_build_context_and_git_metadata_never_transfer() {

    let mac = tempdir().unwrap();
    let windows = tempdir().unwrap();
    let local = store(mac.path());
    let remote = LocalPeer(store(windows.path()));
    for directory in ["target", "node_modules", ".git", ".cache"] {

        std::fs::create_dir(windows.path().join(directory)).unwrap();
        std::fs::write(windows.path().join(directory).join("MEMORY.md"), b"skip").unwrap();

    }
    remote.0.write(".gitignore", Some(b"MEMORY.md"), None).unwrap();
    remote.0.write("MEMORY.md", Some(b"context"), None).unwrap();
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    session.round(&remote).await.unwrap();
    assert_eq!(local.list().unwrap().len(), 2);

}

#[tokio::test]
async fn root_identity_policy_change_and_corrupt_journal_fail_without_deleting_files() {

    let mac = tempdir().unwrap();
    let windows = tempdir().unwrap();
    let local = store(mac.path());
    let remote = LocalPeer(store(windows.path()));
    local.write("safe.md", Some(b"keep"), None).unwrap();
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    session.bind_peer(digest(b"root-one+policy-one")).unwrap();
    session.round(&remote).await.unwrap();
    assert!(matches!(session.bind_peer(digest(b"different-root")), Err(SyncError::State)));
    let mut state: serde_json::Value =
        serde_json::from_slice(&local.load_metadata("sync.json").unwrap().unwrap()).unwrap();
    state["pending"] =
        serde_json::json!({"path":"../escape","remote_target":true,"expected":null,"intended":digest(b"unsafe")});
    local.save_metadata("sync.json", &serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(matches!(Session::open(local.clone(), "pair".into()), Err(SyncError::State)));
    assert_eq!(local.read("safe.md").unwrap(), b"keep");
    assert_eq!(remote.0.read("safe.md").unwrap(), b"keep");

}

#[tokio::test]
async fn cross_peer_directory_case_and_file_directory_collisions_are_rejected_before_transfer() {

    for (left, right) in [("Src/a.md", "src/b.md"), ("a", "a/b")] {

        let mac = tempdir().unwrap();
        let windows = tempdir().unwrap();
        let local = store(mac.path());
        let remote = LocalPeer(store(windows.path()));
        local.write(left, Some(b"mac"), None).unwrap();
        remote.0.write(right, Some(b"windows"), None).unwrap();
        let mut session = Session::open(local.clone(), "pair".into()).unwrap();
        assert!(matches!(session.round(&remote).await, Err(SyncError::File(FileError::Path))));
        assert_eq!(local.read(left).unwrap(), b"mac");
        assert_eq!(remote.0.read(right).unwrap(), b"windows");

    }

}

#[tokio::test]
async fn unknown_write_outcome_and_new_peer_edit_stay_conflicted_after_restart() {

    let mac = tempdir().unwrap();
    let windows = tempdir().unwrap();
    let local = store(mac.path());
    let peer = LostReply { files: store(windows.path()), lose: true.into() };
    local.write("context.md", Some(b"sent"), None).unwrap();
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    assert!(matches!(session.round(&peer).await, Err(SyncError::Offline)));
    peer.files.write("context.md", Some(b"new remote"), Some(&digest(b"sent"))).unwrap();
    local.write("context.md", Some(b"new local"), Some(&digest(b"sent"))).unwrap();
    let report = session.round(&peer).await.unwrap();
    assert_eq!(report.status, Status::Conflict);
    drop(session);
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    assert_eq!(session.round(&peer).await.unwrap().status, Status::Conflict);
    assert_eq!(local.read("context.md").unwrap(), b"new local");
    assert_eq!(peer.files.read("context.md").unwrap(), b"new remote");

}

#[tokio::test]
async fn multiple_device_baselines_keep_each_peers_offline_changes_separate( ) {

    let mac = tempdir().unwrap();
    let windows_a = tempdir().unwrap();
    let windows_b = tempdir().unwrap();
    let local = store(mac.path());
    let first = LocalPeer(store(windows_a.path()));
    let second = LocalPeer(store(windows_b.path()));
    local.write("src.rs", Some(b"v1"), None).unwrap();
    let mut a = Session::open_pair(local.clone(), "windows-a".into()).unwrap();
    let mut b = Session::open_pair(local.clone(), "windows-b".into()).unwrap();
    a.round(&first).await.unwrap();
    b.round(&second).await.unwrap();
    local.write("src.rs", Some(b"offline mac"), Some(&digest(b"v1"))).unwrap();
    a.round(&first).await.unwrap();
    assert_eq!(first.0.read("src.rs").unwrap(), b"offline mac");
    assert_eq!(second.0.read("src.rs").unwrap(), b"v1");
    let mut b = Session::open_pair(local.clone(), "windows-b".into()).unwrap();
    b.round(&second).await.unwrap();
    assert_eq!(second.0.read("src.rs").unwrap(), b"offline mac");
    first.0.write("src.rs", Some(b"windows-a change"), Some(&digest(b"offline mac"))).unwrap();
    let mut a = Session::open_pair(local, "windows-a".into()).unwrap();
    a.round(&first).await.unwrap();
    b.round(&second).await.unwrap();
    assert_eq!(second.0.read("src.rs").unwrap(), b"windows-a change");

}

use super::ProjectMemory;
use crate::sync::{Peer, Session, Status, SyncError};
use crate::{
    Config, FileError,
    filesystem::{FileEntry, FileSettings, FileStore, digest},
    security::Policy,
};
use serde_json::json;
use std::{fs, path::Path};
use tempfile::tempdir;

fn policy( root: &Path, enabled: bool, contexts: &[&str], ) -> Policy {

    let config = json!({"version": 1, "projects": [{"id": "sample", "root": root,
        "files": {"enabled": enabled, "context_files": contexts}}]});
    Policy::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap().projects).unwrap()

}

#[test]
fn missing_context_and_utf8_round_trip_use_the_project_files() {

    let root = tempdir().unwrap();
    let memory = ProjectMemory::open(&policy(root.path(), true, &["notes/context.md"]), "sample").unwrap();
    let absent = memory.read("MEMORY.md").unwrap();
    assert!(!absent.document.state.exists);
    assert!(absent.document.content.is_none());
    assert!(absent.document.state.sha256.is_none());
    assert_eq!(absent.document.state.bytes, 0);
    assert_eq!(absent.context_files, ["AGENTS.md", "MEMORY.md", "PLANS.md", "notes/context.md"]);
    assert!(!root.path().join("MEMORY.md").exists());
    let content = "결정: 공유 파일\r\nlanjut dari Windows 🪟\r\n";
    let revision = memory.update("MEMORY.md", content, None).unwrap();
    assert_eq!(revision.state.bytes, content.len());
    assert_eq!(revision.state.sha256, Some(digest(content.as_bytes())));
    assert_eq!(memory.read("MEMORY.md").unwrap().document.content.as_deref(), Some(content));
    assert_eq!(fs::read(root.path().join("MEMORY.md")).unwrap(), content.as_bytes());
    memory.update("notes/context.md", "custom", None).unwrap();
    assert_eq!(fs::read(root.path().join("notes/context.md")).unwrap(), b"custom");

}

#[test]
fn external_edits_are_fresh_and_stale_updates_preserve_them() {

    let root = tempdir().unwrap();
    let registry = policy(root.path(), true, &[]);
    let memory = ProjectMemory::open(&registry, "sample").unwrap();
    let first = memory.update("MEMORY.md", "first", None).unwrap();
    fs::write(root.path().join("MEMORY.md"), "editor changed this").unwrap();
    assert!(matches!(memory.update("MEMORY.md", "stale", first.state.sha256.as_deref()), Err(FileError::Conflict)));
    assert!(matches!(memory.update("MEMORY.md", "create again", None), Err(FileError::Conflict)));
    let reopened = ProjectMemory::open(&registry, "sample").unwrap();
    let current = reopened.read("MEMORY.md").unwrap();
    assert_eq!(current.document.content.as_deref(), Some("editor changed this"));
    let empty = reopened.update("MEMORY.md", "", current.document.state.sha256.as_deref()).unwrap();
    assert!(empty.state.exists);
    assert_eq!(empty.state.sha256, Some(digest(b"")));
    assert_eq!(reopened.read("MEMORY.md").unwrap().document.content.as_deref(), Some(""));

}

#[test]
fn registry_paths_encoding_and_size_cannot_bypass_file_policy() {

    let root = tempdir().unwrap();
    assert!(matches!(ProjectMemory::open(&policy(root.path(), false, &[]), "sample"), Err(FileError::Disabled)));
    assert!(matches!(ProjectMemory::open(&policy(root.path(), true, &[]), "unknown"), Err(FileError::Disabled)));
    let memory = ProjectMemory::open(&policy(root.path(), true, &[]), "sample").unwrap();
    for path in ["src/main.rs", "../MEMORY.md", "target/MEMORY.md", ".env", ".w7bridge/sync.json", "memory.md"] {

        assert!(matches!(memory.read(path), Err(FileError::Path)), "{path}");
        assert!(matches!(memory.update(path, "blocked", None), Err(FileError::Path)), "{path}");

    }
    fs::write(root.path().join("MEMORY.md"), [0xff, 0xfe]).unwrap();
    assert!(matches!(memory.read("MEMORY.md"), Err(FileError::Data)));
    assert_eq!(fs::read(root.path().join("MEMORY.md")).unwrap(), [0xff, 0xfe]);
    assert!(matches!(memory.update("PLANS.md", &"x".repeat(1_048_577), None), Err(FileError::Limit)));
    assert!(!root.path().join("PLANS.md").exists());

}

#[cfg(unix)]
#[test]
fn symlink_context_cannot_read_or_modify_another_root() {

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("MEMORY.md"), "private").unwrap();
    std::os::unix::fs::symlink(outside.path().join("MEMORY.md"), root.path().join("MEMORY.md")).unwrap();
    let memory = ProjectMemory::open(&policy(root.path(), true, &[]), "sample").unwrap();
    assert!(matches!(memory.read("MEMORY.md"), Err(FileError::Path)));
    assert!(matches!(memory.update("MEMORY.md", "blocked", Some(&digest(b"private"))), Err(FileError::Path)));
    assert_eq!(fs::read(outside.path().join("MEMORY.md")).unwrap(), b"private");

}

struct FilePeer(FileStore);
impl Peer for FilePeer {

    async fn list( &self, ) -> Result<Vec<FileEntry>, SyncError> {

        Ok(self.0.list()?)

    }
    async fn read( &self, path: &str, ) -> Result<Vec<u8>, SyncError> {

        Ok(self.0.read(path)?)

    }
    async fn write( &self, path: &str, content: Option<&[u8]>, expected: Option<&str>, ) -> Result<(), SyncError> {

        self.0.write(path, content, expected)?;
        Ok(())

    }

}

#[tokio::test]
async fn memory_updates_sync_in_both_directions_and_preserve_conflicts() {

    let mac = tempdir().unwrap();
    let windows = tempdir().unwrap();
    fs::write(windows.path().join(".gitignore"), "MEMORY.md\n").unwrap();
    let left = ProjectMemory::open(&policy(mac.path(), true, &[]), "sample").unwrap();
    let right = ProjectMemory::open(&policy(windows.path(), true, &[]), "sample").unwrap();
    let settings = FileSettings { enabled: true, ..Default::default() };
    let local = FileStore::new(mac.path(), settings.clone()).unwrap();
    let peer = FilePeer(FileStore::new(windows.path(), settings).unwrap());
    let mut session = Session::open(local, "memory-pair".into()).unwrap();
    right.update("MEMORY.md", "windows context", None).unwrap();
    assert_eq!(session.round(&peer).await.unwrap().status, Status::Synced);
    let imported = left.read("MEMORY.md").unwrap();
    assert_eq!(imported.document.content.as_deref(), Some("windows context"));
    let baseline = left.update("MEMORY.md", "mac context", imported.document.state.sha256.as_deref()).unwrap();
    assert_eq!(session.round(&peer).await.unwrap().status, Status::Synced);
    assert_eq!(right.read("MEMORY.md").unwrap().document.content.as_deref(), Some("mac context"));
    left.update("MEMORY.md", "left edit", baseline.state.sha256.as_deref()).unwrap();
    right.update("MEMORY.md", "right edit", baseline.state.sha256.as_deref()).unwrap();
    assert_eq!(session.round(&peer).await.unwrap().status, Status::Conflict);
    assert_eq!(left.read("MEMORY.md").unwrap().document.content.as_deref(), Some("left edit"));
    assert_eq!(right.read("MEMORY.md").unwrap().document.content.as_deref(), Some("right edit"));

}

use super::*;
use crate::filesystem::FileSettings;
use tempfile::tempdir;

fn store( root: &std::path::Path, ) -> FileStore {

    FileStore::new(root, FileSettings { enabled: true, ..Default::default() }).unwrap()

}

#[test]
fn delta_resume_reopen_integrity_and_stale_commit_preserve_target( ) {

    let root = tempdir().unwrap();
    let files = store(root.path());
    let old = vec![1; 2 * 65536];
    files.write("large.bin", Some(&old), None).unwrap();
    let mut new = old.clone();
    new[65536..].fill(2);
    let prepared = files.prepare_transfer("large.bin", Some(&digest(&old)), &digest(&new), describe(&new)).unwrap();
    assert_eq!(prepared["missing"], json!([1]));
    let id = prepared["transfer_id"].as_str().unwrap();
    assert!(matches!(files.put_chunk(id, 1, &new[..65536]), Err(FileError::Data)));
    files.put_chunk(id, 1, &new[65536..]).unwrap();
    let reopened = store(root.path());
    let resumed = reopened.prepare_transfer("large.bin", Some(&digest(&old)), &digest(&new), describe(&new)).unwrap();
    assert_eq!(resumed["transfer_id"], id);
    assert_eq!(resumed["missing"], json!([]));
    assert_eq!(reopened.commit_transfer(id).unwrap()["sha256"], digest(&new));
    assert_eq!(reopened.read("large.bin").unwrap(), new);
    assert_eq!(
        reopened.prepare_transfer("large.bin", Some(&digest(&old)), &digest(&new), describe(&new)).unwrap()["already_committed"],
        true
    );
    let third = vec![3; new.len()];
    let prepared =
        reopened.prepare_transfer("large.bin", Some(&digest(&new)), &digest(&third), describe(&third)).unwrap();
    let id = prepared["transfer_id"].as_str().unwrap();
    for (index, bytes) in third.chunks(65536).enumerate() {

        reopened.put_chunk(id, index, bytes).unwrap();

    }
    reopened.write("large.bin", Some(b"new editor change"), Some(&digest(&new))).unwrap();
    assert!(matches!(reopened.commit_transfer(id), Err(FileError::Conflict)));
    assert_eq!(reopened.read("large.bin").unwrap(), b"new editor change");

}

#[test]
fn rename_reuses_content_and_corrupt_chunk_never_commits( ) {

    let root = tempdir().unwrap();
    let files = store(root.path());
    let content = vec![4; 3 * 65536];
    files.write("original.bin", Some(&content), None).unwrap();
    let prepared = files.prepare_transfer("moved.bin", None, &digest(&content), describe(&content)).unwrap();
    assert_eq!(prepared["missing"], json!([]));
    let id = prepared["transfer_id"].as_str().unwrap();
    files.save_metadata(&format!("transfer-{id}-1.bin"), b"corrupt").unwrap();
    assert!(matches!(files.commit_transfer(id), Err(FileError::Data)));
    assert!(!root.path().join("moved.bin").exists());
    let prepared = files.prepare_transfer("moved.bin", None, &digest(&content), describe(&content)).unwrap();
    assert_eq!(prepared["missing"], json!([]));
    files.commit_transfer(id).unwrap();
    assert_eq!(files.read("moved.bin").unwrap(), content);
    assert_eq!(files.read("original.bin").unwrap(), content);
    assert!(files.prepare_transfer("../escape", None, &digest(&content), describe(&content)).is_err());
    assert!(files.prepare_transfer("target/escape", None, &digest(&content), describe(&content)).is_err());

}

#[test]
fn abandoned_transfer_can_be_cancelled_without_touching_target( ) {

    let root = tempdir().unwrap();
    let files = store(root.path());
    let bytes = vec![9; 65537];
    let prepared = files.prepare_transfer("cancelled.bin", None, &digest(&bytes), describe(&bytes)).unwrap();
    let id = prepared["transfer_id"].as_str().unwrap();
    files.put_chunk(id, 0, &bytes[..65536]).unwrap();
    files.abort_transfer(id).unwrap();
    assert!(!root.path().join("cancelled.bin").exists());
    assert!(files.commit_transfer(id).is_err());
    let resumed = files.prepare_transfer("cancelled.bin", None, &digest(&bytes), describe(&bytes)).unwrap();
    assert_eq!(resumed["missing"], json!([0, 1]));

}

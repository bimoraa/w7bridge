use super::*;
use crate::filesystem::FileSettings;
use tempfile::tempdir;

#[test]
fn durable_history_restore_checks_current_revision_and_blob_integrity( ) {

    let root = tempdir().unwrap();
    let files = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    files.write("src.rs", Some(b"before"), None).unwrap();
    files.write("src.rs", Some(b"after"), Some(&digest(b"before"))).unwrap();
    let records = files.history().unwrap();
    let id = records["records"][0]["id"].as_str().unwrap();
    assert_eq!(records["records"][0]["previous"], digest(b"before"));
    let reopened = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    assert!(matches!(reopened.restore(id, "previous", Some(&digest(b"before"))), Err(FileError::Conflict)));
    assert_eq!(reopened.read("src.rs").unwrap(), b"after");
    reopened.restore(id, "previous", Some(&digest(b"after"))).unwrap();
    assert_eq!(reopened.read("src.rs").unwrap(), b"before");
    reopened.save_metadata(&format!("history-blob-{}.bin", digest(b"after")), b"corrupt").unwrap();
    assert!(matches!(reopened.restore(id, "result", Some(&digest(b"before"))), Err(FileError::Data)));
    assert_eq!(reopened.read("src.rs").unwrap(), b"before");

}

#[test]
fn prepared_recovery_and_deletion_keep_original_and_bound_history( ) {

    let root = tempdir().unwrap();
    let files = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    fs::write(root.path().join("src.rs"), b"original").unwrap();
    files.prepare_revision("src.rs", Some(b"original"), Some(b"interrupted")).unwrap();
    assert_eq!(files.history().unwrap()["records"][0]["recovery_required"], true);
    assert_eq!(files.read("src.rs").unwrap(), b"original");
    files.write("src.rs", None, Some(&digest(b"original"))).unwrap();
    let record = files.history().unwrap()["records"][0].clone();
    files.restore(record["id"].as_str().unwrap(), "previous", None).unwrap();
    for number in 0..70 {

        let expected = digest(&files.read("src.rs").unwrap());
        files.write("src.rs", Some(number.to_string().as_bytes()), Some(&expected)).unwrap();

    }
    assert_eq!(files.history().unwrap()["records"].as_array().unwrap().len(), 64);
    assert_eq!(files.read("src.rs").unwrap(), b"69");

}

#[test]
fn history_prunes_before_reserving_incoming_content_and_preserves_pending( ) {

    let root = tempdir().unwrap();
    let files = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    files.write("src.rs", Some(b"old"), None).unwrap();
    files.write("src.rs", Some(b"new"), Some(&digest(b"old"))).unwrap();
    files.write("src.rs", Some(b"last"), Some(&digest(b"new"))).unwrap();
    files.prune_history(&[b"next"], 11).unwrap();
    assert_eq!(files.revisions().unwrap().len(), 1);
    files.prepare_revision("src.rs", Some(b"last"), Some(b"next")).unwrap();
    assert!(matches!(files.prune_history(&[b"large"], 5), Err(FileError::Limit)));
    assert!(files.revisions().unwrap().iter().any(|record| record.phase == "prepared"));
    assert_eq!(files.read("src.rs").unwrap(), b"last");

}

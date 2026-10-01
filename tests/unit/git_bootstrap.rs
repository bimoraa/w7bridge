use super::*;
use crate::config::{GitBootstrap, GitSource};

#[test]
fn reviewed_initial_handoff_requires_both_hashes_and_equal_head_and_index( ) {

    let local = json!({"state_hash":"a".repeat(64),"state":{"head":"c".repeat(40),"index":[{"path":"input.rs","oid":"d".repeat(40)}]}});
    let remote = json!({"state_hash":"b".repeat(64),"state":local["state"]});
    let mut settings = GitBootstrap {

        source: GitSource::Local,
        expected_local_state: "a".repeat(64),
        expected_remote_state: "b".repeat(64),

    };
    assert!(bootstrap(&settings, &local, &remote).unwrap());
    settings.source = GitSource::Remote;
    assert!(!bootstrap(&settings, &local, &remote).unwrap());
    let mut changed = remote.clone();
    changed["state_hash"] = json!("e".repeat(64));
    assert!(bootstrap(&settings, &local, &changed).is_err());
    changed = remote.clone();
    changed["state"]["head"] = json!("e".repeat(40));
    assert!(bootstrap(&settings, &local, &changed).is_err());
    changed = remote.clone();
    changed["state"]["index"][0]["oid"] = json!("e".repeat(40));
    assert!(bootstrap(&settings, &local, &changed).is_err());

}

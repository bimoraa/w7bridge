use super::*;
use tempfile::tempdir;

#[test]
fn folder_discovery_is_bounded_skips_builds_and_never_grants_commands( ) {

    let root = tempdir().unwrap();
    for (name, manifest) in [("rust", "Cargo.toml"), ("web", "package.json"), ("target", "Cargo.toml")] {

        fs::create_dir(root.path().join(name)).unwrap();
        fs::write(root.path().join(name).join(manifest), "{}").unwrap();

    }
    let discovery = Discovery::new(DiscoverySettings { roots: vec![root.path().to_owned()], max_depth: 1 });
    let snapshot = discovery.read();
    assert_eq!(snapshot.status, "ready");
    assert_eq!(snapshot.projects.len(), 2);
    assert!(snapshot.projects.iter().all(|project| project["commands"] == json!([])));
    assert_eq!(
        Discovery::new(DiscoverySettings { roots: vec![root.path().to_owned()], max_depth: 0 }).read().projects.len(),
        0
    );

}

#[cfg(unix)]
#[test]
fn folder_discovery_does_not_follow_outside_symlinks( ) {

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("Cargo.toml"), "[package]").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
    let snapshot = Discovery::new(DiscoverySettings { roots: vec![root.path().to_owned()], max_depth: 8 }).read();
    assert!(snapshot.projects.is_empty());

}

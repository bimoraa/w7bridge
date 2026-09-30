use super::*;
use crate::{Bridge, Config};
use tempfile::tempdir;

#[tokio::test]
async fn changed_source_after_sync_does_not_spawn_the_command( ) {

    let root = tempdir().unwrap();
    std::fs::write(root.path().join("source.rs"), b"confirmed").unwrap();
    let settings = json!({"version":1,"codex":{"enabled":false},"projects":[{"id":"p","root":root.path(),"files":{"enabled":true},"commands":{"build":{"executable":std::env::current_exe().unwrap(),"source_snapshot":true}}}]});
    let bridge =
        Bridge::new(Config::parse(&toml::to_string(&settings).unwrap()).unwrap(), CancellationToken::new()).unwrap();
    let files = bridge.policy.files("p").unwrap();
    let confirmed = crate::sync::state::coordination::manifest(&files).unwrap();
    std::fs::write(root.path().join("source.rs"), b"changed before build").unwrap();
    let result = bridge.processes.start(&bridge.policy, "p", "build", Some(&confirmed)).await;
    assert!(result.unwrap_err().contains("revision"));
    assert_eq!(bridge.processes.list("p").unwrap()["processes"].as_array().unwrap().len(), 0);
    assert_eq!(std::fs::read(root.path().join("source.rs")).unwrap(), b"changed before build");

}

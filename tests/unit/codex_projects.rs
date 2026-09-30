use super::*;
use serde_json::json;
use tempfile::tempdir;

fn discovery( home: &Path, ) -> Discovery {

    Discovery::new(CodexSettings { enabled: true, home: Some(home.to_owned()) })

}

fn database( home: &Path, ) -> Connection {

    let connection = Connection::open(home.join("state_5.sqlite")).unwrap();
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
         CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, position INTEGER NOT NULL);
         CREATE TABLE project_roots (project_id TEXT NOT NULL, position INTEGER NOT NULL, path TEXT NOT NULL);",
        )
        .unwrap();
    connection

}

fn insert( connection: &Connection, id: &str, name: &str, root: &Path, ) {

    connection.execute("INSERT INTO projects VALUES (?1, ?2, 0)", [id, name]).unwrap();
    connection.execute("INSERT INTO project_roots VALUES (?1, 0, ?2)", [id, root.to_str().unwrap()]).unwrap();

}

#[test]
fn sqlite_reads_committed_wal_changes_and_never_resurrects_legacy_projects() {

    let home = tempdir().unwrap();
    let root = tempdir().unwrap();
    let connection = database(home.path());
    fs::write(
        home.path().join(".codex-global-state.json"),
        json!({"local-projects": {
            "old": {"id": "old", "name": "removed", "rootPaths": [root.path()]}
        }})
        .to_string(),
    )
    .unwrap();
    let discovery = discovery(home.path());
    let empty = discovery.read();
    assert_eq!(empty.status, "ready");
    assert_eq!(empty.source, Some("codex_database"));
    assert!(empty.projects.is_empty());

    insert(&connection, "codex-id", "fatomic", root.path());
    let first = discovery.read();
    assert_eq!(first.projects.len(), 1);
    assert_eq!(first.projects[0].name, "fatomic");
    assert_eq!(first.projects[0].roots, [root.path()]);
    assert!(first.projects[0].available);
    connection.execute("UPDATE projects SET name = 'renamed'", []).unwrap();
    assert_eq!(discovery.read().projects[0].name, "renamed");
    connection.execute("DELETE FROM projects", []).unwrap();
    assert!(discovery.read().projects.is_empty());

}

#[test]
fn legacy_reads_only_local_project_metadata_and_reports_unavailable_roots() {

    let home = tempdir().unwrap();
    let root = tempdir().unwrap();
    let missing = root.path().join("missing");
    fs::write(
        home.path().join(".codex-global-state.json"),
        json!({
            "local-projects": {
                "local": {"id": "local", "name": "프로젝트", "rootPaths": [root.path(), missing, root.path()]},
                "relative": {"id": "relative", "name": "bad", "rootPaths": ["../outside"]},
                "mismatch": {"id": "other", "name": "bad", "rootPaths": [root.path()]},
                "bad": {"name": "incomplete"}
            },
            "remote-projects": {"private": {"token": "비공개 값"}},
            "thread-workspace-root-hints": {"old": root.path()}
        })
        .to_string(),
    )
    .unwrap();
    let snapshot = discovery(home.path()).read();
    assert_eq!(snapshot.source, Some("codex_legacy_json"));
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].roots, [root.path().to_owned(), missing]);
    assert!(!snapshot.projects[0].available);
    assert!(!serde_json::to_string(&snapshot.projects).unwrap().contains("비공개 값"));

}

#[test]
fn missing_corrupt_and_disabled_sources_do_not_create_or_reuse_a_registry() {

    let root = tempdir().unwrap();
    let home = root.path().join("absent");
    assert_eq!(discovery(&home).read().status, "missing");
    assert!(!home.exists());
    assert_eq!(Discovery::new(CodexSettings { enabled: false, home: Some(home) }).read().status, "disabled");

    let connection = database(root.path());
    connection.execute_batch("DROP TABLE project_roots").unwrap();
    let snapshot = discovery(root.path()).read();
    assert_eq!(snapshot.status, "error");
    assert!(snapshot.projects.is_empty());
    assert!(snapshot.error.unwrap().contains("schema"));
    assert!(!root.path().join(".codex-global-state.json").exists());

}

#[tokio::test]
async fn listing_merges_explicit_permissions_without_granting_discovered_projects() {

    let home = tempdir().unwrap();
    let root = tempdir().unwrap();
    let other = tempdir().unwrap();
    let connection = database(home.path());
    insert(&connection, "codex-id", "fatomic", root.path());
    insert(&connection, "other-id", "other", other.path());
    let policy = crate::security::Policy::new(vec![crate::config::ProjectDefinition {
        id: "manual".into(),
        root: root.path().to_owned(),
        files: Default::default(),
        requires_sync: false,
        presets: None,
        git: None,
        commands: BTreeMap::new(),
    }])
    .unwrap();
    let listing = crate::tools::project::status::list_projects(
        &policy,
        std::sync::Arc::new(discovery(home.path())),
        std::sync::Arc::new(crate::security::discovery::Discovery::new(Default::default())),
        serde_json::Map::new(),
    )
    .await
    .unwrap()
    .structured_content
    .unwrap();
    let projects = listing["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0]["id"], "manual");
    assert_eq!(projects[0]["codex_id"], "codex-id");
    assert_eq!(projects[1]["id"], "other-id");
    assert_eq!(projects[1]["commands"], json!([]));
    assert!(matches!(policy.resolve("other-id", "test"), Err(crate::PolicyError::Unknown)));
    assert!(matches!(policy.files("other-id"), Err(crate::FileError::Disabled)));
    assert_eq!(listing["codex"]["status"], "ready");

}

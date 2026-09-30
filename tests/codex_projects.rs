use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;

#[path = "fixtures/client.rs"]
mod client;
#[path = "fixtures/process.rs"]
mod fixture;

#[cfg(windows)]
#[tokio::test]
#[ignore = "실제 Windows Codex home을 W7BRIDGE_LIVE_CODEX_HOME으로 지정한 읽기 전용 검증"]
async fn live_windows_codex_projects_are_returned_over_mcp() {

    let home = std::env::var("W7BRIDGE_LIVE_CODEX_HOME").expect("실제 Codex home이 필요해");
    let root = tempdir().unwrap();
    let path = root.path().join("settings.toml");
    std::fs::write(&path, toml::to_string(&json!({"version": 1, "codex": {"home": home}})).unwrap()).unwrap();
    let mut client = client::Client::start_config(&path).await;
    let listing = client.call(2, "list_projects", json!({})).await;
    let content = &listing["result"]["structuredContent"];
    assert_eq!(content["codex"]["status"], "ready");
    assert_eq!(content["codex"]["source"], "codex_database");
    let projects = content["projects"].as_array().unwrap();
    assert!(projects.iter().any(|project| project["name"] == "fatomic"));
    assert!(projects.iter().all(|project| project["commands"] == json!([])));
    println!("{}", json!({"count": projects.len(), "projects": projects}));
    client.close().await;

}

#[tokio::test]
async fn codex_projects_refresh_over_stdio_without_changing_execution_permissions() {

    let root = tempdir().unwrap();
    let home = root.path().join("codex");
    let project = root.path().join("project");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&project).unwrap();
    let connection = Connection::open(home.join("state_5.sqlite")).unwrap();
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
         CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, position INTEGER NOT NULL);
         CREATE TABLE project_roots (project_id TEXT NOT NULL, position INTEGER NOT NULL, path TEXT NOT NULL);
         INSERT INTO projects VALUES ('codex-project', 'fatomic', 0);",
        )
        .unwrap();
    connection
        .execute("INSERT INTO project_roots VALUES ('codex-project', 0, ?1)", [project.to_str().unwrap()])
        .unwrap();
    let mut client = client::Client::start(root.path(), "output", json!({})).await;
    let first = client.call(2, "list_projects", json!({})).await;
    let projects = first["result"]["structuredContent"]["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[1]["id"], "codex-project");
    assert_eq!(projects[1]["name"], "fatomic");
    assert_eq!(projects[1]["roots"], json!([project]));
    assert_eq!(projects[1]["commands"], json!([]));
    assert_eq!(first["result"]["structuredContent"]["codex"]["source"], "codex_database");
    let denied = client.call(3, "run_command", json!({"project_id": "codex-project", "command": "test"})).await;
    assert_eq!(denied["result"]["isError"], true);

    connection.execute("UPDATE projects SET name = 'renamed'", []).unwrap();
    let renamed = client.call(4, "list_projects", json!({})).await;
    assert_eq!(renamed["result"]["structuredContent"]["projects"][1]["name"], "renamed");
    connection.execute("DELETE FROM projects", []).unwrap();
    let deleted = client.call(5, "list_projects", json!({})).await;
    assert_eq!(deleted["result"]["structuredContent"]["projects"].as_array().unwrap().len(), 1);
    assert_eq!(deleted["result"]["structuredContent"]["codex"]["status"], "ready");
    let invalid = client.call(6, "list_projects", json!({"home": home})).await;
    assert_eq!(invalid["error"]["code"], -32602);
    connection.execute_batch("DROP TABLE project_roots").unwrap();
    let broken = client.call(7, "list_projects", json!({})).await;
    assert_eq!(broken["result"]["structuredContent"]["codex"]["status"], "error");
    assert_eq!(broken["result"]["structuredContent"]["projects"][0]["id"], "sample");
    client.close().await;

}

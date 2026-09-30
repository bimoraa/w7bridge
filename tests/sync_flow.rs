use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::{RoleClient, ServiceExt, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use std::{fs, time::Duration};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;
use w7bridge::{
    Bridge, Config, FileError,
    files::{FileEntry, FileSettings, FileStore, digest, hashes},
    sync::{Peer, Session, SyncError},
};

#[test]
#[ignore = "sync flow 테스트가 환경을 준비한 뒤 실행하는 fixture"]
fn command_fixture() {

    assert_eq!(fs::read_to_string("MEMORY.md").unwrap(), "latest mac context");
    println!("synced source build/test/run");
    eprintln!("live stderr");
    std::thread::sleep(Duration::from_millis(250));

}

async fn client(bridge: Bridge) -> RunningService<RoleClient, ()> {

    let (server, client) = tokio::io::duplex(65536);
    tokio::spawn(async move {

        let service = bridge.serve(server).await.unwrap();
        let _ = service.waiting().await;

    });
    ().serve(client).await.unwrap()

}
async fn call(client: &RunningService<RoleClient, ()>, name: &str, args: Value) -> Result<Value, SyncError> {

    let result = client
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args.as_object().unwrap().clone()))
        .await
        .map_err(|_| SyncError::Offline)?;
    if result.is_error == Some(true) {

        return Err(SyncError::Peer);

    }
    result.structured_content.ok_or(SyncError::Peer)

}
struct McpPeer {

    client: RunningService<RoleClient, ()>,

}
impl Peer for McpPeer {

    async fn list(&self) -> Result<Vec<FileEntry>, SyncError> {

        serde_json::from_value(call(&self.client, "list_files", json!({"project_id":"sample"})).await?["files"].clone())
            .map_err(|_| SyncError::Peer)

    }
    async fn read(&self, path: &str) -> Result<Vec<u8>, SyncError> {

        let response = self
            .client
            .call_tool(
                CallToolRequestParams::new("read_file")
                    .with_arguments(json!({"project_id":"sample","path":path}).as_object().unwrap().clone()),
            )
            .await
            .map_err(|_| SyncError::Offline)?;
        if response.is_error == Some(true)
            && response.structured_content.as_ref().is_some_and(|value| value["code"] == "not_found")
        {

            return Err(FileError::Io(std::io::Error::from(std::io::ErrorKind::NotFound)).into());

        }
        let value = response.structured_content.ok_or(SyncError::Peer)?;
        STANDARD.decode(value["content_base64"].as_str().ok_or(SyncError::Peer)?).map_err(|_| SyncError::Peer)

    }
    async fn write(&self, path: &str, content: Option<&[u8]>, expected: Option<&str>) -> Result<(), SyncError> {

        call(&self.client,"write_file",json!({"project_id":"sample","path":path,"content_base64":content.map(|bytes|STANDARD.encode(bytes)),"expected_hash":expected})).await?;
        Ok(())

    }

}
async fn checkpoint(peer: &McpPeer, generation: u64, status: &str, files: &FileStore) {

    let hash = digest(&serde_json::to_vec(&hashes(&files.list().unwrap())).unwrap());
    call(&peer.client,"sync_checkpoint",json!({"project_id":"sample","generation":generation,"status":status,"manifest_hash":hash,"conflicts":[],"lease_seconds":3})).await.unwrap();

}

#[tokio::test]
async fn sync_then_fresh_wait_gates_build_test_run_and_reports_conflict_offline() {

    let mac = tempdir().unwrap();
    let windows = tempdir().unwrap();
    let local = FileStore::new(mac.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    let remote = FileStore::new(windows.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    remote.write("AGENTS.md", Some(b"shared rules"), None).unwrap();
    local.write("MEMORY.md", Some(b"initial mac context"), None).unwrap();
    let command = json!({"executable":std::env::current_exe().unwrap(),"args":["--ignored","--exact","command_fixture","--nocapture"]});
    let config = json!({"version":1,"projects":[{"id":"sample","root":windows.path(),"requires_sync":true,"files":{"enabled":true},
        "commands":{"build":command,"test":command,"run":command}}]});
    let shutdown = CancellationToken::new();
    let bridge = Bridge::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap(), shutdown.clone()).unwrap();
    let peer = McpPeer { client: client(bridge.clone()).await };
    let commands = client(bridge.clone()).await;
    let mut session = Session::open(local.clone(), "pair".into()).unwrap();
    session.round(&peer).await.unwrap();
    checkpoint(&peer, 0, "synced", &remote).await;
    assert_eq!(local.read("AGENTS.md").unwrap(), b"shared rules");
    local.write("MEMORY.md", Some(b"latest mac context"), Some(&digest(b"initial mac context"))).unwrap();
    // 이전 synced receipt가 있어도 build마다 새 round를 기다려.
    for command in ["build", "test", "run"] {

        let execute = call(&commands, "run_command", json!({"project_id":"sample","command":command,"wait":true}));
        let sync = async {

            let ticket = tokio::time::timeout(Duration::from_secs(5), async {

                loop {

                    let status = call(&peer.client, "sync_status", json!({"project_id":"sample"})).await.unwrap();
                    if status["requested_generation"].as_u64().unwrap()
                        > status["confirmed_generation"].as_u64().unwrap()
                    {

                        break status["requested_generation"].as_u64().unwrap();

                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;

                }

            })
            .await
            .unwrap();
            session.round(&peer).await.unwrap();
            checkpoint(&peer, ticket, "synced", &remote).await;

        };
        let (result, ()) = tokio::join!(execute, sync);
        let result = result.unwrap();
        assert_eq!(result["exit_code"], 0);
        assert!(result["stdout"].as_str().unwrap().contains("synced source"));

    }
    let hash = digest(&serde_json::to_vec(&hashes(&remote.list().unwrap())).unwrap());
    assert!(call(&peer.client, "sync_checkpoint", json!({"project_id":"sample","generation":0,"status":"conflict","manifest_hash":hash,"conflicts":[],"lease_seconds":3})).await.is_err());
    assert_eq!(call(&commands, "sync_status", json!({"project_id":"sample"})).await.unwrap()["status"], "synced");
    checkpoint(&peer, 3, "conflict", &remote).await;
    assert_eq!(call(&commands, "sync_status", json!({"project_id":"sample"})).await.unwrap()["status"], "conflict");
    assert!(call(&commands, "run_command", json!({"project_id":"sample","command":"build"})).await.is_err());
    tokio::time::sleep(Duration::from_millis(3100)).await;
    assert_eq!(call(&commands, "sync_status", json!({"project_id":"sample"})).await.unwrap()["status"], "offline");
    shutdown.cancel();
    bridge.shutdown().await;
    peer.client.cancel().await.unwrap();
    commands.cancel().await.unwrap();

}

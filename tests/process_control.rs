use rmcp::{RoleClient, ServiceExt, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    time::Duration,
};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;
use w7bridge::{Bridge, Config};

#[test]
#[ignore = "process control 테스트가 환경을 준비한 뒤 실행하는 fixture"]
fn application() {

    println!("live stdout");
    io::stdout().flush().unwrap();
    eprintln!("live stderr");
    io::stderr().flush().unwrap();
    std::thread::sleep(Duration::from_secs(30));

}

async fn client(bridge: Bridge) -> RunningService<RoleClient, ()> {

    let (server, client) = tokio::io::duplex(65536);
    tokio::spawn(async move {

        let service = bridge.serve(server).await.unwrap();
        let _ = service.waiting().await;

    });
    ().serve(client).await.unwrap()

}
async fn call(client: &RunningService<RoleClient, ()>, name: &str, args: Value) -> Value {

    let result = client
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args.as_object().unwrap().clone()))
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.unwrap()

}

#[tokio::test]
async fn live_output_reconnect_handles_stop_restart_and_host_cleanup() {

    let root = tempdir().unwrap();
    let config = json!({"version":1,"execution":{"timeout_seconds":1},"projects":[{"id":"app","root":root.path(),
        "commands":{"run":{"executable":std::env::current_exe().unwrap(),"args":["--ignored","--exact","application","--nocapture"],"background":true}}}]});
    let shutdown = CancellationToken::new();
    let bridge = Bridge::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap(), shutdown.clone()).unwrap();
    let first = client(bridge.clone()).await;
    let started = call(&first, "start_process", json!({"project_id":"app","command":"run"})).await;
    let id = started["process_id"].as_str().unwrap().to_owned();
    tokio::time::timeout(Duration::from_secs(5), async {

        loop {

            let output = call(&first, "read_process_output", json!({"project_id":"app","process_id":id})).await;
            let events = output["events"].as_array().unwrap();
            if events.iter().any(|event| event["text"].as_str().is_some_and(|text| text.contains("live stdout")))
                && events.iter().any(|event| event["stream"] == "stderr")
            {

                assert_eq!(output["status"], "running");
                break;

            }
            tokio::time::sleep(Duration::from_millis(20)).await;

        }

    })
    .await
    .unwrap();
    first.cancel().await.unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let second = client(bridge.clone()).await;
    let recovered = call(&second, "list_processes", json!({"project_id":"app"})).await;
    assert_eq!(recovered["processes"][0]["process_id"], id);
    assert_eq!(recovered["processes"][0]["status"], "running");
    let restarted = call(&second, "restart_process", json!({"project_id":"app","process_id":id})).await;
    assert_ne!(restarted["process_id"], id);
    let stopped = call(&second, "stop_process", json!({"project_id":"app","process_id":restarted["process_id"]})).await;
    assert_eq!(stopped["status"], "stopped");
    assert_eq!(stopped["result"]["status"], "cancelled");
    let last = call(&second, "start_process", json!({"project_id":"app","command":"run"})).await;
    shutdown.cancel();
    bridge.shutdown().await;
    let output =
        call(&second, "read_process_output", json!({"project_id":"app","process_id":last["process_id"]})).await;
    assert_eq!(output["status"], "stopped");
    second.cancel().await.unwrap();
    assert!(!root.path().join("survived").exists());

}

#[test]
#[ignore = "long-poll 테스트가 환경을 준비한 뒤 실행하는 fixture"]
fn delayed_application() {

    println!("first event");
    io::stdout().flush().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    eprintln!("delayed event");
    io::stderr().flush().unwrap();
    std::thread::sleep(Duration::from_millis(500));

}

#[tokio::test]
async fn long_poll_wakes_for_output_and_completion_and_preserves_cursor_replay() {

    let root = tempdir().unwrap();
    let config = json!({"version":1,"projects":[{"id":"app","root":root.path(),
        "commands":{"run":{"executable":std::env::current_exe().unwrap(),"args":["--ignored","--exact","delayed_application","--nocapture"]}}}]});
    let bridge =
        Bridge::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap(), CancellationToken::new()).unwrap();
    let first = client(bridge.clone()).await;
    let second = client(bridge.clone()).await;
    let started = call(&first, "start_process", json!({"project_id":"app","command":"run"})).await;
    let id = started["process_id"].clone();
    let mut cursor = 0;
    let mut saw_delayed_while_running = false;
    tokio::time::timeout(Duration::from_secs(5), async {

        loop {

            let args = json!({"project_id":"app","process_id":id,"cursor":cursor,"wait_seconds":2});
            let (left, right) = tokio::join!(
                call(&first, "read_process_output", args.clone()),
                call(&second, "read_process_output", args)
            );
            for output in [&left, &right] {

                if output["events"].as_array().unwrap().iter().any(|event| {

                    event["stream"] == "stderr" && event["text"].as_str().unwrap().contains("delayed event")

                }) {

                    saw_delayed_while_running |= output["status"] == "running";

                }

            }
            cursor = left["next_cursor"].as_u64().unwrap();
            if left["status"] == "stopped" {

                assert_eq!(left["result"]["exit_code"], 0);
                break;

            }

        }

    })
    .await
    .unwrap();
    assert!(saw_delayed_while_running);
    let replay =
        call(&second, "read_process_output", json!({"project_id":"app","process_id":id,"cursor":0,"wait_seconds":30}))
            .await;
    assert!(
        replay["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["text"].as_str().unwrap().contains("first event"))
    );
    for args in [
        json!({"project_id":"app","process_id":id,"wait_seconds":31}),
        json!({"project_id":"app","process_id":id,"cursor":cursor+1}),
    ] {

        let result = first
            .call_tool(
                CallToolRequestParams::new("read_process_output").with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));

    }
    bridge.shutdown().await;
    first.cancel().await.unwrap();
    second.cancel().await.unwrap();

}

#[test]
#[ignore = "source snapshot 테스트용 subprocess fixture"]
fn snapshot_application( ) {

    assert_eq!(std::fs::read("snapshot.txt").unwrap(), b"before");
    println!("snapshot started");
    io::stdout().flush().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(std::fs::read("snapshot.txt").unwrap(), b"before");
    println!("snapshot finished");

}

#[tokio::test]
async fn verified_build_snapshot_is_not_changed_by_next_sync( ) {

    let root = tempdir().unwrap();
    std::fs::write(root.path().join("snapshot.txt"), b"before").unwrap();
    let config = json!({"version":1,"projects":[{"id":"app","root":root.path(),"files":{"enabled":true},"commands":{"build":{"executable":std::env::current_exe().unwrap(),"source_snapshot":true,"args":["--ignored","--exact","snapshot_application","--nocapture"]}}}]});
    let bridge =
        Bridge::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap(), CancellationToken::new()).unwrap();
    let peer = client(bridge.clone()).await;
    let started = call(&peer, "start_process", json!({"project_id":"app","command":"build"})).await;
    std::fs::write(root.path().join("snapshot.txt"), b"next sync").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), async {

        let mut cursor = 0;
        loop {

            let output = call(
                &peer,
                "read_process_output",
                json!({"project_id":"app","process_id":started["process_id"],"cursor":cursor,"wait_seconds":2}),
            )
            .await;
            cursor = output["next_cursor"].as_u64().unwrap();
            if output["status"] == "stopped" {

                break output["result"].clone();

            }

        }

    })
    .await
    .unwrap();
    assert_eq!(result["exit_code"], 0);
    assert_eq!(result["revision_verified"], true);
    assert_eq!(result["revision_at_start"], result["revision_at_completion"]);
    assert_eq!(std::fs::read(root.path().join("snapshot.txt")).unwrap(), b"next sync");
    assert!(std::path::Path::new(result["artifact_root"].as_str().unwrap()).join("snapshot.txt").exists());
    bridge.shutdown().await;
    peer.cancel().await.unwrap();

}

#[tokio::test]
async fn sync_restart_is_opt_in_and_same_revision_does_not_restart( ) {

    use w7bridge::files::{FileSettings, FileStore, digest, hashes};
    let root = tempdir().unwrap();
    let files = FileStore::new(root.path(), FileSettings { enabled: true, ..Default::default() }).unwrap();
    files.write("source.rs", Some(b"v1"), None).unwrap();
    let command = json!({"executable":std::env::current_exe().unwrap(),"args":["--ignored","--exact","application","--nocapture"],"background":true,"restart_on_sync":true});
    let config = json!({"version":1,"projects":[{"id":"app","root":root.path(),"requires_sync":true,"files":{"enabled":true},"commands":{"run":command}}]});
    let bridge =
        Bridge::new(Config::parse(&toml::to_string(&config).unwrap()).unwrap(), CancellationToken::new()).unwrap();
    let peer = client(bridge.clone()).await;
    let sync = client(bridge.clone()).await;
    let start = call(&peer, "start_process", json!({"project_id":"app","command":"run"}));
    let checkpoint = async {

        loop {

            let status = call(&sync, "sync_status", json!({"project_id":"app"})).await;
            if status["requested_generation"] == 1 {

                break;

            }
            tokio::time::sleep(Duration::from_millis(10)).await;

        }
        let hash = digest(&serde_json::to_vec(&hashes(&files.list().unwrap())).unwrap());
        call(&sync,"sync_checkpoint",json!({"project_id":"app","generation":1,"status":"synced","manifest_hash":hash,"conflicts":[],"lease_seconds":6})).await;

    };
    let (started, ()) = tokio::join!(start, checkpoint);
    let first = started["process_id"].clone();
    files.write("source.rs", Some(b"v2"), Some(&digest(b"v1"))).unwrap();
    let hash = digest(&serde_json::to_vec(&hashes(&files.list().unwrap())).unwrap());
    let args = json!({"project_id":"app","generation":1,"status":"synced","manifest_hash":hash,"conflicts":[],"lease_seconds":6});
    assert_eq!(call(&sync, "sync_checkpoint", args.clone()).await["changed"], true);
    let processes = call(&peer, "list_processes", json!({"project_id":"app"})).await;
    let running =
        processes["processes"].as_array().unwrap().iter().find(|process| process["status"] == "running").unwrap();
    assert_ne!(running["process_id"], first);
    let second = running["process_id"].clone();
    assert_eq!(call(&sync, "sync_checkpoint", args).await["changed"], false);
    let status = call(&peer, "project_status", json!({"project_id":"app"})).await;
    assert_eq!(status["requires_sync"], true);
    assert_eq!(status["commands"], json!(["run"]));
    assert!(
        status["processes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|process| process["process_id"] == second && process["status"] == "running")
    );
    let events = call(&peer, "read_events", json!({"project_id":"app"})).await;
    assert!(events["events"].as_array().unwrap().iter().any(|event| event["kind"] == "sync_completed"));
    bridge.shutdown().await;
    peer.cancel().await.unwrap();
    sync.cancel().await.unwrap();

}

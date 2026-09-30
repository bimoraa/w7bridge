/*! device, sync와 process 상태를 한 응답에 모아. */

use crate::server::Bridge;
use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {

    project_id: String,
    #[serde(default)]
    cursor: u64,
    #[serde(default)]
    wait_seconds: u64,

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Negotiation {

    versions: Vec<u32>,

}

pub(crate) fn definitions( ) -> Vec<Tool> {

    ["project_status", "read_events", "bridge_info"].into_iter().map(|name| {

        let schema = if name == "bridge_info" {

            json!({"type":"object","properties":{"versions":{"type":"array","items":{"type":"integer"},"minItems":1,"maxItems":8}},"additionalProperties":false})

        } else {

            let mut properties = json!({"project_id":{"type":"string"}});
            if name == "read_events" {

                properties["cursor"] = json!({"type":"integer","minimum":0});
                properties["wait_seconds"] = json!({"type":"integer","minimum":0,"maximum":30});

            }
            json!({"type":"object","properties":properties,"required":["project_id"],"additionalProperties":false})

        };
        Tool::new(name, "device 식별, sync·process 상태와 event 또는 지원 protocol을 조회합니다", schema.as_object().cloned().unwrap_or_default())
            .with_annotations(ToolAnnotations::new().read_only(true).idempotent(true).open_world(false))

    }).collect()

}

pub(crate) async fn call( bridge: &Bridge, name: &str, args: Map<String, Value>, cancellation: CancellationToken, peer: Option<&str>, ) -> Result<CallToolResult, ErrorData> {

    let invalid = || ErrorData::invalid_params("상태 도구의 인자를 확인하세요", None);
    if name == "bridge_info" {

        let versions = if args.is_empty() {

            vec![1, 2]

        } else {

            serde_json::from_value::<Negotiation>(Value::Object(args)).map_err(|_| invalid())?.versions

        };
        if versions.is_empty() || versions.len() > 8 {

            return Err(invalid());

        }
        let version = [2, 1].into_iter().find(|version| versions.contains(version));
        let info = json!({"device_id":bridge.device_id,"machine_os":std::env::consts::OS,"server_version":env!("CARGO_PKG_VERSION"),
            "protocol_version":version,"supported_versions":[1,2],"boot_id":bridge.events.boot_id,
            "features":["sync_gate","live_output","output_long_poll","process_handles","events","presets","folder_discovery","chunk_sync","peer_sync","history","git_handoff","source_snapshot"]});
        return Ok(if version.is_some() {

            CallToolResult::structured(info)

        } else {

            CallToolResult::structured_error(info)

        });

    }
    if name == "project_status" && (args.contains_key("cursor") || args.contains_key("wait_seconds")) {

        return Err(invalid());

    }
    let args: Project = serde_json::from_value(Value::Object(args)).map_err(|_| invalid())?;
    let root = match bridge.policy.root(&args.project_id) {

        Ok(root) => root,
        Err(error) => return Ok(CallToolResult::structured_error(json!({"message":error.to_string()}))),

    };
    if name == "read_events" {

        return Ok(match bridge.events.read(&args.project_id, args.cursor, args.wait_seconds, cancellation).await {

            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::structured_error(json!({"message":error})),

        });

    }
    let started = std::time::Instant::now();
    let sync = match bridge.policy.files(&args.project_id) {

        Ok(files) => {

            let all = bridge
                .coordinator
                .status(&args.project_id, &files)
                .unwrap_or_else(|error| json!({"status":"error","error":error}));
            if peer.is_some() {

                let mut selected = bridge
                    .coordinator
                    .status_peer(&args.project_id, &files, peer)
                    .unwrap_or_else(|error| json!({"status":"error","error":error}));
                selected["paired_devices"] = all;
                selected

            } else {

                all

            }

        }
        Err(crate::FileError::Disabled) => json!({"status":"disabled"}),
        Err(error) => json!({"status":"error","error":error.to_string()}),

    };
    let processes = bridge
        .processes
        .list(&args.project_id)
        .map_err(|_| ErrorData::internal_error("process 상태를 읽을 수 없습니다", None))?;
    let events = bridge
        .events
        .read(&args.project_id, 0, 0, cancellation)
        .await
        .map_err(|_| ErrorData::internal_error("event 상태를 읽을 수 없습니다", None))?;
    let last_error = events["events"]
        .as_array()
        .and_then(|events| {

            events.iter().rev().find(|event| {

                matches!(
                    event["kind"].as_str(),
                    Some("process_crashed" | "command_timed_out" | "restart_failed" | "verification_failed")
                )

            })

        })
        .cloned();
    let commands = bridge
        .policy
        .list()
        .into_iter()
        .map(|project| json!(project))
        .find(|project| project["id"] == args.project_id)
        .map(|project| project["commands"].clone())
        .unwrap_or_else(|| json!([]));
    Ok(CallToolResult::structured(json!({"project_id":args.project_id,"device_id":bridge.device_id,"root":root,
        "commands":commands,"requires_sync":bridge.policy.requires_sync(&args.project_id).unwrap_or(false),
        "device_online":true,"peer_online":!matches!(sync["status"].as_str(),Some("offline"|"disabled"|"error")),
        "machine_os":std::env::consts::OS,"boot_id":bridge.events.boot_id,"sync":sync,"processes":processes["processes"],
        "last_error":last_error.or_else(||sync.get("error").cloned()),"available_command_slots":bridge.executor.available_slots(),"queue_depth":0,
        "diagnostic_duration_ms":started.elapsed().as_millis()})))

}

/*! sync 상태, fresh 확인 대기와 daemon checkpoint를 MCP에 연결해. */

use crate::{security::Policy, sync::Coordinator};
use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;
mod pull;
mod push;
mod status;

pub(crate) fn definitions() -> Vec<Tool> {

    ["sync_status", "wait_for_sync", "sync_checkpoint"].into_iter().map(|name| {

        let mut properties = json!({"project_id": {"type": "string"}});
        let mut required = vec!["project_id"];
        if name == "wait_for_sync" { properties["timeout_seconds"] = json!({"type": "integer", "minimum": 1, "maximum": 120}); }
        if name == "sync_checkpoint" {

            properties["generation"] = json!({"type": "integer", "minimum": 0});
            properties["status"] = json!({"type": "string", "enum": ["synced", "syncing", "conflict"]});
            properties["manifest_hash"] = json!({"type": "string"});
            properties["conflicts"] = json!({"type": "array", "items": {"type": "string"}, "maxItems": 10000});
            properties["lease_seconds"] = json!({"type": "integer", "minimum": 3, "maximum": 180});
            properties["latency_ms"] = json!({"type": "integer", "minimum": 0, "maximum": 60000});
            required.extend(["generation", "status", "manifest_hash", "conflicts", "lease_seconds"]);

        }
        let schema = json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false});
        Tool::new(name, match name { "sync_status" => "daemon 연결과 manifest 일치 상태를 조회합니다",

            "wait_for_sync" => "새 sync round가 끝나고 양쪽 manifest가 일치할 때까지 기다립니다",

            _ => "sync daemon이 새 round 결과를 확인합니다; 일반 작업에는 sync_status와 wait_for_sync를 사용하세요" },
            schema.as_object().cloned().unwrap_or_default()).with_annotations(ToolAnnotations::new()
                .read_only(name == "sync_status").destructive(false).idempotent(name == "sync_status").open_world(false))

    }).collect()

}

pub(crate) async fn call(
    policy: &Policy,
    coordinator: &Coordinator,
    name: &str,
    args: Map<String, Value>,
    cancellation: CancellationToken,
    peer: Option<&str>,
) -> Result<CallToolResult, ErrorData> {

    let result = match name {

        "sync_status" => status::read(policy, coordinator, args, peer)?,
        "wait_for_sync" => pull::wait(policy, coordinator, args, cancellation, peer).await?,
        _ => push::checkpoint(policy, coordinator, args, peer)?,

    };
    Ok(match result {

        Ok(value) => CallToolResult::structured(value),
        Err(message) => CallToolResult::structured_error(json!({"message": message})),

    })

}

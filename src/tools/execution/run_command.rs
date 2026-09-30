use crate::protocol::request::RunArguments;
use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::{execution::Processes, security::Policy};

pub(crate) async fn run(
    policy: &Policy,
    processes: &Processes,
    coordinator: &crate::sync::Coordinator,
    arguments: Map<String, Value>,
    cancellation: CancellationToken,
    peer: Option<&str>,
) -> Result<CallToolResult, ErrorData> {

    let args: RunArguments = serde_json::from_value(Value::Object(arguments))
        .map_err(|_| ErrorData::invalid_params("project_id와 command 문자열만 전달하세요", None))?;

    let revision = match coordinator.gate_peer(policy, &args.project_id, cancellation.clone(), peer).await {

        Ok(revision) => revision,
        Err(message) => return Ok(CallToolResult::structured_error(json!({"message":message}))),

    };
    Ok(match processes.run(policy, &args.project_id, &args.command, cancellation, revision.as_deref()).await {

        Ok(value) if value["success"] == true => CallToolResult::structured(value),
        Ok(value) => CallToolResult::structured_error(value),
        Err(error) => CallToolResult::structured_error(json!({"message":error})),

    })

}

pub(crate) fn definition() -> Tool {

    let schema = Map::from_iter([
        ("type".into(), json!("object")),
        (
            "properties".into(),
            json!({
                "project_id": { "type": "string", "description": "등록된 프로젝트 ID" },
                "command": { "type": "string", "description": "등록된 명령 이름" }
            }),
        ),
        ("required".into(), json!(["project_id", "command"])),
        ("additionalProperties".into(), json!(false)),
    ]);

    Tool::new("run_command", "등록된 명령을 고정된 root와 인자로 실행합니다", schema)
        .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(false).open_world(true))

}

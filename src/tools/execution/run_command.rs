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
    output: Option<tokio::sync::mpsc::Sender<Value>>,
) -> Result<CallToolResult, ErrorData> {

    let args: RunArguments = serde_json::from_value(Value::Object(arguments))
        .map_err(|_| ErrorData::invalid_params("project_id, command, wait, yield_time_ms를 확인하세요", None))?;
    if args.yield_time_ms.is_some_and(|value| value > 30_000) || (args.wait && args.yield_time_ms.is_some()) {

        return Err(ErrorData::invalid_params("yield_time_ms는 0..=30000이며 wait=true와 함께 쓸 수 없습니다", None));

    }

    let revision = match coordinator.gate_peer(policy, &args.project_id, cancellation.clone(), peer).await {

        Ok(revision) => revision,
        Err(message) => return Ok(CallToolResult::structured_error(json!({"message":message}))),

    };
    let yield_time = (!args.wait).then(|| std::time::Duration::from_millis(args.yield_time_ms.unwrap_or(100)));
    Ok(
        match processes
            .run(
                policy,
                &args.project_id,
                &args.command,
                cancellation,
                revision.as_deref(),
                crate::execution::process::RunOptions { yield_time, output },
            )
            .await
        {

            Ok(value) if value["success"] == true || value["status"] == "running" => CallToolResult::structured(value),
            Ok(value) => CallToolResult::structured_error(value),
            Err(error) => CallToolResult::structured_error(json!({"message":error})),

        },
    )

}

pub(crate) fn definition() -> Tool {

    let schema = Map::from_iter([
        ("type".into(), json!("object")),
        (
            "properties".into(),
            json!({
                "project_id": { "type": "string", "description": "등록된 프로젝트 ID" },
                "command": { "type": "string", "description": "등록된 명령 이름" },
                "wait": { "type": "boolean", "default": false, "description": "true면 종료까지 기다리며 progress notification으로 출력합니다" },
                "yield_time_ms": { "type": "integer", "minimum": 0, "maximum": 30000, "default": 100, "description": "process 시작 후 첫 출력 또는 이 대기 한도에 handle과 cursor를 반환합니다" }
            }),
        ),
        ("required".into(), json!(["project_id", "command"])),
        ("additionalProperties".into(), json!(false)),
    ]);

    Tool::new("run_command", "최신 sync 후 명령을 시작하고 첫 출력 또는 100ms 뒤 반환합니다. status=running이면 process_id와 next_cursor로 read_process_output을 계속 읽고 stop_process로 중지하세요. wait=true는 종료까지 기다립니다.", schema)
        .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(false).open_world(true))

}

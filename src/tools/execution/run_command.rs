use crate::protocol::request::RunArguments;
use rmcp::{
    ErrorData,
    model::{CallToolResult, ContentBlock, Tool, ToolAnnotations},
};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::{execution::Executor, security::Policy};

pub(crate) async fn run(
    policy: &Policy,
    executor: &Executor,
    coordinator: &crate::sync::Coordinator,
    arguments: Map<String, Value>,
    cancellation: CancellationToken,
) -> Result<CallToolResult, ErrorData> {

    let args: RunArguments = serde_json::from_value(Value::Object(arguments))
        .map_err(|_| ErrorData::invalid_params("project_id와 command 문자열만 전달하세요", None))?;

    if let Err(message) = coordinator.gate(policy, &args.project_id, cancellation.clone()).await {

        return Ok(CallToolResult::structured_error(json!({"message": message})));

    }
    Ok(match policy.resolve(&args.project_id, &args.command) {

        Ok((root, command)) => match executor.run(root, command, cancellation).await {

            Ok(output) => {

                let success = output.success;
                let value = serde_json::to_value(output)
                    .map_err(|_| ErrorData::internal_error("실행 결과를 변환할 수 없습니다", None))?;

                if success { CallToolResult::structured(value) } else { CallToolResult::structured_error(value) }

            }
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),

        },
        Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),

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

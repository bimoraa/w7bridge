/*! 등록된 명령만 시작하고 서버가 발급한 handle로 제어해. */

use crate::{execution::Processes, security::Policy};
use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {

    project_id: String,
    command: String,

}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Handle {

    project_id: String,
    process_id: String,

}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {

    project_id: String,
    process_id: String,
    #[serde(default)]
    cursor: u64,

}

pub(crate) fn definitions() -> Vec<Tool> {

    ["start_process", "stop_process", "restart_process", "read_process_output", "list_processes"].into_iter().map(|name| {

        let mut properties = json!({ "project_id": {"type": "string"} });
        let selector = if name == "start_process" { "command" } else { "process_id" };
        if name != "list_processes" { properties[selector] = json!({"type": "string"}); }
        if name == "read_process_output" { properties["cursor"] = json!({"type": "integer", "minimum": 0}); }
        let schema = json!({"type": "object", "properties": properties, "required": if name == "list_processes" { vec!["project_id"] } else { vec!["project_id", selector] }, "additionalProperties": false});
        Tool::new(name, "등록된 명령의 process 수명과 cursor 기반 live output을 처리합니다", schema.as_object().cloned().unwrap_or_default())
            .with_annotations(ToolAnnotations::new().read_only(matches!(name, "read_process_output" | "list_processes"))
                .destructive(name != "read_process_output").idempotent(matches!(name, "read_process_output" | "list_processes")).open_world(true))

    }).collect()

}

pub(crate) async fn call(
    policy: &Policy,
    processes: &Processes,
    coordinator: &crate::sync::Coordinator,
    name: &str,
    args: Map<String, Value>,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<CallToolResult, ErrorData> {

    let invalid = || ErrorData::invalid_params("project_id와 process 도구 인자를 확인하세요", None);
    let result = match name {

        "list_processes" => {

            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct List {

                project_id: String,

            }
            let args: List = serde_json::from_value(Value::Object(args)).map_err(|_| invalid())?;
            match policy.requires_sync(&args.project_id) {

                Ok(_) => processes.list(&args.project_id),
                Err(error) => Err(error.to_string()),

            }

        }
        "start_process" => {

            let args: Start = serde_json::from_value(Value::Object(args)).map_err(|_| invalid())?;
            match coordinator.gate(policy, &args.project_id, cancellation.clone()).await {

                Ok(()) if !cancellation.is_cancelled() => {

                    processes.start(policy, &args.project_id, &args.command).await

                }
                Ok(()) => Err("시작 요청이 취소되었습니다".into()),
                Err(error) => Err(error),

            }

        }
        "read_process_output" => {

            let args: Read = serde_json::from_value(Value::Object(args)).map_err(|_| invalid())?;
            processes.read(&args.project_id, &args.process_id, args.cursor)

        }
        _ => {

            let args: Handle = serde_json::from_value(Value::Object(args)).map_err(|_| invalid())?;
            if name == "stop_process" {

                processes.stop(&args.project_id, &args.process_id).await

            } else {

                match coordinator.gate(policy, &args.project_id, cancellation).await {

                    Ok(()) => processes.restart(policy, &args.project_id, &args.process_id).await,
                    Err(error) => Err(error),

                }

            }

        }

    };
    Ok(match result {

        Ok(value) => CallToolResult::structured(value),
        Err(message) => CallToolResult::structured_error(json!({"message": message})),

    })

}

pub(crate) mod run_command;

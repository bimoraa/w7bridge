/*! 도구 발견과 이름별 dispatch의 단일 owner야. */

use super::handler::Bridge;
use crate::tools::{capture, execution, files, project, sync};
use rmcp::{
    ErrorData,
    model::{CallToolRequestParams, CallToolResult, Tool},
};
use tokio_util::sync::CancellationToken;
pub(crate) fn definitions(screenshots: bool) -> Vec<Tool> {

    let mut tools = vec![project::status::definition(), execution::run_command::definition()];
    tools.extend(files::definitions());
    tools.extend(execution::definitions());
    tools.extend(sync::definitions());
    if screenshots {

        tools.push(capture::definition());

    }
    tools

}

pub(crate) async fn call(
    bridge: &Bridge,
    request: CallToolRequestParams,
    cancellation: CancellationToken,
) -> Result<CallToolResult, ErrorData> {

    let arguments = request.arguments.unwrap_or_default();

    match request.name.as_ref() {

        "list_projects" => project::status::list_projects(&bridge.policy, arguments),
        "run_command" => {

            execution::run_command::run(&bridge.policy, &bridge.executor, &bridge.coordinator, arguments, cancellation)
                .await

        }
        name @ ("list_files" | "read_file" | "write_file") => {

            files::call(&bridge.policy, bridge.file_slots.clone(), name, arguments, cancellation).await

        }
        name @ ("start_process" | "stop_process" | "restart_process" | "read_process_output" | "list_processes") => {

            execution::call(&bridge.policy, &bridge.processes, &bridge.coordinator, name, arguments, cancellation).await

        }
        name @ ("sync_status" | "wait_for_sync" | "sync_checkpoint") => {

            sync::call(&bridge.policy, &bridge.coordinator, name, arguments, cancellation).await

        }
        "capture_screenshot" => capture::call(&bridge.capture, arguments, cancellation).await,
        _ => Err(ErrorData::invalid_params("등록되지 않은 도구입니다", None)),

    }

}

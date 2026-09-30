/*! 도구 발견과 이름별 dispatch의 단일 owner야. */

use super::handler::Bridge;
use crate::tools::{capture, execution, files, memory, project, sync};
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
    tools.extend(memory::definitions());
    tools.extend(project::health::definitions());
    tools.extend(files::chunks::definitions());
    tools.extend(files::history::definitions());
    tools.extend(project::git::definitions());
    if screenshots {

        tools.push(capture::definition());

    }
    tools

}

pub(crate) async fn call(
    bridge: &Bridge,
    request: CallToolRequestParams,
    cancellation: CancellationToken,
    output: Option<tokio::sync::mpsc::Sender<serde_json::Value>>,
) -> Result<CallToolResult, ErrorData> {

    let mut arguments = request.arguments.unwrap_or_default();
    let sync_peer = arguments
        .remove("sync_peer")
        .map(|value| {

            value
                .as_str()
                .map(str::to_owned)
                .filter(|peer| {

                    peer.len() == 64 && peer.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))

                })
                .ok_or_else(|| ErrorData::invalid_params("sync peer ID를 확인하세요", None))

        })
        .transpose()?;
    if sync_peer.is_some()
        && !matches!(
            request.name.as_ref(),
            "run_command"
                | "start_process"
                | "restart_process"
                | "sync_status"
                | "wait_for_sync"
                | "sync_checkpoint"
                | "project_status"
        )
    {

        return Err(ErrorData::invalid_params("이 도구에는 sync peer 인자가 없습니다", None));

    }
    let selected_project = arguments.get("project_id").and_then(serde_json::Value::as_str).map(str::to_owned);
    let selected_path = arguments.get("path").and_then(serde_json::Value::as_str).map(str::to_owned);

    let mut result = match request.name.as_ref() {

        "list_projects" => {

            let mut result = project::status::list_projects(
                &bridge.policy,
                bridge.codex.clone(),
                bridge.discovery.clone(),
                arguments,
            )
            .await?;
            if let Some(value) = result.structured_content.as_mut()
                && let Some(projects) = value["projects"].as_array_mut()
            {

                for project in projects {

                    project["device_id"] = serde_json::json!(bridge.device_id);
                    if let Some(id) = project["id"].as_str()
                        && let Ok(root) = bridge.policy.root(id)
                    {

                        project["root"] = serde_json::json!(root);

                    }

                }

            }
            Ok(match result.structured_content.take() {

                Some(value) => CallToolResult::structured(value),
                None => result,

            })

        }
        "run_command" => {

            execution::run_command::run(
                &bridge.policy,
                &bridge.processes,
                &bridge.coordinator,
                arguments,
                cancellation,
                sync_peer.as_deref(),
                output,
            )
            .await

        }
        name @ ("list_files" | "read_file" | "write_file") => {

            files::call(&bridge.policy, bridge.file_slots.clone(), name, arguments, cancellation).await

        }
        name @ ("start_process" | "stop_process" | "restart_process" | "read_process_output" | "list_processes") => {

            execution::call(
                &bridge.policy,
                &bridge.processes,
                &bridge.coordinator,
                name,
                arguments,
                cancellation,
                sync_peer.as_deref(),
            )
            .await

        }
        name @ ("sync_status" | "wait_for_sync" | "sync_checkpoint") => {

            let project = arguments.get("project_id").and_then(serde_json::Value::as_str).map(str::to_owned);
            let synced = arguments.get("status").and_then(serde_json::Value::as_str) == Some("synced");
            let result =
                sync::call(&bridge.policy, &bridge.coordinator, name, arguments, cancellation, sync_peer.as_deref())
                    .await?;
            if name == "sync_checkpoint"
                && result.is_error != Some(true)
                && synced
                && result
                    .structured_content
                    .as_ref()
                    .is_some_and(|value| value["accepted"] == true && value["changed"] == true)
                && let Some(project) = project
            {

                bridge.events.publish(
                    &project,
                    "sync_completed",
                    result.structured_content.clone().unwrap_or_default(),
                );
                let _ = bridge.processes.sync_restart(&bridge.policy, &project).await;

            }
            Ok(result)

        }
        name @ ("read_memory" | "update_memory") => {

            memory::call(&bridge.policy, bridge.file_slots.clone(), name, arguments, cancellation).await

        }
        "capture_screenshot" => capture::call(&bridge.capture, arguments, cancellation).await,
        name @ ("project_status" | "read_events" | "bridge_info") => {

            project::health::call(bridge, name, arguments, cancellation, sync_peer.as_deref()).await

        }
        name @ ("file_chunks" | "read_chunk" | "prepare_transfer" | "put_chunk" | "commit_transfer"
        | "abort_transfer") => {

            files::chunks::call(&bridge.policy, bridge.file_slots.clone(), name, arguments, cancellation).await

        }
        name @ ("sync_history" | "restore_file") => {

            files::history::call(&bridge.policy, bridge.file_slots.clone(), name, arguments, cancellation).await

        }
        name @ ("git_status" | "git_export" | "read_git_chunk" | "prepare_git_import" | "put_git_chunk"
        | "import_git") => {

            project::git::call(&bridge.policy, bridge.file_slots.clone(), name, arguments, cancellation).await

        }
        _ => Err(ErrorData::invalid_params("등록되지 않은 도구입니다", None)),

    }?;
    if let Some(project) = selected_project
        && result.is_error != Some(true)
    {

        if request.name == "list_files"
            && let Some(value) = result.structured_content.as_mut()
        {

            value["requires_sync"] = serde_json::json!(bridge.policy.requires_sync(&project).unwrap_or(false));
            result = CallToolResult::structured(value.clone());

        }
        if matches!(request.name.as_ref(), "write_file" | "commit_transfer" | "restore_file" | "update_memory") {

            let mut data = result.structured_content.clone().unwrap_or_default();
            if data["path"].is_null() {

                data["path"] = serde_json::json!(selected_path);

            }
            bridge.events.publish(&project, "file_synced", data);

        }

    }
    Ok(result)

}

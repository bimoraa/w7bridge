/*! MCP 도구 발견과 이름별 호출을 담당한다. */

use rmcp::{
    ErrorData,
    model::{CallToolRequestParams, CallToolResult, Tool},
};
use tokio_util::sync::CancellationToken;

use crate::{execution::Executor, security::Policy};

mod command;
mod filesystem;
mod process;
mod system;

pub(crate) fn definitions() -> Vec<Tool> {

    vec![system::definition(), command::definition()]

}

pub(crate) async fn call(
    policy: &Policy,
    executor: &Executor,
    request: CallToolRequestParams,
    cancellation: CancellationToken,
) -> Result<CallToolResult, ErrorData> {

    let arguments = request.arguments.unwrap_or_default();

    match request.name.as_ref() {

        "list_projects" => system::list_projects(policy, arguments),
        "run_command" => command::run(policy, executor, arguments, cancellation).await,
        _ => Err(ErrorData::invalid_params("등록되지 않은 도구입니다", None)),

    }

}

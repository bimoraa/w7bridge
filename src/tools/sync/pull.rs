use crate::{security::Policy, sync::Coordinator};
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {

    project_id: String,
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,

}
fn default_timeout() -> u64 {

    30

}
pub(super) async fn wait(
    policy: &Policy,
    coordinator: &Coordinator,
    args: Map<String, Value>,
    token: CancellationToken,
) -> Result<Result<Value, String>, ErrorData> {

    let args: Arguments = serde_json::from_value(Value::Object(args))
        .map_err(|_| ErrorData::invalid_params("sync 대기 인자가 올바르지 않습니다", None))?;
    let files = match policy.files(&args.project_id) {

        Ok(files) => files,
        Err(error) => return Ok(Err(error.to_string())),

    };
    Ok(coordinator.wait(&args.project_id, &files, args.timeout_seconds, token).await)

}

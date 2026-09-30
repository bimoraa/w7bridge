use crate::{security::Policy, sync::Coordinator};
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::{Map, Value};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {

    project_id: String,

}
pub(super) fn read(
    policy: &Policy,
    coordinator: &Coordinator,
    args: Map<String, Value>,
) -> Result<Result<Value, String>, ErrorData> {

    let args: Arguments = serde_json::from_value(Value::Object(args))
        .map_err(|_| ErrorData::invalid_params("project_id만 전달하세요", None))?;
    Ok(policy
        .files(&args.project_id)
        .map_err(|error| error.to_string())
        .and_then(|files| coordinator.status(&args.project_id, &files)))

}

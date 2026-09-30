use crate::{
    security::Policy,
    sync::{Checkpoint, Coordinator},
};
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::{Map, Value};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {

    project_id: String,
    generation: u64,
    status: String,
    manifest_hash: String,
    conflicts: Vec<String>,
    lease_seconds: u64,

}
pub(super) fn checkpoint(
    policy: &Policy,
    coordinator: &Coordinator,
    args: Map<String, Value>,
) -> Result<Result<Value, String>, ErrorData> {

    let args: Arguments = serde_json::from_value(Value::Object(args))
        .map_err(|_| ErrorData::invalid_params("checkpoint 인자가 올바르지 않습니다", None))?;
    Ok(policy.files(&args.project_id).map_err(|error| error.to_string()).and_then(|files| {

        coordinator.checkpoint(
            &args.project_id,
            &files,
            Checkpoint {

                generation: args.generation,
                status: &args.status,
                hash: &args.manifest_hash,
                conflicts: args.conflicts,
                lease_seconds: args.lease_seconds,

            },
        )

    }))

}

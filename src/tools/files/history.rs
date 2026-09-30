/*! history와 조건부 restore만 노출하고 임의 metadata 접근은 허용하지 않아. */

use crate::{FileError, security::Policy};
use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {

    project_id: String,
    #[serde(default)]
    revision_id: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    expected_hash: Option<String>,

}

pub(crate) fn definitions( ) -> Vec<Tool> {

    ["sync_history", "restore_file"]
        .into_iter()
        .map(|name| {

            let restore = name == "restore_file";
            let mut properties = json!({"project_id":{"type":"string"}});
            let mut required = vec!["project_id"];
            if restore {

                properties["revision_id"] = json!({"type":"string"});
                properties["version"] = json!({"type":"string","enum":["previous","result"]});
                properties["expected_hash"] = json!({"type":["string","null"]});
                required.extend(["revision_id", "version", "expected_hash"]);

            }
            let schema =
                json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
            Tool::new(
                name,
                "최근 파일 교체 기록을 읽거나 명시한 현재 hash가 일치할 때만 보존된 버전을 복구합니다",
                schema.as_object().cloned().unwrap_or_default(),
            )
            .with_annotations(
                ToolAnnotations::new().read_only(!restore).destructive(restore).idempotent(!restore).open_world(false),
            )

        })
        .collect()

}

pub(crate) async fn call( policy: &Policy, slots: Arc<Semaphore>, name: &str, arguments: Map<String, Value>, cancellation: CancellationToken, ) -> Result<CallToolResult, ErrorData> {

    let invalid = || ErrorData::invalid_params("history 인자와 현재 expected_hash를 확인하세요", None);
    if name == "sync_history" && arguments.keys().any(|key| key != "project_id") {

        return Err(invalid());

    }
    if name == "restore_file" && !arguments.contains_key("expected_hash") {

        return Err(invalid());

    }
    let args: Arguments = serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid())?;
    if name == "restore_file" && (args.revision_id.is_none() || args.version.is_none()) {

        return Err(invalid());

    }
    let store = match policy.files(&args.project_id) {

        Ok(store) => store,
        Err(error) => return Ok(super::failure(error)),

    };
    let permit = match slots.try_acquire_owned() {

        Ok(permit) => permit,
        Err(_) => return Ok(super::failure(FileError::Busy)),

    };
    let read_only = name == "sync_history";
    let result = tokio::task::spawn_blocking(move || {

        let _permit = permit;
        if cancellation.is_cancelled() {

            return Err(FileError::Cancelled);

        }
        if read_only {

            store.history()

        } else {

            store.restore(
                args.revision_id.as_deref().ok_or(FileError::Data)?,
                args.version.as_deref().ok_or(FileError::Data)?,
                args.expected_hash.as_deref(),
            )

        }

    })
    .await
    .map_err(|_| ErrorData::internal_error("history 작업을 완료할 수 없습니다", None))?;
    Ok(match result {

        Ok(value) => CallToolResult::structured(value),
        Err(error) => super::failure(error),

    })

}

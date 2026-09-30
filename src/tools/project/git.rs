/*! 등록된 Git executable로 검증한 archive만 전송·인계해. */

use crate::security::Policy;
use base64::{Engine, engine::general_purpose::STANDARD};
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
#[serde(tag = "operation", deny_unknown_fields)]
enum Arguments {

    #[serde(rename = "git_status")]
    Status { project_id: String },
    #[serde(rename = "git_export")]
    Export { project_id: String, expected_state: String },
    #[serde(rename = "read_git_chunk")]
    Read { project_id: String, sha256: String, index: usize },
    #[serde(rename = "prepare_git_import")]
    Prepare {

        project_id: String,
        sha256: String,
        bytes: usize,
        expected_state: Option<String>,
        expected_manifest: String,

    },
    #[serde(rename = "put_git_chunk")]
    Put { project_id: String, transfer_id: String, index: usize, sha256: String, content_base64: String },
    #[serde(rename = "import_git")]
    Import { project_id: String, transfer_id: String },

}

pub(crate) fn definitions( ) -> Vec<Tool> {

    ["git_status", "git_export", "read_git_chunk", "prepare_git_import", "put_git_chunk", "import_git"]
        .into_iter()
        .map(|name| {

            let mut properties = json!({"project_id":{"type":"string"}});
            let mut required = vec!["project_id"];
            for key in match name {

                "git_export" => vec!["expected_state"],
                "read_git_chunk" => vec!["sha256", "index"],
                "prepare_git_import" => vec!["sha256", "bytes", "expected_state", "expected_manifest"],
                "put_git_chunk" => vec!["transfer_id", "index", "sha256", "content_base64"],
                "import_git" => vec!["transfer_id"],
                _ => vec![],

            } {

                properties[key] = match key {

                    "expected_state" if name == "prepare_git_import" => json!({"type":["string","null"]}),
                    "index" => json!({"type":"integer","minimum":0,"maximum":1023}),
                    "bytes" => json!({"type":"integer","minimum":1,"maximum":67108864}),
                    "content_base64" => json!({"type":"string","maxLength":87384}),
                    _ => json!({"type":"string"}),

                };
                required.push(key);

            }
            let schema =
                json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
            let read_only = matches!(name, "git_status" | "read_git_chunk");
            Tool::new(
                name,
                "bundle와 index를 인계하고 새 대상 변경·credential·임의 Git 명령은 거부합니다",
                schema.as_object().cloned().unwrap_or_default(),
            )
            .with_annotations(
                ToolAnnotations::new()
                    .read_only(read_only)
                    .destructive(name == "import_git")
                    .idempotent(read_only || name == "put_git_chunk")
                    .open_world(false),
            )

        })
        .collect()

}

pub(crate) async fn call( policy: &Policy, slots: Arc<Semaphore>, name: &str, mut arguments: Map<String,Value>, cancellation: CancellationToken, ) -> Result<CallToolResult,ErrorData> {

    let invalid = || ErrorData::invalid_params("Git 인자, 필수 expected_state와 한도를 확인하세요", None);
    if arguments.contains_key("operation") || name == "prepare_git_import" && !arguments.contains_key("expected_state")
    {

        return Err(invalid());

    }
    arguments.insert("operation".into(), json!(name));
    let args: Arguments = serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid())?;
    let project = match &args {

        Arguments::Status { project_id }
        | Arguments::Export { project_id, .. }
        | Arguments::Read { project_id, .. }
        | Arguments::Prepare { project_id, .. }
        | Arguments::Put { project_id, .. }
        | Arguments::Import { project_id, .. } => project_id,

    };
    let repository = match policy.git(project) {

        Ok(repository) => repository,
        Err(error) => return Ok(CallToolResult::structured_error(json!({"message":error}))),

    };
    let _permit = match slots.try_acquire_owned() {

        Ok(permit) => permit,
        Err(_) => return Ok(CallToolResult::structured_error(json!({"message":"Git 작업 slot이 사용 중입니다"}))),

    };
    let result = match args {

        Arguments::Status { .. } => repository.status(cancellation).await,
        Arguments::Export { expected_state, .. } => repository.export(&expected_state, cancellation).await,
        Arguments::Read { sha256, index, .. } => repository.read_export(&sha256, index),
        Arguments::Prepare { sha256, bytes, expected_state, expected_manifest, .. } => {

            repository.prepare_import(&sha256, bytes, expected_state, expected_manifest)

        }
        Arguments::Put { transfer_id, index, sha256, content_base64, .. } => {

            if content_base64.len() > 87384 {

                return Err(invalid());

            }
            let bytes = STANDARD.decode(content_base64).map_err(|_| invalid())?;
            repository.put_import(&transfer_id, index, &bytes, &sha256)

        }
        Arguments::Import { transfer_id, .. } => repository.apply_import(&transfer_id, cancellation).await,

    };
    Ok(match result {

        Ok(value) => CallToolResult::structured(value),
        Err(error) => CallToolResult::structured_error(json!({"message":error})),

    })

}

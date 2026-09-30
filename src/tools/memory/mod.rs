/*! project context를 UTF-8 문서로 읽고 조건부 갱신하는 MCP 도구야. */

use crate::{FileError, memory::ProjectMemory, security::Policy, tools::files::failure};
use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

mod read;
mod update;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArguments {

    project_id: String,
    #[serde(default = "default_path")]
    path: String,

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArguments {

    project_id: String,
    #[serde(default = "default_path")]
    path: String,
    content: String,
    expected_hash: Option<String>,

}

fn default_path() -> String {

    "MEMORY.md".into()

}

pub(crate) fn definitions() -> Vec<Tool> {

    ["read_memory", "update_memory"].into_iter().map(|name| {

        let mut properties = json!({
            "project_id": { "type": "string", "description": "등록된 프로젝트 ID" },
            "path": { "type": "string", "default": "MEMORY.md", "description": "등록된 context 상대 경로; 기본 MEMORY.md" }
        });
        let mut required = vec!["project_id"];
        if name == "update_memory" {

            properties["content"] = json!({"type": "string", "description": "새 UTF-8 문서 전체, 최대 1 MiB"});
            properties["expected_hash"] = json!({"type": ["string", "null"], "description": "read_memory의 SHA-256; null은 신규 파일만 생성"});
            required.extend(["content", "expected_hash"]);

        }
        let schema = json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false});
        let description = if name == "read_memory" {

            "project context 문서와 현재 hash를 읽습니다. 없는 파일은 exists=false, content/sha256=null입니다"

        } else {

            "기존 hash가 일치할 때만 context를 갱신합니다. 저장 성공은 파일 반영이며 peer sync 완료는 wait_for_sync로 확인하세요"

        };
        Tool::new(name, description, schema.as_object().cloned().unwrap_or_default())
            .with_annotations(ToolAnnotations::new().read_only(name == "read_memory")
                .destructive(name == "update_memory").idempotent(name == "read_memory").open_world(false))

    }).collect()

}

pub(crate) async fn call(
    policy: &Policy,
    slots: Arc<Semaphore>,
    name: &str,
    arguments: Map<String, Value>,
    cancellation: CancellationToken,
) -> Result<CallToolResult, ErrorData> {

    let invalid = || ErrorData::invalid_params("project_id, context 경로와 memory 인자를 확인하세요", None);
    let (project, path, update) = match name {

        "read_memory" => {

            let args: ReadArguments = serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid())?;
            (args.project_id, args.path, None)

        }
        "update_memory" => {

            if !arguments.contains_key("expected_hash") {

                return Err(invalid());

            }
            let args: UpdateArguments = serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid())?;
            if args.expected_hash.as_ref().is_some_and(|hash| {

                hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))

            }) {

                return Err(invalid());

            }
            if args.content.len() > 1_048_576 {

                return Ok(failure(FileError::Limit));

            }
            (args.project_id, args.path, Some((args.content, args.expected_hash)))

        }
        _ => return Err(ErrorData::invalid_params("등록되지 않은 memory 도구입니다", None)),

    };
    let memory = match ProjectMemory::open(policy, &project) {

        Ok(memory) => memory,
        Err(error) => return Ok(failure(error)),

    };
    let permit = match slots.try_acquire_owned() {

        Ok(permit) => permit,
        Err(_) => return Ok(failure(FileError::Busy)),

    };
    let result = tokio::task::spawn_blocking(move || {

        let _permit = permit;
        if cancellation.is_cancelled() {

            return Err(FileError::Cancelled);

        }
        match update {

            Some((content, expected)) => update::execute(&memory, &path, &content, expected.as_deref()),
            None => read::execute(&memory, &path),

        }

    })
    .await
    .map_err(|_| ErrorData::internal_error("memory 작업을 완료할 수 없습니다", None))?;
    Ok(match result {

        Ok(value) => CallToolResult::structured(value),
        Err(error) => failure(error),

    })

}

#[cfg(test)]
#[path = "../../../tests/unit/memory_tools.rs"]
mod tests;

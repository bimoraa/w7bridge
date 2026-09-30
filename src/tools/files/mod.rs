/*! registry가 활성화한 프로젝트 파일만 목록·읽기·조건부 쓰기로 공개해. */

use crate::protocol::request::{ListArgs, ReadArgs, WriteArgs};
use crate::{FileError, security::Policy};
use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub(crate) fn definitions() -> Vec<Tool> {

    ["list_files", "read_file", "write_file"].into_iter().map(|name| {

        let mut properties = json!({ "project_id": { "type": "string" } });
        let mut required = vec!["project_id"];
        if name != "list_files" {

            properties["path"] = json!({ "type": "string", "description": "project root 기준 상대 경로, 구분자는 /" });
            required.push("path");

        }
        if name == "write_file" {

            properties["content_base64"] = json!({ "type": ["string", "null"], "description": "파일 내용; null은 삭제" });
            properties["expected_hash"] = json!({ "type": ["string", "null"], "description": "기존 SHA-256; null은 신규 파일" });
            required.extend(["content_base64", "expected_hash"]);

        }
        let schema = json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false });
        Tool::new(name, "활성화된 project root 안에서 공통 제외 정책과 파일 한도를 적용합니다", schema.as_object().cloned().unwrap_or_default())
            .with_annotations(ToolAnnotations::new().read_only(name != "write_file")
                .destructive(name == "write_file").idempotent(name != "write_file").open_world(false))

    }).collect()

}

pub(crate) async fn call(
    policy: &Policy,
    slots: Arc<Semaphore>,
    name: &str,
    arguments: Map<String, Value>,
    cancellation: CancellationToken,
) -> Result<CallToolResult, ErrorData> {

    let invalid = || ErrorData::invalid_params("파일 도구의 필수 인자와 형식을 확인하세요", None);
    let (project, path, data, expected) = match name {

        "list_files" => {

            let args: ListArgs = serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid())?;
            (args.project_id, None, None, None)

        }
        "read_file" => {

            let args: ReadArgs = serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid())?;
            (args.project_id, Some(args.path), None, None)

        }
        _ => {

            if !arguments.contains_key("content_base64") || !arguments.contains_key("expected_hash") {

                return Err(invalid());

            }
            let args: WriteArgs = serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid())?;
            if args.content_base64.as_ref().is_some_and(|content| content.len() > 1_398_104) {

                return Err(invalid());

            }
            let data =
                args.content_base64.map(|content| STANDARD.decode(content).map_err(|_| invalid())).transpose()?;
            (args.project_id, Some(args.path), data, args.expected_hash)

        }

    };
    let operation = name.to_owned();
    let store = match policy.files(&project) {

        Ok(store) => store,
        Err(error) => return Ok(failure(error)),

    };
    let permit = match slots.try_acquire_owned() {

        Ok(permit) => permit,
        Err(_) => return Ok(failure(FileError::Busy)),

    };
    let result = tokio::task::spawn_blocking(move || -> Result<Value, FileError> {

        let _permit = permit;
        if cancellation.is_cancelled() {

            return Err(FileError::Cancelled);

        }
        match operation.as_str() {

            "list_files" => search::execute(&store),
            "read_file" => read::execute(&store, path.as_deref().ok_or(FileError::Data)?),
            _ => write::execute(&store, path.as_deref().ok_or(FileError::Data)?, data.as_deref(), expected.as_deref()),

        }

    })
    .await
    .map_err(|_| ErrorData::internal_error("파일 작업을 완료할 수 없습니다", None))?;
    Ok(match result {

        Ok(value) => CallToolResult::structured(value),
        Err(error) => failure(error),

    })

}

fn failure(error: FileError) -> CallToolResult {

    let code = match &error {

        FileError::Disabled => "disabled",
        FileError::Path => "path",
        FileError::Conflict => "conflict",
        FileError::Busy => "busy",
        FileError::Cancelled => "cancelled",
        FileError::Limit => "limit",
        FileError::Io(error) if error.kind() == std::io::ErrorKind::NotFound => "not_found",
        FileError::Io(_) => "io",
        FileError::Data => "data",

    };
    CallToolResult::structured_error(json!({ "code": code, "message": error.to_string() }))

}

mod read;
mod search;
mod write;

#[cfg(test)]
#[path = "../../../tests/unit/file_tools.rs"]
mod tests;

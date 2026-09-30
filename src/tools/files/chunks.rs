/*! chunk 전송의 경로·hash·한도를 검사하고 기존 파일 slot을 재사용해. */

use crate::{FileError, filesystem::chunks::Chunk, security::Policy};
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

    #[serde(rename = "file_chunks")]
    Manifest { project_id: String, path: String },
    #[serde(rename = "read_chunk")]
    Read { project_id: String, path: String, index: usize, expected_hash: String },
    #[serde(rename = "prepare_transfer")]
    Prepare { project_id: String, path: String, expected_hash: Option<String>, sha256: String, chunks: Vec<Chunk> },
    #[serde(rename = "put_chunk")]
    Put { project_id: String, transfer_id: String, index: usize, content_base64: String },
    #[serde(rename = "commit_transfer")]
    Commit { project_id: String, transfer_id: String },
    #[serde(rename = "abort_transfer")]
    Abort { project_id: String, transfer_id: String },

}

pub(crate) fn definitions( ) -> Vec<Tool> {

    ["file_chunks","read_chunk","prepare_transfer","put_chunk","commit_transfer","abort_transfer"].into_iter().map(|name| {

        let mut properties = json!({"project_id":{"type":"string"}});
        let mut required = vec!["project_id"];
        if matches!(name,"file_chunks"|"read_chunk"|"prepare_transfer") {

            properties["path"] = json!({"type":"string"}); required.push("path");

        } else {

            properties["transfer_id"] = json!({"type":"string"}); required.push("transfer_id");

        }
        if name == "read_chunk" || name == "put_chunk" {

            properties["index"] = json!({"type":"integer","minimum":0,"maximum":1023}); required.push("index");

        }
        if name == "read_chunk" || name == "prepare_transfer" {

            properties["expected_hash"] = if name=="read_chunk" { json!({"type":"string"}) } else { json!({"type":["string","null"]}) };
            required.push("expected_hash");

        }
        if name == "prepare_transfer" {

            properties["sha256"] = json!({"type":"string"});
            properties["chunks"] = json!({"type":"array","maxItems":1024,"items":{"type":"object","properties":{"sha256":{"type":"string"},"bytes":{"type":"integer","minimum":1,"maximum":65536}},"required":["sha256","bytes"],"additionalProperties":false}});
            required.extend(["sha256","chunks"]);

        }
        if name == "put_chunk" {

            properties["content_base64"] = json!({"type":"string","maxLength":87384}); required.push("content_base64");

        }
        let schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
        let read_only = matches!(name,"file_chunks"|"read_chunk");
        Tool::new(name,"검증한 chunk만 전송하고 최종 hash와 기존 revision이 일치할 때 파일을 교체합니다",schema.as_object().cloned().unwrap_or_default())
            .with_annotations(ToolAnnotations::new().read_only(read_only).destructive(!read_only).idempotent(read_only||name=="put_chunk").open_world(false))

    }).collect()

}

pub(crate) async fn call( policy: &Policy, slots: Arc<Semaphore>, name: &str, mut arguments: Map<String,Value>, cancellation: CancellationToken, ) -> Result<CallToolResult,ErrorData> {

    let invalid = || ErrorData::invalid_params("chunk 도구의 필수 인자와 형식을 확인하세요", None);
    if arguments.contains_key("operation") || name == "prepare_transfer" && !arguments.contains_key("expected_hash") {

        return Err(invalid());

    }
    arguments.insert("operation".into(), json!(name));
    let args: Arguments = serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid())?;
    let project = match &args {

        Arguments::Manifest { project_id, .. }
        | Arguments::Read { project_id, .. }
        | Arguments::Prepare { project_id, .. }
        | Arguments::Put { project_id, .. }
        | Arguments::Commit { project_id, .. }
        | Arguments::Abort { project_id, .. } => project_id,

    };
    let store = match policy.files(project) {

        Ok(store) => store,
        Err(error) => return Ok(super::failure(error)),

    };
    let permit = match slots.try_acquire_owned() {

        Ok(permit) => permit,
        Err(_) => return Ok(super::failure(FileError::Busy)),

    };
    let result = tokio::task::spawn_blocking(move || -> Result<Value, FileError> {

        let _permit = permit;
        if cancellation.is_cancelled() {

            return Err(FileError::Cancelled);

        }
        match args {

            Arguments::Manifest { path, .. } => store.chunk_manifest(&path),
            Arguments::Read { path, index, expected_hash, .. } => {

                Ok(json!({"content_base64":STANDARD.encode(store.read_chunk(&path,index,&expected_hash)?)}))

            }
            Arguments::Prepare { path, expected_hash, sha256, chunks, .. } => {

                store.prepare_transfer(&path, expected_hash.as_deref(), &sha256, chunks)

            }
            Arguments::Put { transfer_id, index, content_base64, .. } => {

                if content_base64.len() > 87384 {

                    return Err(FileError::Limit);

                }
                let content = STANDARD.decode(content_base64).map_err(|_| FileError::Data)?;
                store.put_chunk(&transfer_id, index, &content)?;
                Ok(json!({"accepted":true,"bytes":content.len()}))

            }
            Arguments::Commit { transfer_id, .. } => store.commit_transfer(&transfer_id),
            Arguments::Abort { transfer_id, .. } => store.abort_transfer(&transfer_id),

        }

    })
    .await
    .map_err(|_| ErrorData::internal_error("chunk 작업을 완료할 수 없습니다", None))?;
    Ok(match result {

        Ok(value) => CallToolResult::structured(value),
        Err(error) => super::failure(error),

    })

}

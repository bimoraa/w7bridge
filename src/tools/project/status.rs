/*! 명시적인 registry와 Codex의 최신 로컬 project 목록을 함께 보여줘. */

use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde_json::{Map, Value, json};

use crate::security::{Policy, codex::Discovery};
use std::sync::Arc;

pub(crate) fn definition() -> Tool {

    let schema = Map::from_iter([
        ("type".into(), json!("object")),
        ("properties".into(), json!({})),
        ("additionalProperties".into(), json!(false)),
    ]);

    Tool::new(
        "list_projects",
        "Codex 로컬 프로젝트와 등록된 명령을 조회합니다. 자동 발견은 실행·파일 권한을 추가하지 않습니다",
        schema,
    )
    .with_annotations(ToolAnnotations::new().read_only(true).idempotent(true).open_world(false))

}

pub(crate) async fn list_projects( policy: &Policy, discovery: Arc<Discovery>, arguments: Map<String, Value>, ) -> Result<CallToolResult, ErrorData> {

    if !arguments.is_empty() {

        return Err(ErrorData::invalid_params("list_projects에는 인자를 전달하지 마세요", None));

    }

    let snapshot = tokio::task::spawn_blocking(move || discovery.read())
        .await
        .map_err(|_| ErrorData::internal_error("Codex project 조회 작업을 완료할 수 없습니다", None))?;
    let mut projects = policy.list().into_iter().map(|project| json!(project)).collect::<Vec<_>>();
    for project in &snapshot.projects {

        let registered = project.roots.iter().find_map(|root| policy.project_at(root));
        if let Some(id) = registered {

            if let Some(entry) = projects.iter_mut().find(|entry| entry["id"] == id) {

                entry["name"] = json!(project.name);
                entry["roots"] = json!(project.roots);
                entry["available"] = json!(project.available);
                entry["codex_id"] = json!(project.id);
                entry["source"] = json!("config");

            }

        } else if !projects.iter().any(|entry| entry["id"] == project.id) {

            let mut entry = json!(project);
            entry["commands"] = json!([]);
            entry["source"] = json!("codex");
            projects.push(entry);

        }

    }
    Ok(CallToolResult::structured(json!({ "projects": projects, "codex": snapshot })))

}

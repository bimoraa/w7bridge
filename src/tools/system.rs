/*! 현재는 등록된 프로젝트 목록만 제공한다. host 정보 조회는 필요할 때 확장한다. */

use rmcp::{
    ErrorData,
    model::{CallToolResult, Tool, ToolAnnotations},
};
use serde_json::{Map, Value, json};

use crate::security::Policy;

pub(super) fn definition() -> Tool {

    let schema = Map::from_iter([
        ("type".into(), json!("object")),
        ("properties".into(), json!({})),
        ("additionalProperties".into(), json!(false)),
    ]);

    Tool::new("list_projects", "등록된 프로젝트와 명령 이름을 조회합니다", schema)
        .with_annotations(ToolAnnotations::new().read_only(true).idempotent(true).open_world(false))

}

pub(super) fn list_projects(policy: &Policy, arguments: Map<String, Value>) -> Result<CallToolResult, ErrorData> {

    if !arguments.is_empty() {

        return Err(ErrorData::invalid_params("list_projects에는 인자를 전달하지 마세요", None));

    }

    Ok(CallToolResult::structured(json!({ "projects": policy.list() })))

}

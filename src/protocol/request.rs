/*! 활성화된 MCP 도구의 입력 schema를 소유해. */

use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListArgs {

    pub project_id: String,

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadArgs {

    pub project_id: String,
    pub path: String,

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteArgs {

    pub project_id: String,
    pub path: String,
    pub content_base64: Option<String>,
    pub expected_hash: Option<String>,

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunArguments {

    pub project_id: String,
    pub command: String,

}

/*! 명령 실행 결과의 wire 형식을 유지해. */

use serde::Serialize;
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {

    Completed,
    TimedOut,
    Cancelled,

}

#[derive(Debug, Serialize)]
pub(crate) struct Output {

    pub status: Status,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,

}

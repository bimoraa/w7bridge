use std::io;
use thiserror::Error;
#[derive(Debug, Error)]
pub(crate) enum ExecutionError {

    #[error("실행 한도를 초과했습니다. 진행 중인 명령이 끝난 뒤 다시 요청하세요")]
    Busy,
    #[error("요청이 취소되었거나 서버가 종료 중입니다")]
    Cancelled,
    #[error("프로세스 작업에 실패했습니다: {operation}")]
    Process {

        operation: &'static str,
        #[source]
        source: io::Error,

    },
    #[error("프로세스 종료 확인 시간이 초과되었습니다")]
    CleanupTimeout,

}

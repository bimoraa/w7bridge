use std::io;
use thiserror::Error;
/** project root 안의 파일 접근, 버전 검사와 한도 위반을 구분한다. */
#[derive(Debug, Error)]
pub enum FileError {

    #[error("파일 공유가 활성화되지 않았거나 프로젝트가 없습니다")]
    Disabled,
    #[error("공유할 수 없는 경로 또는 symlink입니다")]
    Path,
    #[error("파일이 다른 작업에서 변경되었습니다")]
    Conflict,
    #[error("다른 파일 작업이 진행 중입니다")]
    Busy,
    #[error("파일 작업이 시작되기 전에 요청이 취소되었습니다")]
    Cancelled,
    #[error("파일 공유 한도를 초과했습니다 (chunk 64 KiB, 파일 설정 한도, 10000개, 합계 256 MiB)")]
    Limit,
    #[error("파일 작업에 실패했습니다")]
    Io(#[from] io::Error),
    #[error("파일 데이터 형식이 올바르지 않습니다")]
    Data,

}

/** peer transport와 정책 실패를 구분한다. offline은 자동 reconnect 대상이다. */
#[derive(Debug, Error)]
pub enum SyncError {

    #[error("파일 sync 작업에 실패했습니다: {0}")]
    File(#[from] FileError),
    #[error("peer 연결이 끊겼거나 응답 시간이 초과되었습니다")]
    Offline,
    #[error("peer가 파일 요청을 거부했거나 응답 형식이 올바르지 않습니다")]
    Peer,
    #[error("peer 요청 실패: {0}")]
    PeerRequest(String),
    #[error("peer project 설정을 확인하세요: {0}")]
    PeerConfig(&'static str),
    #[error("sync metadata가 손상되었거나 다른 pairing에 속합니다")]
    State,
    #[error("Git handoff를 완료할 수 없습니다: {0}")]
    Git(String),
    #[error("sync metadata를 직렬화할 수 없습니다")]
    Json(#[from] serde_json::Error),

}

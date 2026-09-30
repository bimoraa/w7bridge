/*! 설정, 정책, 실행 경계의 실패를 한곳에 정의한다. */

use std::io;
use thiserror::Error;

/** 로컬 설정을 읽거나 검증할 때 발생하는 실패다. */
#[derive(Debug, Error)]
pub enum ConfigError {

    #[error("설정 파일을 읽을 수 없습니다")]
    Read(#[source] std::io::Error),
    #[error("설정 TOML 형식이 올바르지 않습니다. 필수 항목과 알 수 없는 항목을 확인하세요")]
    Parse(#[source] toml::de::Error),
    #[error("설정 값이 올바르지 않습니다: {0}")]
    Invalid(&'static str),

}

/** registry 구성 또는 명령 선택을 거부한 이유다. */
#[derive(Debug, Error)]
pub enum PolicyError {

    #[error("프로젝트와 명령 이름은 1..=64자의 영문 소문자, 숫자, 밑줄, 하이픈이어야 합니다")]
    Name,
    #[error("프로젝트 ID가 중복되었습니다")]
    Duplicate,
    #[error("존재하는 절대 디렉터리 경로가 필요합니다")]
    Root,
    #[error("존재하는 절대 실행 파일 경로가 필요합니다. Windows에서는 .exe만 허용합니다")]
    Executable,
    #[error("명령 인자 또는 환경 변수에 사용할 수 없는 문자가 있습니다")]
    Command,
    #[error("프로젝트 또는 명령이 등록되지 않았습니다")]
    Unknown,
    #[error("등록된 경로가 변경되었거나 삭제되었습니다")]
    Changed,

}

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

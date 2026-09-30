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
    #[error("파일 공유 설정이 올바르지 않습니다")]
    Files,

}

/** Mac의 SSH 연결 확인과 Codex 등록 실패를 구분한다. 실패한 연결은 등록하지 않는다. */
#[derive(Debug, Error)]
pub enum ConnectError {

    #[error("연결 인자가 올바르지 않습니다: {0}. connect --help로 사용법을 확인하세요")]
    Argument(&'static str),
    #[error("연결 설정을 직렬화할 수 없습니다")]
    Config(#[from] toml::ser::Error),
    #[error("프로세스를 실행하거나 기다릴 수 없습니다: {operation}")]
    Process {

        operation: &'static str,
        #[source]
        source: std::io::Error,

    },
    #[error("MCP 연결 확인에 실패했습니다. SSH 인증, host key, Windows 경로와 서버 stderr를 확인하세요")]
    Protocol,
    #[error("w7bridge 서버와 필수 도구를 확인할 수 없습니다")]
    Server,
    #[error("연결 확인 시간이 초과되었습니다")]
    Timeout,
    #[error("연결 확인이 취소되었습니다")]
    Cancelled,
    #[error("SSH 프로세스 종료 확인에 실패했습니다")]
    Cleanup,
    #[error("Codex MCP 목록을 읽을 수 없습니다. codex mcp list --json을 확인하세요")]
    CodexList,
    #[error("같은 이름의 Codex MCP가 이미 있습니다. --name으로 다른 이름을 지정하세요")]
    Exists,
    #[error("Codex MCP 등록에 실패했습니다")]
    CodexAdd,

}

/** Windows 영구 설치의 인자, 설정 검증과 파일 배치 실패를 구분한다. */
#[derive(Debug, Error)]
pub enum InstallError {

    #[error("설치 인자가 올바르지 않습니다: {0}. install --help로 사용법을 확인하세요")]
    Argument(&'static str),
    #[error("영구 설치는 Windows에서 실행하세요. Mac에서는 connect를 사용하세요")]
    Platform,
    #[error("실행 파일이 이미 있습니다. 새 build에서 --update로 갱신하세요")]
    Exists,
    #[error("설정이 이미 있습니다. --config를 빼면 기존 설정을 그대로 사용합니다")]
    ConfigExists,
    #[error("설치된 실행 파일에서 자신을 갱신할 수 없습니다. 다른 경로의 새 build를 실행하세요")]
    SameExecutable,
    #[error("설정 검증에 실패했습니다: {0}")]
    Config(#[from] ConfigError),
    #[error("registry 검증에 실패했습니다: {0}")]
    Policy(#[from] PolicyError),
    #[error("설치 파일 작업에 실패했습니다: {operation}: {source}")]
    File {

        operation: &'static str,
        #[source]
        source: io::Error,

    },

}

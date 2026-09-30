use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
/**
version 1의 검증된 설정이다. 필드는 외부에서 수정할 수 없다.

설정 변경은 재시작으로 적용한다. 프로젝트 경로 검증은 `Bridge::new`에서 수행한다.
*/
#[derive(Debug)]
pub struct Config {

    pub(crate) codex: CodexSettings,
    pub(crate) execution: ExecutionLimits,
    pub(crate) screenshots: ScreenshotSettings,
    #[cfg(windows)]
    pub(crate) service: ServiceSettings,
    pub(crate) projects: Vec<ProjectDefinition>,

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawConfig {

    pub(super) version: u32,
    #[serde(default)]
    pub(super) codex: CodexSettings,
    #[serde(default)]
    pub(super) screenshots: ScreenshotSettings,
    #[serde(default)]
    pub(super) service: ServiceSettings,
    #[serde(default)]
    pub(super) execution: ExecutionLimits,
    #[serde(default)]
    pub(super) projects: Vec<ProjectDefinition>,

}

/** Codex의 로컬 project 목록만 읽는다. 발견한 project에 실행·파일 권한을 추가하지 않는다. */
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CodexSettings {

    pub enabled: bool,
    pub home: Option<PathBuf>,

}

impl Default for CodexSettings {

    fn default() -> Self {

        Self { enabled: true, home: None }

    }

}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ExecutionLimits {

    pub timeout_seconds: u64,
    pub output_bytes: usize,
    pub concurrency: usize,

}

impl Default for ExecutionLimits {

    fn default() -> Self {

        Self { timeout_seconds: 60, output_bytes: 65_536, concurrency: 1 }

    }

}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectDefinition {

    pub id: String,
    pub root: PathBuf,
    #[serde(default)]
    pub requires_sync: bool,
    #[serde(default)]
    pub files: FileSettings,
    #[serde(default)]
    pub commands: BTreeMap<String, CommandDefinition>,

}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandDefinition {

    pub executable: PathBuf,
    #[serde(default)]
    pub background: bool,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,

}

/** 로컬 config 소유자가 정하는 파일 공유 정책이다. 기본값은 비활성화다. */
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileSettings {

    pub enabled: bool,
    pub exclude_dirs: Vec<String>,
    pub context_files: Vec<String>,

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SyncSettings {

    pub version: u32,
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,
    pub pairs: Vec<Pair>,

}
fn default_interval() -> u64 {

    2

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pair {

    pub local_root: PathBuf,
    pub remote_project: String,
    pub host: String,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub config: Option<String>,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub service: bool,

}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceSettings {

    pub allowed_sid: Option<String>,

}

/** 명시적인 MCP screenshot 요청만 허용한다. 기본 비활성화이며 OS desktop 권한은 추가하지 않는다. */
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ScreenshotSettings {

    pub enabled: bool,

}

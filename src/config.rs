use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::error::ConfigError;
use serde::Deserialize;

/**
version 1의 검증된 설정이다. 필드는 외부에서 수정할 수 없다.

설정 변경은 재시작으로 적용한다. 프로젝트 경로 검증은 `Bridge::new`에서 수행한다.
*/
#[derive(Debug)]
pub struct Config {

    pub(crate) execution: ExecutionLimits,
    pub(crate) projects: Vec<ProjectDefinition>,

}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {

    version: u32,
    #[serde(default)]
    execution: ExecutionLimits,
    #[serde(default)]
    projects: Vec<ProjectDefinition>,

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
    pub commands: BTreeMap<String, CommandDefinition>,

}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandDefinition {

    pub executable: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,

}

impl Config {

    /**
    UTF-8 TOML 파일을 읽어 검증된 설정을 만든다.

    # Errors

    파일 읽기 실패, TOML 오류, 알 수 없는 항목, 지원하지 않는 version 또는 실행 한도는 오류다.
    경로 문자열의 환경 변수와 `~`는 확장하지 않는다.
    */
    pub fn load(path: &Path) -> Result<Self, ConfigError> {

        Self::parse(&fs::read_to_string(path).map_err(ConfigError::Read)?)

    }

    /** TOML 문자열을 해석한다. 파일 읽기를 제외한 오류 조건은 `load`와 같다. */
    pub fn parse(source: &str) -> Result<Self, ConfigError> {

        let config: RawConfig = toml::from_str(source).map_err(ConfigError::Parse)?;

        if config.version != 1 {

            return Err(ConfigError::Invalid("version은 1이어야 합니다"));

        }

        let limits = &config.execution;

        if !(1..=3600).contains(&limits.timeout_seconds) {

            return Err(ConfigError::Invalid("timeout_seconds는 1..=3600이어야 합니다"));

        }

        if !(1024..=1_048_576).contains(&limits.output_bytes) {

            return Err(ConfigError::Invalid("output_bytes는 1024..=1048576이어야 합니다"));

        }

        if !(1..=8).contains(&limits.concurrency) {

            return Err(ConfigError::Invalid("concurrency는 1..=8이어야 합니다"));

        }

        Ok(Self { execution: config.execution, projects: config.projects })

    }

}

#[cfg(test)]
#[path = "../tests/unit/config.rs"]
mod tests;

use super::{Config, model::RawConfig};
use crate::error::ConfigError;
use std::{fs, path::Path};
impl Config {

    /**
    UTF-8 TOML 파일을 읽어 검증된 설정을 만든다.

    # Errors

    파일 읽기 실패, TOML 오류, 알 수 없는 항목, 지원하지 않는 version 또는 실행 한도는 오류다.
    경로 문자열의 환경 변수와 `~`는 확장하지 않는다.
    */
    pub fn load(path: &Path) -> Result<Self, ConfigError> {

        let path = path.canonicalize().map_err(ConfigError::Read)?;
        let mut config = Self::parse(&fs::read_to_string(&path).map_err(ConfigError::Read)?)?;
        for project in &mut config.projects {

            if let Ok(root) = project.root.canonicalize()
                && let Ok(relative) = path.strip_prefix(root)
            {

                let relative = relative
                    .to_str()
                    .ok_or(ConfigError::Invalid("공유 root의 설정 경로는 UTF-8이어야 합니다"))?
                    .replace('\\', "/");
                if !project.files.exclude_dirs.contains(&relative) {

                    project.files.exclude_dirs.push(relative);

                }

            }

        }
        Ok(config)

    }

    /** TOML 문자열을 해석한다. 파일 읽기를 제외한 오류 조건은 `load`와 같다. */
    pub fn parse(source: &str) -> Result<Self, ConfigError> {

        let config: RawConfig = toml::from_str(source).map_err(ConfigError::Parse)?;

        if config.codex.home.as_ref().is_some_and(|home| !home.is_absolute()) {

            return Err(ConfigError::Invalid("codex.home은 절대 디렉터리 경로여야 합니다"));

        }

        if config.version != 1 {

            return Err(ConfigError::Invalid("version은 1이어야 합니다"));

        }
        if config.device_id.as_ref().is_some_and(|id| {

            id.is_empty()
                || id.len() > 64
                || !id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))

        }) {

            return Err(ConfigError::Invalid("device_id는 1..=64자의 영문, 숫자, 밑줄, 하이픈이어야 합니다"));

        }
        if config.discovery.roots.len() > 16
            || config.discovery.max_depth > 8
            || config.discovery.roots.iter().any(|root| !root.is_absolute())
        {

            return Err(ConfigError::Invalid("discovery는 절대 root 16개, depth 0..=8만 허용합니다"));

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

        if config.service.allowed_sid.as_ref().is_some_and(|sid| {

            sid.len() > 184
                || !sid.starts_with("S-1-")
                || sid.split('-').skip(2).any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))

        }) {

            return Err(ConfigError::Invalid("service.allowed_sid는 숫자 Windows SID여야 합니다"));

        }
        Ok(Self {

            codex: config.codex,
            device_id: config.device_id,
            discovery: config.discovery,
            execution: config.execution,
            screenshots: config.screenshots,
            projects: config.projects,
            #[cfg(windows)]
            service: config.service,

        })

    }

}

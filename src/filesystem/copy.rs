/*! 설치 파일의 준비·배치·정리를 소유해. */

use crate::{Bridge, Config, InstallError, config::paths::InstallOptions};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;
pub(crate) fn install_files(options: &InstallOptions, source: &Path) -> Result<(), InstallError> {

    let directory = &options.directory;
    let executable = directory.join("w7bridge.exe");
    let config = directory.join("w7bridge.toml");
    regular_file_or_missing(directory, true)?;
    let executable_exists = regular_file_or_missing(&executable, false)?;
    let config_exists = regular_file_or_missing(&config, false)?;
    if executable_exists && !options.update {

        return Err(InstallError::Exists);

    }
    if executable_exists
        && executable.canonicalize().map_err(|source| file_error("기존 실행 파일 확인", source))?
            == source.canonicalize().map_err(|source| file_error("설치 원본 확인", source))?
    {

        return Err(InstallError::SameExecutable);

    }
    if config_exists && options.config.is_some() {

        return Err(InstallError::ConfigExists);

    }
    let contents = if config_exists {

        fs::read_to_string(&config).map_err(|source| file_error("기존 설정 읽기", source))?

    } else if let Some(path) = &options.config {

        fs::read_to_string(path).map_err(|source| file_error("설치 설정 읽기", source))?

    } else {

        include_str!("../../w7bridge.example.toml").into()

    };
    // registry 검증만 해. 등록된 명령이나 MCP 연결은 시작하지 않아.
    Bridge::new(Config::parse(&contents)?, CancellationToken::new())?;

    fs::create_dir_all(directory).map_err(|source| file_error("설치 디렉터리 생성", source))?;
    let staging = Staging::new(directory.join(".w7bridge-install"))?;
    let staged_executable = staging.path.join("w7bridge.exe");
    let mut input = File::open(source).map_err(|source| file_error("설치 원본 읽기", source))?;
    let mut output = File::create(&staged_executable).map_err(|source| file_error("실행 파일 준비", source))?;
    io::copy(&mut input, &mut output).map_err(|source| file_error("실행 파일 복사", source))?;
    output.sync_all().map_err(|source| file_error("실행 파일 저장", source))?;
    drop(output);
    drop(input);
    if !config_exists {

        let staged_config = staging.path.join("w7bridge.toml");
        let mut file = File::create(&staged_config).map_err(|source| file_error("설정 준비", source))?;
        file.write_all(contents.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| file_error("설정 저장", source))?;
        drop(file);
        // 완성된 파일만 공개하고, 병렬 작업이 만든 기존 설정은 덮어쓰지 마.
        fs::hard_link(&staged_config, &config).map_err(|source| file_error("설정 배치", source))?;

    }
    if options.update {

        fs::rename(&staged_executable, &executable).map_err(|source| file_error("실행 파일 갱신", source))?;

    } else {

        fs::hard_link(&staged_executable, &executable).map_err(|source| file_error("실행 파일 배치", source))?;

    }
    Ok(())

}

fn regular_file_or_missing(path: &Path, directory: bool) -> Result<bool, InstallError> {

    match fs::symlink_metadata(path) {

        Ok(metadata) if !metadata.is_symlink() && if directory { metadata.is_dir() } else { metadata.is_file() } => {

            Ok(true)

        }
        Ok(_) => Err(InstallError::Argument("설치 대상은 symlink가 아닌 일반 파일과 디렉터리여야 합니다")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(file_error("설치 대상 확인", source)),

    }

}

struct Staging {

    path: PathBuf,

}

impl Staging {

    fn new(path: PathBuf) -> Result<Self, InstallError> {

        fs::create_dir(&path).map_err(|source| file_error("임시 디렉터리 생성 (기존 설치 작업 여부 확인)", source))?;
        Ok(Self { path })

    }

}

impl Drop for Staging {

    fn drop(&mut self) {

        // 이번 작업이 소유한 두 파일만 지워. 낯선 파일이나 이전 작업 흔적은 남겨 둬.
        for name in ["w7bridge.exe", "w7bridge.toml"] {

            if let Err(error) = fs::remove_file(self.path.join(name))
                && error.kind() != io::ErrorKind::NotFound
            {

                eprintln!("설치 임시 파일을 정리할 수 없습니다: {error}");

            }

        }
        if let Err(error) = fs::remove_dir(&self.path) {

            eprintln!("설치 임시 디렉터리를 정리할 수 없습니다: {error}");

        }

    }

}

pub(crate) fn file_error(operation: &'static str, source: io::Error) -> InstallError {

    InstallError::File { operation, source }

}

#[cfg(test)]
#[path = "../../tests/unit/install.rs"]
mod tests;

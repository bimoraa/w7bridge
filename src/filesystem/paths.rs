/*! 모든 파일 작업에 같은 상대 경로와 제외 규칙을 적용해. 경로 검사는 OS sandbox가 아니야. */

use super::transfer::FileStore;
use crate::{FileError, config::FileSettings};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
impl FileSettings {

    /** 상대 경로를 검증한다. 명시한 context 파일의 부모를 custom exclude로 막을 수 없다. */
    pub fn validate(&self) -> Result<(), FileError> {

        if self.exclude_dirs.len() > 64
            || self.context_files.len() > 64
            || !(1_048_576..=64 * 1024 * 1024).contains(&self.max_file_bytes)
        {

            return Err(FileError::Limit);

        }
        for path in self.exclude_dirs.iter().chain(&self.context_files) {

            relative(path)?;

        }
        for context in
            ["AGENTS.md", "MEMORY.md", "PLANS.md"].into_iter().chain(self.context_files.iter().map(String::as_str))
        {

            if excluded_builtin(context) || self.exclude_dirs.iter().any(|directory| under(context, directory)) {

                return Err(FileError::Path);

            }

        }
        Ok(())

    }

}

impl FileStore {

    /** 존재하는 root를 canonicalize한다. 비활성 정책이나 잘못된 경로는 거부한다. */
    pub fn new(root: &Path, settings: FileSettings) -> Result<Self, FileError> {

        settings.validate()?;
        if !settings.enabled {

            return Err(FileError::Disabled);

        }
        let root = root.canonicalize()?;
        if !root.is_dir() {

            return Err(FileError::Path);

        }
        Ok(Self { root, settings })

    }

    /** 적용 중인 제외/context 설정을 반환한다. 파일 시스템을 읽지 않는다. */
    pub fn settings(&self) -> &FileSettings {

        &self.settings

    }

    /** canonical root를 유지한 채 peer 정책만 교체한다. root가 달라졌거나 비활성 정책이면 실패한다. */
    pub fn with_settings(&self, settings: FileSettings) -> Result<Self, FileError> {

        self.check_root()?;
        settings.validate()?;
        if !settings.enabled {

            return Err(FileError::Disabled);

        }
        Ok(Self { root: self.root.clone(), settings })

    }

    /** 공유 가능한 경로인지 검사한다. 포함/제외 정책은 목록과 모든 파일 접근에서 동일하다. */
    pub fn permits(&self, path: &str) -> bool {

        relative(path).is_ok()
            && !excluded_builtin(path)
            && !self.settings.exclude_dirs.iter().any(|directory| under(path, directory))

    }

    pub(super) fn check_root(&self) -> Result<(), FileError> {

        if self.root.canonicalize()? != self.root || !self.root.is_dir() {

            return Err(FileError::Path);

        }
        Ok(())

    }

    pub(super) fn resolve(&self, path: &str, allow_missing: bool) -> Result<PathBuf, FileError> {

        self.check_root()?;
        if !self.permits(path) {

            return Err(FileError::Path);

        }
        let mut resolved = self.root.clone();
        for part in path.split('/') {

            resolved.push(part);
            match fs::symlink_metadata(&resolved) {

                Ok(metadata) if redirected(&metadata) => return Err(FileError::Path),
                Ok(_) => {}
                Err(error) if allow_missing && error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),

            }

        }
        Ok(resolved)

    }

    pub(super) fn create_parents(&self, parent: &Path) -> Result<(), FileError> {

        let mut current = self.root.clone();
        for part in parent.strip_prefix(&self.root).map_err(|_| FileError::Path)?.components() {

            current.push(part);
            match fs::symlink_metadata(&current) {

                Ok(metadata) if redirected(&metadata) || !metadata.is_dir() => return Err(FileError::Path),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
                Err(error) => return Err(error.into()),

            }

        }
        Ok(())

    }

}

/** Windows junction을 포함한 모든 reparse point를 거부한다. */
pub(crate) fn redirected( metadata: &fs::Metadata, ) -> bool {

    #[cfg(windows)]
    {

        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0

    }
    #[cfg(not(windows))]
    {

        metadata.is_symlink()

    }

}

/** 양쪽 manifest의 경로를 검증한다. 대소문자와 file/directory 충돌은 전송 전에 거부한다. */
pub fn validate_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<(), FileError> {

    let mut folded = BTreeMap::new();
    let mut files = BTreeSet::new();
    for path in paths {

        relative(path)?;
        files.insert(path.to_owned());
        let mut prefix = String::new();
        for part in path.split('/') {

            if !prefix.is_empty() {

                prefix.push('/');

            }
            prefix.push_str(part);
            if folded.insert(prefix.to_lowercase(), prefix.clone()).is_some_and(|previous| previous != prefix) {

                return Err(FileError::Path);

            }

        }

    }
    for path in &files {

        let mut parent = Path::new(path).parent();
        while let Some(directory) = parent {

            if directory.to_str().is_some_and(|directory| files.contains(directory)) {

                return Err(FileError::Path);

            }
            parent = directory.parent();

        }

    }
    Ok(())

}

fn relative(path: &str) -> Result<(), FileError> {

    if path.is_empty() || path.len() > 1024 {

        return Err(FileError::Path);

    }
    for part in path.split('/') {

        let stem = part.split('.').next().unwrap_or_default().to_ascii_lowercase();
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with(['.', ' '])
            || part.chars().any(|character| character.is_control() || "\\:*?<>|\"".contains(character))
            || matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
            || (stem.len() == 4
                && (stem.starts_with("com") || stem.starts_with("lpt"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {

            return Err(FileError::Path);

        }

    }
    Ok(())

}

fn under(path: &str, directory: &str) -> bool {

    let path = path.to_lowercase();
    let directory = directory.to_lowercase();
    path == directory || path.starts_with(&(directory + "/"))

}

pub(crate) fn excluded_builtin(path: &str) -> bool {

    path.split('/').any(|part| {

        matches!(
            part.to_ascii_lowercase().as_str(),
            ".git"
                | "target"
                | "node_modules"
                | "dist"
                | "build"
                | "out"
                | "coverage"
                | ".cache"
                | ".next"
                | ".nuxt"
                | ".svelte-kit"
                | ".turbo"
                | "__pycache__"
                | ".pytest_cache"
                | ".mypy_cache"
                | ".ruff_cache"
                | ".codebase-memory"
                | ".w7bridge"
                | ".w7bridge-update"
                | ".ds_store"
        )

    }) || path.rsplit('/').next().is_some_and(|name| {

        let name = name.to_ascii_lowercase();
        name == ".env"
            || name.starts_with(".env.") && !matches!(name.as_str(), ".env.example" | ".env.sample")
            || ["pyc", "pyo", "o", "obj", "a", "lib", "so", "dylib", "dll", "exe", "pdb", "class", "tsbuildinfo"]
                .iter()
                .any(|extension| name.ends_with(&format!(".{extension}")))

    })

}

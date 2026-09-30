/*! 공유 파일 목록·읽기·조건부 쓰기를 처리해. */

use super::{
    metadata::{FileEntry, digest},
    paths::{redirected, validate_paths},
};
use crate::{FileError, config::FileSettings};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
/** root identity와 파일 공유 정책을 고정한다. 경로 검사는 OS sandbox가 아니며 로컬 파일 소유자를 신뢰한다. */
#[derive(Clone)]
pub struct FileStore {

    pub(super) root: PathBuf,
    pub(super) settings: FileSettings,

}

#[derive(Default)]
struct Scan {

    entries: Vec<FileEntry>,
    visited: usize,
    total_bytes: usize,

}

impl FileStore {

    /** 공유 파일을 정렬된 목록으로 읽는다. 10000개, 합계 256 MiB를 넘으면 부분 목록 대신 실패한다. */
    pub fn list(&self) -> Result<Vec<FileEntry>, FileError> {

        let _lock = self.lock("access.lock")?;
        let mut scan = Scan::default();
        self.walk(&self.root, &mut scan, 0)?;
        let mut entries = scan.entries;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        validate_paths(entries.iter().map(|entry| entry.path.as_str()))?;
        Ok(entries)

    }

    fn walk(&self, directory: &Path, scan: &mut Scan, depth: usize) -> Result<(), FileError> {

        if depth > 64 {

            return Err(FileError::Limit);

        }
        for entry in fs::read_dir(directory)? {

            let entry = entry?;
            scan.visited += 1;
            if scan.visited > 20000 {

                return Err(FileError::Limit);

            }
            let path = entry.path();
            let relative = path
                .strip_prefix(&self.root)
                .map_err(|_| FileError::Path)?
                .to_str()
                .ok_or(FileError::Path)?
                .replace('\\', "/");
            if !self.permits(&relative) {

                continue;

            }
            let metadata = fs::symlink_metadata(&path)?;
            if redirected(&metadata) {

                return Err(FileError::Path);

            }
            if metadata.is_dir() {

                self.walk(&path, scan, depth + 1)?;

            } else if metadata.is_file() {

                let content = self.read_unlocked(&relative)?;
                scan.total_bytes += content.len();
                scan.entries.push(FileEntry { path: relative, sha256: digest(&content), bytes: content.len() });
                if scan.entries.len() > 10000 || scan.total_bytes > 256 * 1024 * 1024 {

                    return Err(FileError::Limit);

                }

            } else {

                return Err(FileError::Path);

            }

        }
        Ok(())

    }

    /** 설정된 파일 한도까지 읽는다. symlink, special file과 제외 경로는 거부한다. */
    pub fn read(&self, path: &str) -> Result<Vec<u8>, FileError> {

        let _lock = self.lock("access.lock")?;
        self.read_unlocked(path)

    }

    pub(super) fn read_unlocked(&self, path: &str) -> Result<Vec<u8>, FileError> {

        let resolved = self.resolve(path, false)?;
        if !fs::symlink_metadata(&resolved)?.is_file() {

            return Err(FileError::Path);

        }
        let file = File::open(resolved)?;
        if file.metadata()?.len() > self.settings.max_file_bytes as u64 {

            return Err(FileError::Limit);

        }
        let mut content = Vec::new();
        file.take(self.settings.max_file_bytes as u64 + 1).read_to_end(&mut content)?;
        if content.len() > self.settings.max_file_bytes {

            return Err(FileError::Limit);

        }
        Ok(content)

    }

    /** 기존 SHA-256이 일치할 때만 atomic replace한다. expected가 None이면 신규 파일만 만든다.
    content가 None이면 조건부 삭제다. 응답이 유실되어도 자동 재시도나 rollback을 하지 않는다.
    파일 단위 원자성을 제공하며 프로젝트 전체 transaction은 아니다. 로컬 editor는 이 lock을 따르지 않을 수 있다. */
    pub fn write(
        &self,
        path: &str,
        content: Option<&[u8]>,
        expected: Option<&str>,
    ) -> Result<Option<String>, FileError> {

        if !self.permits(path) {

            return Err(FileError::Path);

        }
        let _lock = self.lock("access.lock")?;
        if content.is_some_and(|bytes| bytes.len() > self.settings.max_file_bytes) {

            return Err(FileError::Limit);

        }
        let resolved = self.resolve(path, true)?;
        let previous = match self.read_unlocked(path) {

            Ok(bytes) => Some(bytes),
            Err(FileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),

        };
        let current = previous.as_deref().map(digest);
        if current.as_deref() != expected {

            return Err(FileError::Conflict);

        }
        if previous.as_deref() == content {

            return Ok(current);

        }
        let Some(content) = content else {

            if current.is_some() {

                let revision = self.prepare_revision(path, previous.as_deref(), None)?;
                let latest = self.read_unlocked(path)?;
                if Some(digest(&latest)).as_deref() != expected {

                    return Err(FileError::Conflict);

                }
                fs::remove_file(resolved)?;
                self.commit_revision(revision)?;

            }
            return Ok(None);

        };
        let parent = resolved.parent().ok_or(FileError::Path)?;
        self.create_parents(parent)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        temporary.write_all(content)?;
        if current.is_some() {

            temporary.as_file().set_permissions(fs::metadata(&resolved)?.permissions())?;

        }
        temporary.as_file().sync_all()?;
        // 외부 editor가 lock을 무시할 수 있으니 commit 직전에도 버전을 확인해.
        let last = match self.read_unlocked(path) {

            Ok(bytes) => Some(digest(&bytes)),
            Err(FileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),

        };
        if last.as_deref() != expected {

            return Err(FileError::Conflict);

        }
        let revision = self.prepare_revision(path, previous.as_deref(), Some(content))?;
        let latest = match self.read_unlocked(path) {

            Ok(bytes) => Some(digest(&bytes)),
            Err(FileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),

        };
        if latest.as_deref() != expected {

            return Err(FileError::Conflict);

        }
        self.resolve(path, true)?;
        if current.is_none() {

            temporary.persist_noclobber(resolved).map_err(|error| {

                if error.error.kind() == std::io::ErrorKind::AlreadyExists {

                    FileError::Conflict

                } else {

                    FileError::Io(error.error)

                }

            })?;

        } else {

            temporary.persist(resolved).map_err(|error| FileError::Io(error.error))?;

        }
        self.commit_revision(revision)?;
        Ok(Some(digest(content)))

    }

}

#[cfg(test)]
#[path = "../../tests/unit/files.rs"]
mod tests;

/*! 파일 manifest, 내부 metadata와 process 간 lock을 소유해. */

use super::paths::redirected;
use super::transfer::FileStore;
use crate::FileError;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};
use tempfile::NamedTempFile;
/** 목록에 공개하는 상대 경로와 파일 내용의 SHA-256이다. root나 절대 경로는 포함하지 않는다. */
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileEntry {

    pub path: String,
    pub sha256: String,
    pub bytes: usize,

}

impl FileStore {

    /** root별 persistent ID다. source나 권한을 담지 않으며 metadata가 없는 새 root에는 다른 ID를 만든다. */
    pub fn root_key(&self) -> Result<String, FileError> {

        let reading = self.read_lock()?;
        if let Some(key) = self.stored_root_key()? {

            return Ok(key);

        }
        drop(reading);
        let _lock = self.lock("access.lock")?;
        if let Some(key) = self.stored_root_key()? {

            return Ok(key);

        }
        let mut temporary = tempfile::Builder::new().rand_bytes(32).tempfile_in(self.metadata_dir()?)?;
        let key = digest(temporary.path().file_name().ok_or(FileError::Path)?.as_encoded_bytes());
        temporary.write_all(key.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(self.metadata_path("root-key")?).map_err(|error| FileError::Io(error.error))?;
        Ok(key)

    }

    fn stored_root_key( &self, ) -> Result<Option<String>,FileError> {

        if let Some(bytes) = self.load_metadata("root-key")? {

            let key = String::from_utf8(bytes).map_err(|_| FileError::Data)?;
            if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {

                return Err(FileError::Data);

            }
            return Ok(Some(key));

        }
        Ok(None)

    }

    /** 제외된 내부 metadata 폴더를 검증하고 만든다. symlink나 변경된 root는 거부한다. */
    pub fn metadata_dir(&self) -> Result<PathBuf, FileError> {

        self.check_root()?;
        let directory = self.root.join(".w7bridge");
        match fs::symlink_metadata(&directory) {

            Ok(metadata) if redirected(&metadata) || !metadata.is_dir() => return Err(FileError::Path),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&directory)?,
            Err(error) => return Err(error.into()),

        }
        Ok(directory)

    }

    /** 같은 metadata 이름의 작업을 직렬화한다. spawn 직후의 짧은 경합은 최대 100 ms만 기다린다. */
    pub fn lock(&self, name: &str) -> Result<File, FileError> {

        self.acquire_lock(name, false)

    }

    /** 읽기는 서로 함께 실행하고 파일 교체는 exclusive lock으로 막아. */
    pub(super) fn read_lock( &self, ) -> Result<File,FileError> {

        self.acquire_lock("access.lock", true)

    }

    fn acquire_lock( &self, name: &str, shared: bool, ) -> Result<File,FileError> {

        let pair_lock = name.strip_prefix("sync-").and_then(|name| name.strip_suffix(".lock")).is_some_and(|hash| {

            hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))

        });
        if !matches!(name, "access.lock" | "sync.lock" | "transfer.lock" | "git.lock") && !pair_lock {

            return Err(FileError::Path);

        }
        let path = self.metadata_dir()?.join(name);
        if fs::symlink_metadata(&path).is_ok_and(|metadata| redirected(&metadata) || !metadata.is_file()) {

            return Err(FileError::Path);

        }
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
        let started = std::time::Instant::now();
        loop {

            let attempt = if shared { FileExt::try_lock_shared(&file) } else { file.try_lock_exclusive() };
            match attempt {

                Ok(()) => break,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
                {

                    if started.elapsed() >= std::time::Duration::from_millis(100) {

                        return Err(FileError::Busy);

                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));

                }
                Err(error) => return Err(error.into()),

            }

        }
        Ok(file)

    }

    /** sync metadata를 읽는다. 고정된 단일 이름만 허용하고 symlink를 거부한다. */
    pub fn load_metadata(&self, name: &str) -> Result<Option<Vec<u8>>, FileError> {

        let path = self.metadata_path(name)?;
        match fs::symlink_metadata(&path) {

            Ok(metadata) if redirected(&metadata) || !metadata.is_file() => Err(FileError::Path),
            Ok(metadata) if metadata.len() > 64 * 1024 * 1024 => Err(FileError::Limit),
            Ok(_) => {

                let mut bytes = Vec::new();
                File::open(path)?.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > 64 * 1024 * 1024 {

                    return Err(FileError::Limit);

                }
                Ok(Some(bytes))

            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),

        }

    }

    /** 완성된 metadata를 원자적으로 배치한다. source 전송 목록에는 포함하지 않는다. */
    pub fn save_metadata(&self, name: &str, content: &[u8]) -> Result<(), FileError> {

        let path = self.metadata_path(name)?;
        if content.len() > 64 * 1024 * 1024 {

            return Err(FileError::Limit);

        }
        if fs::symlink_metadata(&path).is_ok_and(|metadata| redirected(&metadata) || !metadata.is_file()) {

            return Err(FileError::Path);

        }
        let mut temporary = NamedTempFile::new_in(self.metadata_dir()?)?;
        temporary.write_all(content)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| FileError::Io(error.error))?;
        Ok(())

    }

    fn metadata_path(&self, name: &str) -> Result<PathBuf, FileError> {

        if name.is_empty()
            || name.len() > 160
            || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            || matches!(name, "." | ".." | "access.lock" | "sync.lock" | "transfer.lock" | "git.lock")
            || name.ends_with(".lock")
        {

            return Err(FileError::Path);

        }
        Ok(self.metadata_dir()?.join(name))

    }

}
/** 파일 내용의 SHA-256을 lowercase hex로 반환한다. sync 비교용이며 AI memory 데이터가 아니다. */
pub fn digest(content: &[u8]) -> String {

    format!("{:x}", Sha256::digest(content))

}

/** 목록을 상대 경로별 hash map으로 변환한다. */
pub fn hashes(entries: &[FileEntry]) -> BTreeMap<String, String> {

    entries.iter().map(|entry| (entry.path.clone(), entry.sha256.clone())).collect()

}

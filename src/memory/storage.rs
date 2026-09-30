/*! 공유 파일의 lock·한도·조건부 쓰기를 재사용해. 저장한 context는 기존 sync 대상이야. */

use super::{context::ContextDocument, state::MemoryState};
use crate::{FileError, filesystem::FileStore};
use std::io::ErrorKind;

pub(super) fn read( store: &FileStore, path: &str, ) -> Result<ContextDocument, FileError> {

    let bytes = match store.read(path) {

        Ok(bytes) => Some(bytes),
        Err(FileError::Io(error)) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(error),

    };
    ContextDocument::decode(path, bytes)

}

pub(super) fn update( store: &FileStore, path: &str, content: &str, expected: Option<&str>, ) -> Result<MemoryState, FileError> {

    let sha256 = store.write(path, Some(content.as_bytes()), expected)?;
    Ok(MemoryState { exists: true, sha256, bytes: content.len() })

}

/*! context 파일의 UTF-8 내용과 관찰한 revision을 함께 반환해. */

use super::state::MemoryState;
use crate::FileError;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct ContextDocument {

    pub path: String,
    pub content: Option<String>,
    #[serde(flatten)]
    pub state: MemoryState,

}

impl ContextDocument {

    pub(super) fn decode( path: &str, bytes: Option<Vec<u8>>, ) -> Result<Self, FileError> {

        if bytes.as_ref().is_some_and(|bytes| bytes.len() > 1024 * 1024) {

            return Err(FileError::Limit);

        }
        let state = MemoryState::observed(bytes.as_deref());
        let content = bytes.map(String::from_utf8).transpose().map_err(|_| FileError::Data)?;
        Ok(Self { path: path.into(), content, state })

    }

}

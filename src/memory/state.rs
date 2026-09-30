/*! 읽거나 쓴 파일의 revision snapshot을 계산해. 별도 state 파일이나 cache는 만들지 않아. */

use crate::filesystem::digest;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct MemoryState {

    pub exists: bool,
    pub sha256: Option<String>,
    pub bytes: usize,

}

impl MemoryState {

    pub(super) fn observed( content: Option<&[u8]>, ) -> Self {

        Self {

            exists: content.is_some(),
            sha256: content.map(digest),
            bytes: content.map_or(0, <[u8]>::len),

        }

    }

}

/*! client가 읽은 버전이 일치할 때만 context 전체를 갱신해. 자동 retry나 merge는 하지 않아. */

use crate::{FileError, memory::ProjectMemory};
use serde_json::Value;

pub(super) fn execute( memory: &ProjectMemory, path: &str, content: &str, expected: Option<&str>, ) -> Result<Value, FileError> {

    serde_json::to_value(memory.update(path, content, expected)?).map_err(|_| FileError::Data)

}

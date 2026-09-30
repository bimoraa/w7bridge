/*! context 문서, 현재 hash와 사용 가능한 context 경로를 MCP에 반환해. */

use crate::{FileError, memory::ProjectMemory};
use serde_json::Value;

pub(super) fn execute( memory: &ProjectMemory, path: &str, ) -> Result<Value, FileError> {

    serde_json::to_value(memory.read(path)?).map_err(|_| FileError::Data)

}

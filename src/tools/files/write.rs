/*! 기존 hash가 일치하는 파일만 쓰거나 지워. */

use crate::{FileError, filesystem::FileStore};
use serde_json::{Value, json};
pub(super) fn execute( store: &FileStore, path: &str, data: Option<&[u8]>, expected: Option<&str>, ) -> Result<Value, FileError> {

    Ok(json!({"sha256":store.write(path,data,expected)?}))

}

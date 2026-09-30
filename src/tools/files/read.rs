/*! 파일 내용과 버전을 함께 반환해. */

use crate::{
    FileError,
    filesystem::{FileStore, digest},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
pub(super) fn execute(store: &FileStore, path: &str) -> Result<Value, FileError> {

    let bytes = store.read(path)?;
    if bytes.len() > 1_048_576 {

        return Err(FileError::Limit);

    }
    Ok(json!({"content_base64":STANDARD.encode(&bytes),"sha256":digest(&bytes)}))

}

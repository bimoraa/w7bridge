/*! 공유 파일 목록과 적용 중인 정책을 반환해. */

use crate::{FileError, filesystem::FileStore};
use serde_json::{Value, json};
pub(super) fn execute( store: &FileStore, version: u32, ) -> Result<Value, FileError> {

    let files = store.list()?;
    if version == 1 && files.iter().any(|entry| entry.bytes > 1_048_576) {

        return Err(FileError::Limit);

    }
    let mut settings = json!(store.settings());
    if version == 1
        && let Some(settings) = settings.as_object_mut()
    {

        settings.remove("max_file_bytes");

    }
    Ok(json!({ "files": files, "settings": settings, "root_key": store.root_key()? }))

}

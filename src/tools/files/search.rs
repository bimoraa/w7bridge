/*! 공유 파일 목록과 적용 중인 정책을 반환해. */

use crate::{FileError, filesystem::FileStore};
use serde_json::{Value, json};
pub(super) fn execute(store: &FileStore) -> Result<Value, FileError> {

    Ok(json!({ "files": store.list()?, "settings": store.settings(), "root_key": store.root_key()? }))

}

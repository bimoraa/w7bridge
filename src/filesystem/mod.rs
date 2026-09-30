/*! 공유 파일 작업과 설치 파일 작업을 구분하는 owner야. */

pub(crate) mod copy;
mod metadata;
mod paths;
mod transfer;
pub(crate) mod watcher;
pub use crate::config::FileSettings;
pub use metadata::{FileEntry, digest, hashes};
pub use paths::validate_paths;
pub use transfer::FileStore;

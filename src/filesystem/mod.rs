/*! 공유 파일 작업과 설치 파일 작업을 구분하는 owner야. */

pub(crate) mod chunks;
pub(crate) mod copy;
mod history;
mod metadata;
pub(crate) mod paths;
mod snapshot;
mod transfer;
pub(crate) mod watcher;
pub use crate::config::FileSettings;
pub use metadata::{FileEntry, digest, hashes};
pub use paths::validate_paths;
pub use transfer::FileStore;

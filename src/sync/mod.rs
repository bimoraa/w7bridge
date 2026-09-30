/*! 양방향 project sync의 단일 library owner야. */

use crate::filesystem::FileEntry;
use std::future::Future;
mod conflict;
mod engine;
mod manifest;
mod planner;
pub(crate) mod state;
pub use crate::error::filesystem::SyncError;
pub use engine::Session;
pub(crate) use state::coordination::{Checkpoint, Coordinator};
pub use state::{Conflict, Report, Status};
/** 전송 대상은 상대 경로와 조건부 버전만 받는다. registry 설정은 원격에서 변경하지 않는다. */
pub trait Peer {

    fn list(&self) -> impl Future<Output = Result<Vec<FileEntry>, SyncError>> + Send;
    fn read(&self, path: &str) -> impl Future<Output = Result<Vec<u8>, SyncError>> + Send;
    fn read_reusing( &self, path: &str, _local: &crate::filesystem::FileStore, ) -> impl Future<Output=Result<Vec<u8>,SyncError>> + Send {

        self.read(path)

    }
    fn write(
        &self,
        path: &str,
        content: Option<&[u8]>,
        expected: Option<&str>,
    ) -> impl Future<Output = Result<(), SyncError>> + Send;

}

#[cfg(test)]
#[path = "../../tests/unit/sync.rs"]
mod tests;

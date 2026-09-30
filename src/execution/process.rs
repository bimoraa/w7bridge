use std::time::Duration;

use process_wrap::tokio::ChildWrapper;
use tokio::time::timeout;

use super::failure;
use crate::error::ExecutionError;

pub(super) struct ChildGuard {

    pub child: Box<dyn ChildWrapper>,
    pub active: bool,

}

impl Drop for ChildGuard {

    fn drop(&mut self) {

        if self.active {

            // 취소된 future에서도 트리를 종료해. 정상 경로의 오류는 덮어쓰지 않아.
            let _ = self.child.start_kill();

        }

    }

}

pub(super) async fn stop(child: &mut dyn ChildWrapper) -> Result<(), ExecutionError> {

    child.start_kill().map_err(|source| failure("트리 종료", source))?;
    timeout(Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| ExecutionError::CleanupTimeout)?
        .map_err(|source| failure("종료 확인", source))?;
    Ok(())

}

use crate::protocol::response::{Output, Status};
use std::{io, path::Path, sync::Arc, time::Duration};

use process_wrap::tokio::CommandWrap;
use tokio::{
    sync::{Semaphore, oneshot},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

use super::{
    command::Capture,
    failure,
    process::{ChildGuard, stop},
};
use crate::{
    config::{CommandDefinition, ExecutionLimits},
    error::ExecutionError,
};

pub(crate) struct Executor {

    limits: ExecutionLimits,
    slots: Arc<Semaphore>,
    shutdown: CancellationToken,

}

impl Executor {

    pub fn new(limits: ExecutionLimits, shutdown: CancellationToken) -> Self {

        let slots = Arc::new(Semaphore::new(limits.concurrency));
        Self { limits, slots, shutdown }

    }

    pub fn available_slots( &self, ) -> usize {

        self.slots.available_permits()

    }

    pub async fn run_observed(
        &self,
        root: &Path,
        definition: &CommandDefinition,
        cancellation: CancellationToken,
        live: Option<super::process::Live>,
        started: Option<oneshot::Sender<Result<(), String>>>,
    ) -> Result<Output, ExecutionError> {

        if self.shutdown.is_cancelled() || cancellation.is_cancelled() {

            return Err(ExecutionError::Cancelled);

        }

        let _slot = self.slots.clone().try_acquire_owned().map_err(|_| ExecutionError::Busy)?;
        let mut command = CommandWrap::with_new(&definition.executable, |command| {

            super::sandbox::configure(command, root, definition);

        });

        crate::platform::configure(&mut command);

        let child = command.spawn().map_err(|source| failure("생성", source))?;
        let mut guard = ChildGuard { child, active: true };
        let stdout =
            guard.child.stdout().take().ok_or_else(|| failure("stdout 연결", io::Error::other("연결 없음")))?;
        let stderr =
            guard.child.stderr().take().ok_or_else(|| failure("stderr 연결", io::Error::other("연결 없음")))?;
        if let Some(started) = started {

            let _ = started.send(Ok(()));

        }
        let mut out = Capture::default();
        let mut err = Capture::default();

        let outcome = {

            let completion = async {

                tokio::try_join!(
                    guard.child.wait(),
                    out.read_observed(stdout, self.limits.output_bytes, live.clone(), "stdout"),
                    err.read_observed(stderr, self.limits.output_bytes, live.clone(), "stderr")
                )
                .map(|(status, (), ())| status)

            };

            tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => Ok((Status::Cancelled, None)),
                _ = cancellation.cancelled() => Ok((Status::Cancelled, None)),
                result = async {
                    if live.is_some() && definition.background { Ok(completion.await) }
                    else { timeout(Duration::from_secs(self.limits.timeout_seconds), completion).await }
                } => {
                    match result {
                        Ok(Ok(exit)) => Ok((Status::Completed, Some(exit))),
                        Ok(Err(source)) => Err(failure("대기 또는 출력 읽기", source)),
                        Err(_) => Ok((Status::TimedOut, None)),
                    }
                }
            }

        };

        if !matches!(&outcome, Ok((Status::Completed, _))) {

            let cleanup = stop(&mut *guard.child).await;

            if let Err(primary) = outcome {

                if let Err(secondary) = cleanup {

                    eprintln!("프로세스 정리 중 추가 오류: {secondary}");

                }

                return Err(primary);

            }

            cleanup?;

        }

        guard.active = false;
        let (status, exit) = outcome?;

        Ok(Output {

            status,
            success: exit.is_some_and(|exit| exit.success()),
            exit_code: exit.and_then(|exit| exit.code()),
            stdout: String::from_utf8_lossy(&out.bytes).into_owned(),
            stderr: String::from_utf8_lossy(&err.bytes).into_owned(),
            stdout_truncated: out.truncated,
            stderr_truncated: err.truncated,

        })

    }

}

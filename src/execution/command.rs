use std::{io, path::Path, process::Stdio, time::Duration};

use process_wrap::tokio::CommandWrap;
use tokio::{sync::Semaphore, time::timeout};
use tokio_util::sync::CancellationToken;

use super::{
    failure,
    output::{Capture, Output, Status},
    process::{ChildGuard, stop},
};
use crate::{
    config::{CommandDefinition, ExecutionLimits},
    error::ExecutionError,
};

pub(crate) struct Executor {

    limits: ExecutionLimits,
    slots: Semaphore,
    shutdown: CancellationToken,

}

impl Executor {

    pub fn new(limits: ExecutionLimits, shutdown: CancellationToken) -> Self {

        let slots = Semaphore::new(limits.concurrency);
        Self { limits, slots, shutdown }

    }

    pub async fn run(
        &self,
        root: &Path,
        definition: &CommandDefinition,
        cancellation: CancellationToken,
    ) -> Result<Output, ExecutionError> {

        if self.shutdown.is_cancelled() || cancellation.is_cancelled() {

            return Err(ExecutionError::Cancelled);

        }

        let _slot = self.slots.try_acquire().map_err(|_| ExecutionError::Busy)?;
        let mut command = CommandWrap::with_new(&definition.executable, |command| {

            command.args(&definition.args).current_dir(root).env_clear();

            // 인증 토큰을 자동 상속하지 않고 OS와 toolchain에 필요한 항목만 넘겨.
            for key in [
                "PATH",
                "SystemRoot",
                "WINDIR",
                "TEMP",
                "TMP",
                "TMPDIR",
                "USERPROFILE",
                "HOME",
                "CARGO_HOME",
                "RUSTUP_HOME",
            ] {

                if let Some(value) = std::env::var_os(key) {

                    command.env(key, value);

                }

            }

            command.envs(&definition.env).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

        });

        crate::platform::configure(&mut command);

        let child = command.spawn().map_err(|source| failure("생성", source))?;
        let mut guard = ChildGuard { child, active: true };
        let stdout =
            guard.child.stdout().take().ok_or_else(|| failure("stdout 연결", io::Error::other("연결 없음")))?;
        let stderr =
            guard.child.stderr().take().ok_or_else(|| failure("stderr 연결", io::Error::other("연결 없음")))?;
        let mut out = Capture::default();
        let mut err = Capture::default();

        let outcome = {

            let completion = async {

                tokio::try_join!(
                    guard.child.wait(),
                    out.read(stdout, self.limits.output_bytes),
                    err.read(stderr, self.limits.output_bytes)
                )
                .map(|(status, (), ())| status)

            };

            tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => Ok((Status::Cancelled, None)),
                _ = cancellation.cancelled() => Ok((Status::Cancelled, None)),
                result = timeout(Duration::from_secs(self.limits.timeout_seconds), completion) => {
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

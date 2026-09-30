use std::io;

use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {

    Completed,
    TimedOut,
    Cancelled,

}

#[derive(Debug, Serialize)]
pub(crate) struct Output {

    pub status: Status,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,

}

#[derive(Default)]
pub(super) struct Capture {

    pub bytes: Vec<u8>,
    pub truncated: bool,

}

impl Capture {

    pub async fn read(&mut self, mut pipe: impl AsyncRead + Unpin, limit: usize) -> io::Result<()> {

        let mut buffer = [0_u8; 8192];

        loop {

            let count = pipe.read(&mut buffer).await?;

            if count == 0 {

                return Ok(());

            }

            let keep = count.min(limit.saturating_sub(self.bytes.len()));
            self.bytes.extend_from_slice(&buffer[..keep]);
            self.truncated |= keep < count;

        }

    }

}

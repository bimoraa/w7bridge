/*! stdout/stderr를 drain하면서 bounded output을 모아. */

use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};
#[derive(Default)]
pub(super) struct Capture {

    pub bytes: Vec<u8>,
    pub truncated: bool,

}

impl Capture {

    pub async fn read_observed(
        &mut self,
        mut pipe: impl AsyncRead + Unpin,
        limit: usize,
        live: Option<super::process::Live>,
        stream: &'static str,
    ) -> io::Result<()> {

        let mut buffer = [0_u8; 8192];

        loop {

            let count = pipe.read(&mut buffer).await?;

            if count == 0 {

                return Ok(());

            }

            if let Some(live) = &live {

                live.append(stream, &buffer[..count]);

            }
            let keep = count.min(limit.saturating_sub(self.bytes.len()));
            self.bytes.extend_from_slice(&buffer[..keep]);
            self.truncated |= keep < count;

        }

    }

}

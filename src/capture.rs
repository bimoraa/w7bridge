/*! 명시한 MCP 요청에만 screenshot을 생성해. 일회성 임시 파일은 공유 project 밖에 두고 응답 뒤 삭제해. */

use crate::error::CaptureError;
use process_wrap::tokio::CommandWrap;
use std::{fs::File, io::Read, path::Path, time::Duration};
use tokio::{sync::Semaphore, time::timeout};
use tokio_util::sync::CancellationToken;

pub(crate) struct Capture {

    enabled: bool,
    slot: Semaphore,
    shutdown: CancellationToken,

}

pub(crate) struct Image {

    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,

}

impl Capture {

    pub fn new(enabled: bool, shutdown: CancellationToken) -> Self {

        Self { enabled, slot: Semaphore::new(1), shutdown }

    }

    pub async fn take(&self, display: u32, request: CancellationToken) -> Result<Image, CaptureError> {

        if !self.enabled {

            return Err(CaptureError::Disabled);

        }
        if !(1..=16).contains(&display) {

            return Err(CaptureError::Data);

        }
        if self.shutdown.is_cancelled() || request.is_cancelled() {

            return Err(CaptureError::Cancelled);

        }
        let _permit = self.slot.try_acquire().map_err(|_| CaptureError::Busy)?;
        let temporary = tempfile::Builder::new()
            .prefix("w7bridge-capture-")
            .rand_bytes(32)
            .tempdir()
            .map_err(|_| CaptureError::Unavailable)?;
        let path = temporary.path().join("capture.png");
        let cancellation = CancellationToken::new();
        {

            let capturing = crate::platform::capture(&path, display, cancellation.clone());
            tokio::pin!(capturing);
            tokio::select! {
                result = &mut capturing => result?,
                _ = request.cancelled() => {
                    cancellation.cancel();
                    capturing.await?;
                    return Err(CaptureError::Cancelled);
                }
                _ = self.shutdown.cancelled() => {
                    cancellation.cancel();
                    capturing.await?;
                    return Err(CaptureError::Cancelled);
                }
            }

        }
        tokio::task::spawn_blocking(move || read_image(&path)).await.map_err(|_| CaptureError::Unavailable)?

    }

}

pub(crate) async fn run_command(mut command: CommandWrap, cancellation: CancellationToken) -> Result<(), CaptureError> {

    if cancellation.is_cancelled() {

        return Err(CaptureError::Cancelled);

    }
    crate::platform::configure(&mut command);
    let mut child = command.spawn().map_err(|_| CaptureError::Unavailable)?;
    let (result, exited) = tokio::select! {
        biased;
        _ = cancellation.cancelled() => (Err(CaptureError::Cancelled), false),
        result = timeout(Duration::from_secs(25), child.wait()) => match result {
            Ok(Ok(exit)) => (if exit.success() { Ok(()) } else { Err(CaptureError::Unavailable) }, true),
            Ok(Err(_)) => (Err(CaptureError::Unavailable), false),
            Err(_) => (Err(CaptureError::Timeout), false),
        },
    };
    if !exited {

        let killed = child.start_kill();
        let waited = timeout(Duration::from_secs(5), child.wait()).await;
        if killed.is_err() || !matches!(waited, Ok(Ok(_))) {

            return Err(CaptureError::Cleanup { cause: result.err().map(Box::new) });

        }

    }
    result

}

fn read_image(path: &Path) -> Result<Image, CaptureError> {

    let metadata = std::fs::symlink_metadata(path).map_err(|_| CaptureError::Unavailable)?;
    if !metadata.is_file() || metadata.is_symlink() {

        return Err(CaptureError::Data);

    }
    if metadata.len() > 8 * 1024 * 1024 {

        return Err(CaptureError::Limit);

    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| CaptureError::Unavailable)?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CaptureError::Unavailable)?;
    if bytes.len() > 8 * 1024 * 1024 {

        return Err(CaptureError::Limit);

    }
    if bytes.len() < 45
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || bytes[8..12] != 13_u32.to_be_bytes()
        || &bytes[12..16] != b"IHDR"
        || &bytes[bytes.len() - 8..bytes.len() - 4] != b"IEND"
    {

        return Err(CaptureError::Data);

    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().map_err(|_| CaptureError::Data)?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().map_err(|_| CaptureError::Data)?);
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 20_000_000 {

        return Err(CaptureError::Limit);

    }
    Ok(Image { bytes, width, height })

}

#[cfg(test)]
#[path = "../tests/unit/capture.rs"]
mod tests;

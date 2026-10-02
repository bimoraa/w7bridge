/*! OS별 process 트리 생성 정책을 선택해. */

use process_wrap::tokio::{CommandWrap, KillOnDrop};
#[cfg(unix)]
pub(crate) mod macos;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::identity::desktop_owner;

pub(crate) fn configure_background( command: &mut tokio::process::Command, ) {

    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    #[cfg(not(windows))]
    let _ = command;

}

pub(crate) fn configure(command: &mut CommandWrap) {

    command.wrap(KillOnDrop);
    #[cfg(windows)]
    windows::process::configure(command);
    #[cfg(unix)]
    macos::process::configure(command);

}

pub(crate) async fn capture(
    path: &std::path::Path,
    display: u32,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<(), crate::error::CaptureError> {

    #[cfg(target_os = "macos")]
    return macos::filesystem::capture(path, display, cancellation).await;
    #[cfg(windows)]
    return windows::filesystem::capture(path, display, cancellation).await;
    #[cfg(not(any(target_os = "macos", windows)))]
    {

        let _ = (path, display, cancellation);
        Err(crate::error::CaptureError::Unsupported)

    }

}

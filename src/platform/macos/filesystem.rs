/*! macOS screenshot은 고정된 screencapture를 호출해. Screen Recording 권한은 우회하지 않아. */

#[cfg(target_os = "macos")]
pub(crate) async fn capture(
    path: &std::path::Path,
    display: u32,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<(), crate::error::CaptureError> {

    use std::process::Stdio;
    let command = process_wrap::tokio::CommandWrap::with_new("/usr/sbin/screencapture", |command| {

        command
            .args(["-x", "-t", "png", "-D", &display.to_string()])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

    });
    // caller 입력은 display 번호뿐이야. executable과 출력 경로는 server가 소유해.
    crate::capture::run_command(command, cancellation).await

}

use process_wrap::tokio::CommandWrap;
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

#[test]
#[ignore = "콘솔 정책 테스트가 subprocess로 실행하는 fixture"]
fn console_probe( ) {

    // SAFETY: GetConsoleWindow는 인자나 소유권 이전 없이 현재 process의 console을 조회해.
    let console = unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() };
    println!("no_console={}", console.is_null());
    eprintln!("console_probe_stderr");

}

fn probe_command( ) -> Command {

    let probe_name = "platform::windows::process::tests::console_probe";
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", probe_name, "--ignored", "--nocapture"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command

}

#[tokio::test]
async fn managed_child_has_no_console_and_preserves_output( ) {

    let mut command = CommandWrap::from(probe_command());
    crate::platform::configure(&mut command);
    let mut child = command.spawn().unwrap();
    let mut stdout = child.stdout().take().unwrap();
    let mut stderr = child.stderr().take().unwrap();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let (status, _, _) = timeout(Duration::from_secs(10), async {

        tokio::try_join!(child.wait(), stdout.read_to_end(&mut out), stderr.read_to_end(&mut err))

    })
    .await
    .unwrap()
    .unwrap();
    assert!(status.success());
    assert!(String::from_utf8(out).unwrap().contains("no_console=true"));
    assert!(String::from_utf8(err).unwrap().contains("console_probe_stderr"));

}

#[tokio::test]
async fn background_child_has_no_console_and_preserves_output( ) {

    let mut command = probe_command();
    crate::platform::configure_background(&mut command);
    let output = timeout(Duration::from_secs(10), command.output()).await.unwrap().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("no_console=true"));
    assert!(String::from_utf8(output.stderr).unwrap().contains("console_probe_stderr"));

}

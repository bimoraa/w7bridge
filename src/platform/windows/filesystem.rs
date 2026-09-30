/*! SSH의 Session 0 대신 같은 owner의 interactive task에서 일회성 screenshot을 생성해. */

use crate::{capture::run_command, error::CaptureError};
use base64::{Engine, engine::general_purpose::STANDARD};
use process_wrap::tokio::CommandWrap;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio_util::sync::CancellationToken;

pub(crate) async fn capture(path: &Path, display: u32, cancellation: CancellationToken) -> Result<(), CaptureError> {

    let directory = path.parent().ok_or(CaptureError::Unavailable)?;
    let task = directory.file_name().and_then(|name| name.to_str()).ok_or(CaptureError::Unavailable)?;
    if !task.starts_with("w7bridge-capture-") || !task.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {

        return Err(CaptureError::Unavailable);

    }
    let path = literal(path.to_str().ok_or(CaptureError::Unavailable)?);
    let task = literal(task);
    let worker = worker_script(&path, &task, display);
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$name = {task}
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if ($sid -in @('S-1-5-18', 'S-1-5-19', 'S-1-5-20')) {{ throw 'interactive owner required' }}
$principal = New-ScheduledTaskPrincipal -UserId $sid -LogonType Interactive -RunLevel Limited
$action = New-ScheduledTaskAction -Execute (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Argument '-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand {worker}'
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Seconds 20) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
$registered = $false
try {{
    Register-ScheduledTask -TaskName $name -Action $action -Principal $principal -Settings $settings | Out-Null
    $registered = $true
    Start-ScheduledTask -TaskName $name
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {{ Start-Sleep -Milliseconds 100 }} until ((Test-Path -LiteralPath {path}) -or [DateTime]::UtcNow -ge $deadline)
    if (!(Test-Path -LiteralPath {path})) {{ throw 'screenshot unavailable' }}
}} finally {{
    if ($registered) {{
        Stop-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $name -Confirm:$false -ErrorAction SilentlyContinue
    }}
}}
exit 0
"#,
        worker = encode(&worker)
    );
    let result = run_command(command(&script)?, cancellation).await;
    // 취소가 parent PowerShell을 종료해도 task는 별도로 회수해. 이름은 이 요청이 만든 private temp 이름이야.
    let cleanup = run_command(command(&cleanup_script(&task))?, CancellationToken::new()).await;
    if cleanup.is_err() {

        return Err(CaptureError::Cleanup { cause: result.err().or_else(|| cleanup.err()).map(Box::new) });

    }
    result

}

fn cleanup_script(task: &str) -> String {

    format!(
        r#"
$ErrorActionPreference = 'Stop'
$task = Get-ScheduledTask -TaskName {task} -ErrorAction SilentlyContinue
if ($null -ne $task) {{
    Stop-ScheduledTask -TaskName {task} -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName {task} -Confirm:$false -ErrorAction SilentlyContinue
}}
$remaining = @(Get-ScheduledTask -ErrorAction Stop | Where-Object {{ $_.TaskName -eq {task} -and $_.TaskPath -eq '\' }})
if ($remaining.Count -ne 0) {{ throw 'screenshot task cleanup failed' }}
exit 0
"#
    )

}

fn worker_script(path: &str, task: &str, display: u32) -> String {

    format!(
        r#"
$ErrorActionPreference = 'Stop'
$bitmap = $null
$graphics = $null
try {{
    if (![Environment]::UserInteractive -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0) {{ throw 'interactive desktop required' }}
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    $screens = @([System.Windows.Forms.Screen]::PrimaryScreen) + @([System.Windows.Forms.Screen]::AllScreens | Where-Object {{ !$_.Primary }} | Sort-Object DeviceName)
    if ({display} -gt $screens.Count) {{ throw 'display missing' }}
    $bounds = $screens[{display} - 1].Bounds
    if ($bounds.Width -le 0 -or $bounds.Height -le 0 -or [long]$bounds.Width * $bounds.Height -gt 20000000) {{ throw 'display limit' }}
    $bitmap = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($bounds.X, $bounds.Y, 0, 0, $bounds.Size)
    $partial = {path} + '.partial'
    $bitmap.Save($partial, [System.Drawing.Imaging.ImageFormat]::Png)
    Move-Item -LiteralPath $partial -Destination {path} -ErrorAction Stop
}} finally {{
    if ($null -ne $graphics) {{ $graphics.Dispose() }}
    if ($null -ne $bitmap) {{ $bitmap.Dispose() }}
    Unregister-ScheduledTask -TaskName {task} -Confirm:$false -ErrorAction SilentlyContinue
}}
"#
    )

}

fn literal(value: &str) -> String {

    format!("'{}'", value.replace('\'', "''"))

}

fn encode(script: &str) -> String {

    STANDARD.encode(script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>())

}

fn command(script: &str) -> Result<CommandWrap, CaptureError> {

    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|root| root.is_absolute())
        .ok_or(CaptureError::Unavailable)?;
    Ok(CommandWrap::with_new(root.join("System32/WindowsPowerShell/v1.0/powershell.exe"), |command| {

        command
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encode(script)])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

    }))

}

#[cfg(test)]
#[path = "../../../tests/unit/windows_capture.rs"]
mod tests;

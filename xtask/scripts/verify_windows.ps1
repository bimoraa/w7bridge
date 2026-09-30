param([Parameter(Mandatory=$true)][string]$Archive, [Parameter(Mandatory=$true)][string]$Root)
$ErrorActionPreference = 'Stop'
if (Test-Path $Root) { throw '검증 root가 이미 있습니다. 새 경로를 사용하세요' }
New-Item -ItemType Directory -Path $Root | Out-Null
Set-Location $Root
& tar.exe -xf $Archive
if ($LASTEXITCODE -ne 0) { throw 'source archive 추출 실패' }
$manifest = Get-Content source-manifest.json -Raw | ConvertFrom-Json
foreach ($entry in $manifest.files) {
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $entry.path).Hash.ToLowerInvariant()
    if ($actual -ne $entry.sha256) { throw "source hash 불일치: $($entry.path)" }
}
$vs = 'C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/Common7/Tools/VsDevCmd.bat'
$environmentScript = Join-Path $Root 'native-environment.cmd'
@(
    ('@call "{0}" -no_logo -arch=x64 -host_arch=x64 >nul' -f $vs),
    ('@set > "{0}/native-environment.txt"' -f $Root)
) | Set-Content -Encoding ASCII $environmentScript
& cmd.exe /d /c $environmentScript
if ($LASTEXITCODE -ne 0) { throw 'MSVC 환경 초기화 실패' }
foreach ($line in Get-Content native-environment.txt) {
    if ($line -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$env:PATH = "C:/Users/user/.cargo/bin;$env:PATH"
$env:CARGO_TARGET_DIR = "$Root/target"
$results = @()
$log = Join-Path $Root 'windows-native.log'
"W7BRIDGE_NATIVE_BEGIN $(Get-Date -Format o)" | Tee-Object -FilePath $log
& cargo.exe --version | Tee-Object -FilePath $log -Append
& rustc.exe --version | Tee-Object -FilePath $log -Append
$commands = @(
    @('run','--locked','-p','xtask','--','--check'),
    @('check','--locked','--workspace','--all-targets'),
    @('clippy','--locked','--workspace','--all-targets','--','-D','warnings'),
    @('test','--locked','--workspace'),
    @('build','--locked')
)
foreach ($arguments in $commands) {
    "COMMAND cargo $($arguments -join ' ')" | Tee-Object -FilePath $log -Append
    $ErrorActionPreference = 'Continue'
    & cargo.exe @arguments 2>&1 | Tee-Object -FilePath $log -Append
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    $results += @{command="cargo $($arguments -join ' ')"; exit_code=$code}
    if ($code -ne 0) { break }
}
$result = @{source_id=$manifest.source_id; os=[Environment]::OSVersion.VersionString; commands=$results; completed_at=(Get-Date -Format o); files_verified=$manifest.files.Count}
if (Test-Path target/debug/w7bridge.exe) { $result.binary_sha256=(Get-FileHash target/debug/w7bridge.exe).Hash.ToLowerInvariant() }
$result | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 complete.json
"W7BRIDGE_NATIVE_END $(Get-Date -Format o)" | Tee-Object -FilePath $log -Append
if ($results.Count -ne $commands.Count -or @($results | Where-Object {$_.exit_code -ne 0}).Count -gt 0) { exit 1 }

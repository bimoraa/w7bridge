param(
    [Parameter(Mandatory=$true)][ValidateSet('check','build','test','run')][string]$Action,
    [Parameter(Mandatory=$true)][string]$Cargo,
    [string]$Manifest = 'Cargo.toml',
    [string]$Bun = '',
    [uint32]$MinimumFreeGiB = 30,
    [uint32]$HeadroomGiB = 10
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
try {
    $root = (Get-Location).Path
    $manifestPath = [IO.Path]::GetFullPath((Join-Path $root $Manifest))
    if (-not $manifestPath.StartsWith($root.TrimEnd('\') + '\',[StringComparison]::OrdinalIgnoreCase)) {
        throw 'manifest는 command root 안에 있어야 합니다'
    }
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw 'manifest 파일이 없습니다' }
    $paths = @($root,$Cargo,$env:SystemRoot,$env:TEMP,$env:USERPROFILE)
    if ($env:CARGO_HOME) { $paths += $env:CARGO_HOME }
    if ($env:RUSTUP_HOME) { $paths += $env:RUSTUP_HOME }
    if ($env:CARGO_TARGET_DIR) {
        $target = $env:CARGO_TARGET_DIR
        if (-not [IO.Path]::IsPathRooted($target)) { $target = Join-Path $root $target }
        $paths += $target
    }
    $minimum = ([double]$MinimumFreeGiB + $HeadroomGiB) * 1GB
    foreach ($volume in ($paths | Where-Object { $_ } | ForEach-Object { [IO.Path]::GetPathRoot($_) } | Sort-Object -Unique)) {
        $drive = [IO.DriveInfo]::new($volume)
        if ($drive.AvailableFreeSpace -lt $minimum) {
            throw ('저장 공간 부족: {0} free={1:N2} GiB, required={2} GiB. native command를 시작하지 않았습니다' -f $volume,($drive.AvailableFreeSpace/1GB),($MinimumFreeGiB+$HeadroomGiB))
        }
    }
    if ($Action -eq 'run') {
        if (-not $Bun -or -not (Test-Path -LiteralPath $Bun -PathType Leaf)) { throw 'owner가 승인한 Bun 경로가 없습니다' }
        $active = @(Get-CimInstance Win32_Process -Filter "name='fatomic.exe'" | Where-Object {
            (-not $_.ExecutablePath) -or $_.ExecutablePath.StartsWith($root.TrimEnd('\') + '\',[StringComparison]::OrdinalIgnoreCase)
        })
        if ($active.Count) { throw '이 project의 app이 이미 실행 중입니다. 기존 process를 보존합니다' }
        & $Bun run app:dev
        exit $LASTEXITCODE
    }
    $vs = 'C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools'
    $vc = Get-ChildItem -LiteralPath (Join-Path $vs 'VC/Tools/MSVC') -Directory | Sort-Object Name | Select-Object -Last 1
    $kits = 'C:/Program Files (x86)/Windows Kits/10'
    $sdk = Get-ChildItem -LiteralPath (Join-Path $kits 'Lib') -Directory | Sort-Object Name | Select-Object -Last 1
    if (-not $vc -or -not $sdk) { throw '설치된 MSVC toolchain과 Windows SDK가 없습니다' }
    $env:LIB = @((Join-Path $vc.FullName 'lib/x64'),(Join-Path $sdk.FullName 'ucrt/x64'),(Join-Path $sdk.FullName 'um/x64')) -join ';'
    $env:INCLUDE = @((Join-Path $vc.FullName 'include')) + @('ucrt','shared','um','winrt' | ForEach-Object { Join-Path $kits ('Include/' + $sdk.Name + '/' + $_) }) -join ';'
    $env:PATH = @((Split-Path $Cargo -Parent),(Join-Path $vc.FullName 'bin/Hostx64/x64'),(Join-Path $kits ('bin/' + $sdk.Name + '/x64')),$env:PATH) -join ';'
    $env:VCToolsInstallDir = $vc.FullName + '/'
    $env:VSCMD_ARG_TGT_ARCH = 'x64'
    $arguments = @($Action,'--locked','--manifest-path',$manifestPath)
    if ($Action -eq 'check') { $arguments += '--lib' }
    & $Cargo @arguments
    exit $LASTEXITCODE
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}

param([ValidateSet("x64", "arm64")][string]$Arch = "x64")
$ErrorActionPreference = "Stop"
$Version = "0.16.0"
$ZigArch = "x86_64"
$Root = Join-Path $env:RUNNER_TEMP "zig-$Version-x64"
$Archive = Join-Path $env:RUNNER_TEMP "zig-$Version-x64.zip"
if (-not (Test-Path (Join-Path $Root "zig.exe"))) {
    Invoke-WebRequest "https://ziglang.org/download/$Version/zig-$ZigArch-windows-$Version.zip" -OutFile $Archive
    $Extract = Join-Path $env:RUNNER_TEMP "zig-extract-$Arch"
    if (Test-Path $Extract) { Remove-Item -Recurse -Force $Extract }
    Expand-Archive $Archive $Extract
    $Source = Get-ChildItem $Extract -Directory | Select-Object -First 1
    Move-Item $Source.FullName $Root
}
$Root | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
Write-Host "Prepared Zig $Version (x64 host, $Arch target) at $Root"

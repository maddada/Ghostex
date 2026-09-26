param([ValidateSet("x64", "arm64")][string]$Arch = "x64")
$ErrorActionPreference = "Stop"
$Version = "0.16.0"
$ZigArch = "x86_64"
$CacheRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { Join-Path $PSScriptRoot "../../build/toolchains" }
$CacheRoot = [System.IO.Path]::GetFullPath($CacheRoot)
$Root = Join-Path $CacheRoot "zig-$Version-x64"
$Archive = Join-Path $CacheRoot "zig-$Version-x64.zip"
if (-not (Test-Path (Join-Path $Root "zig.exe"))) {
    New-Item -ItemType Directory -Force $CacheRoot | Out-Null
    Invoke-WebRequest "https://ziglang.org/download/$Version/zig-$ZigArch-windows-$Version.zip" -OutFile $Archive -UseBasicParsing -TimeoutSec 300
    $ExpectedHash = "68659eb5f1e4eb1437a722f1dd889c5a322c9954607f5edcf337bc3684a75a7e"
    if ((Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ExpectedHash) {
        throw "Zig $Version download checksum does not match."
    }
    $Extract = Join-Path $CacheRoot ("zig-extract-" + [Guid]::NewGuid().ToString("N"))
    Expand-Archive -LiteralPath $Archive -DestinationPath $Extract
    $Source = Join-Path $Extract "zig-$ZigArch-windows-$Version"
    $CachePrefix = $CacheRoot.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    foreach ($Candidate in @($Source, $Root)) {
        if (![System.IO.Path]::GetFullPath($Candidate).StartsWith($CachePrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Zig staging path must stay inside $CacheRoot"
        }
    }
    Move-Item -LiteralPath $Source -Destination $Root
    Remove-Item -LiteralPath $Extract
}
$Zig = Join-Path $Root "zig.exe"
if ((& $Zig version) -ne $Version -or $LASTEXITCODE -ne 0) { throw "Expected Zig $Version at $Zig" }
$env:GHOSTEX_ZIG = $Zig
$env:PATH = "$Root;$env:PATH"
if ($env:GITHUB_PATH) { $Root | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append }
Write-Host "Prepared Zig $Version (x64 host, $Arch target) at $Root"

# CDXC:GPUIWindowsBringup 2026-07-04:
# Windows packaging skeleton for the GPUI app, mirroring the shape of
# build-macos-app.sh: build the sidebar bundle, build both Rust binaries,
# then stage a flat CEF-conventional layout. Written best-effort from macOS
# during P2 (Windows bring-up) — NEEDS-DEVICE-VERIFY: never executed on real
# Windows hardware. Deliberately not yet covered here (macOS-script parity
# items to port as Windows support matures): completion sound assets, CLI
# resources, portless admin runtime, and remote gxserver Linux packages. The
# release workflow wraps this staged directory with Velopack, which injects
# the installed/portable updater manifest and creates signed packages.
#
# Development layouts keep the conventional flat CEF payload beside the app.
# Release layouts stage a CEF-free native bootstrap plus an internal runtime;
# the bootstrap installs the sealed component and starts that runtime with the
# component directory on PATH.
#   build/windows/Ghostex/
#     Ghostex.exe
#     ghostex-gpui-cef-helper.exe      <- cef/windows.rs sets this as
#                                         browser_subprocess_path (sibling)
#     libcef.dll, chrome_elf.dll, ...  <- CEF Release/ payload
#     icudtl.dat, *.pak, locales/      <- CEF Resources/ payload
#     dist/sidebar/                    <- sidebar bundle; the /dist/sidebar/
#                                         path segment is load-bearing for the
#                                         CEF helper first-party URL check and
#                                         the sidebar_url() Windows arm.

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$GpuiDir = Resolve-Path (Join-Path $ScriptDir "..")
$RepoRoot = Resolve-Path (Join-Path $GpuiDir "../..")
$AppName = "Ghostex"
$AppDir = Join-Path $GpuiDir "build/windows/$AppName"
$OnDemandComponents = $env:GHOSTEX_ON_DEMAND_ASSETS -eq "1"
$ReleaseArch = if ($env:GHOSTEX_WINDOWS_ARCH) { $env:GHOSTEX_WINDOWS_ARCH } else { "x64" }
$ReleaseVersion = if ($env:GHOSTEX_GPUI_MARKETING_VERSION) {
    $env:GHOSTEX_GPUI_MARKETING_VERSION
} else {
    (Get-Content -Raw (Join-Path $RepoRoot "package.json") | ConvertFrom-Json).version
}
if ($ReleaseArch -notin @("x64", "arm64")) {
    throw "GHOSTEX_WINDOWS_ARCH must be x64 or arm64, got $ReleaseArch"
}

# Same CEF cache location contract as build-macos-app.sh: cef-dll-sys's build
# script downloads the CEF binary distribution into CEF_PATH.
$CefCacheDir = Join-Path $GpuiDir "build/cef-cache"
$env:CEF_PATH = $CefCacheDir
$env:ZIG_GLOBAL_CACHE_DIR = Join-Path $RepoRoot "build/zig-global-cache"
New-Item -ItemType Directory -Force -Path $env:ZIG_GLOBAL_CACHE_DIR | Out-Null

# 1) Sidebar bundle (same steps as the macOS script).
Push-Location $RepoRoot
try {
    bun run build:sidebar-css
    if ($LASTEXITCODE -ne 0) { throw "build:sidebar-css failed" }
    & (Join-Path $RepoRoot "node_modules/.bin/vite.exe") build --config (Join-Path $GpuiDir "vite.config.ts")
    if ($LASTEXITCODE -ne 0) { throw "vite build failed" }
}
finally {
    Pop-Location
}

# 2) Rust binaries (bootstrap, main app, CEF helper, and installer launcher). Requires MSVC toolchain, cmake,
# and ninja (cef-dll-sys builds libcef_dll_wrapper), plus Zig 0.16.x for
# libghostty-vt (GHOSTEX_ZIG override honored by apps/desktop/build.rs).
Push-Location $GpuiDir
try {
    cargo build --release --bin ghostex-gpui-cef-bootstrap --bin ghostex-gpui --bin ghostex-gpui-cef-helper --bin ghostex-windows-installer
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
finally {
    Pop-Location
}

# 3) Locate the extracted CEF distribution. cef-dll-sys may export either a
# flat Windows payload or the upstream Release/ + Resources/ layout.
$LibCef = Get-ChildItem -Path $CefCacheDir -Recurse -File -Filter "libcef.dll" |
    Select-Object -First 1
if (-not $LibCef) {
    throw "cef-rs did not produce libcef.dll under $CefCacheDir"
}
$CefRelease = $LibCef.Directory
$CefResources = $CefRelease.FullName
if (-not (Test-Path (Join-Path $CefResources "icudtl.dat"))) {
    $CefResources = Join-Path (Split-Path -Parent $CefRelease.FullName) "Resources"
    if (-not (Test-Path (Join-Path $CefResources "icudtl.dat"))) {
        throw "CEF resources with icudtl.dat were not found beside libcef.dll or at $CefResources"
    }
}
$CefDistributionRoot = if (Test-Path (Join-Path $CefRelease.FullName "include/cef_version.h")) {
    $CefRelease.FullName
} else {
    Split-Path -Parent $CefRelease.FullName
}
$CefVersionHeader = Join-Path $CefDistributionRoot "include/cef_version.h"
if (-not (Test-Path $CefVersionHeader)) {
    throw "Could not locate cef_version.h for $($CefRelease.FullName)"
}
$CefVersionMatch = Select-String -Path $CefVersionHeader -Pattern '^#define CEF_VERSION "([^"]+)"$' |
    Select-Object -First 1
if (-not $CefVersionMatch) {
    throw "Could not resolve the CEF component version from $CefVersionHeader"
}
$CefComponentVersion = $CefVersionMatch.Matches[0].Groups[1].Value -replace '[^A-Za-z0-9._-]', '-'

# 4) Stage the app directory. Clear generated contents without deleting the
# directory inode, because a terminal may still have the staged directory as
# its working directory after the previous app process exits.
if (Test-Path $AppDir) {
    Get-ChildItem -LiteralPath $AppDir -Force | Remove-Item -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $AppDir | Out-Null

if ($OnDemandComponents) {
    Copy-Item (Join-Path $GpuiDir "target/release/ghostex-gpui-cef-bootstrap.exe") (Join-Path $AppDir "Ghostex.exe")
    Copy-Item (Join-Path $GpuiDir "target/release/ghostex-gpui.exe") (Join-Path $AppDir "ghostex-gpui-runtime.exe")
}
else {
    Copy-Item (Join-Path $GpuiDir "target/release/ghostex-gpui.exe") (Join-Path $AppDir "Ghostex.exe")
}
Copy-Item (Join-Path $GpuiDir "target/release/ghostex-gpui-cef-helper.exe") $AppDir
$SwiftshaderIcd = @(
    (Join-Path $CefRelease.FullName "vk_swiftshader_icd.json"),
    (Join-Path $CefResources "vk_swiftshader_icd.json")
) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
$Locales = @(
    (Join-Path $CefRelease.FullName "locales"),
    (Join-Path $CefResources "locales")
) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $Locales) {
    throw "CEF locales were not found beside libcef.dll or at $CefResources"
}
if (-not $OnDemandComponents) {
    Copy-Item (Join-Path $CefRelease.FullName "*.dll") $AppDir
    Copy-Item (Join-Path $CefRelease.FullName "*.pak") $AppDir
    Copy-Item (Join-Path $CefRelease.FullName "*.dat") $AppDir
    Copy-Item (Join-Path $CefRelease.FullName "*.bin") $AppDir
    if ($CefResources -ne $CefRelease.FullName) {
        Copy-Item (Join-Path $CefResources "*.pak") $AppDir
        Copy-Item (Join-Path $CefResources "*.dat") $AppDir
        Copy-Item (Join-Path $CefResources "*.bin") $AppDir
    }
    if ($SwiftshaderIcd) { Copy-Item -LiteralPath $SwiftshaderIcd $AppDir }
    Copy-Item -Recurse -LiteralPath $Locales -Destination (Join-Path $AppDir "locales")
}
New-Item -ItemType Directory -Force -Path (Join-Path $AppDir "dist") | Out-Null
Copy-Item -Recurse (Join-Path $GpuiDir "dist/sidebar") (Join-Path $AppDir "dist/sidebar")

$ComponentRoot = Join-Path $RepoRoot "build/on-demand-components"
$ComponentAssetDir = Join-Path $ComponentRoot "assets"
$ComponentManifest = Join-Path $ComponentRoot "components.json"
if ($OnDemandComponents) {
    $CefComponentStage = Join-Path $ComponentRoot "cef-windows-$ReleaseArch-stage"
    $CefComponentAsset = Join-Path $ComponentAssetDir "cef-$CefComponentVersion-windows-$ReleaseArch.tar.gz"
    New-Item -ItemType Directory -Force -Path $ComponentAssetDir | Out-Null
    '{"components":{}}' | Set-Content -Encoding UTF8 $ComponentManifest
    if (Test-Path $CefComponentStage) { Remove-Item -Recurse -Force $CefComponentStage }
    New-Item -ItemType Directory -Force -Path $CefComponentStage | Out-Null
    foreach ($sourceRoot in @($CefRelease.FullName, $CefResources) | Select-Object -Unique) {
        foreach ($pattern in @("*.dll", "*.pak", "*.dat", "*.bin")) {
            Copy-Item (Join-Path $sourceRoot $pattern) $CefComponentStage -ErrorAction SilentlyContinue
        }
    }
    if ($SwiftshaderIcd) { Copy-Item -LiteralPath $SwiftshaderIcd $CefComponentStage }
    Copy-Item -Recurse -LiteralPath $Locales -Destination (Join-Path $CefComponentStage "locales")
    & bash (Join-Path $RepoRoot "scripts/release-gpui/create-deterministic-tar.sh") $CefComponentStage $CefComponentAsset --windows-component
    if ($LASTEXITCODE -ne 0) { throw "Could not create the deterministic Windows CEF component asset" }
    Remove-Item -Recurse -Force $CefComponentStage
    & node (Join-Path $RepoRoot "scripts/release-gpui/publish-component.mjs") `
        --metadata-only `
        --reuse-published `
        --component cef `
        --version $CefComponentVersion `
        --platform "windows-$ReleaseArch" `
        --asset-dir $ComponentAssetDir `
        --output $ComponentManifest
    if ($LASTEXITCODE -ne 0) { throw "Could not seal Windows CEF component metadata" }
}

# Windows is WSL2-only for now. The base app keeps its matching gxserver
# runtime, while Source/code-server is sealed as an optional component and is
# never copied into the installer.
$WslArchive = $env:GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE
$WslCodeServerArchive = $env:GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE
$RequireWslArchive = $env:GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME -ne "0"
if ($WslArchive -and (Test-Path $WslArchive)) {
    $WslResources = Join-Path $AppDir "resources/wsl"
    New-Item -ItemType Directory -Force -Path $WslResources | Out-Null
    $StagedWslArchive = Join-Path $WslResources "gxserver-linux-$ReleaseArch.tar.gz"
    Copy-Item $WslArchive $StagedWslArchive
    $Sha256 = [Security.Cryptography.SHA256]::Create()
    $ArchiveStream = [IO.File]::OpenRead($StagedWslArchive)
    try {
        $StagedWslSha = -join ($Sha256.ComputeHash($ArchiveStream) | ForEach-Object {
            $_.ToString("x2")
        })
    }
    finally {
        $ArchiveStream.Dispose()
        $Sha256.Dispose()
    }
    [IO.File]::WriteAllText(
        "$StagedWslArchive.sha256",
        "$StagedWslSha`n",
        [Text.UTF8Encoding]::new($false)
    )
}
elseif ($RequireWslArchive) {
    throw "Required WSL gxserver archive is missing: $WslArchive"
}
if ($WslCodeServerArchive -and (Test-Path $WslCodeServerArchive)) {
    $ComponentVersion = $env:GHOSTEX_CODE_SERVER_COMPONENT_VERSION
    if (-not $ComponentVersion) {
        $ComponentVersion = (& node (Join-Path $RepoRoot "scripts/release-gpui/code-server-component-identity.mjs") --root (Join-Path $RepoRoot ".dependencies/code-server")).Trim()
        if ($LASTEXITCODE -ne 0 -or -not $ComponentVersion) {
            throw "Could not resolve the code-server component payload identity"
        }
    }
    $ExpectedArchiveName = "code-server-$ComponentVersion-linux-$ReleaseArch.tar.gz"
    if ((Split-Path -Leaf $WslCodeServerArchive) -ne $ExpectedArchiveName) {
        throw "WSL code-server archive identity mismatch: expected $ExpectedArchiveName"
    }
    & node (Join-Path $RepoRoot "scripts/release-gpui/verify-code-server-archive.mjs") `
        --archive $WslCodeServerArchive `
        --version $ComponentVersion `
        --platform "linux-$ReleaseArch"
    if ($LASTEXITCODE -ne 0) {
        throw "WSL code-server archive verification failed"
    }
    $ComponentStage = Join-Path $ComponentRoot "windows-$ReleaseArch-stage"
    $InnerArchiveName = $ExpectedArchiveName
    $ComponentAsset = Join-Path $ComponentAssetDir "code-server-$ComponentVersion-windows-$ReleaseArch.tar.gz"
    New-Item -ItemType Directory -Force -Path $ComponentAssetDir | Out-Null
    if (Test-Path $ComponentStage) { Remove-Item -Recurse -Force $ComponentStage }
    New-Item -ItemType Directory -Force -Path $ComponentStage | Out-Null
    Copy-Item $WslCodeServerArchive (Join-Path $ComponentStage $InnerArchiveName)
    Copy-Item "$WslCodeServerArchive.sha256" (Join-Path $ComponentStage "$InnerArchiveName.sha256")
    & bash (Join-Path $RepoRoot "scripts/release-gpui/create-deterministic-tar.sh") $ComponentStage $ComponentAsset --windows-component
    if ($LASTEXITCODE -ne 0) { throw "Could not create the deterministic Windows code-server component asset" }
    Remove-Item -Recurse -Force $ComponentStage
    $ComponentAssetSha = (Get-FileHash -Algorithm SHA256 $ComponentAsset).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText(
        "$ComponentAsset.sha256",
        "$ComponentAssetSha  $(Split-Path -Leaf $ComponentAsset)`n",
        [Text.UTF8Encoding]::new($false)
    )
    & node (Join-Path $RepoRoot "scripts/release-gpui/publish-component.mjs") `
        --metadata-only `
        --reuse-published `
        --component code-server `
        --version $ComponentVersion `
        --platform "windows-$ReleaseArch" `
        --asset-dir $ComponentAssetDir `
        --require-sha256-sidecars `
        --output $ComponentManifest
    if ($LASTEXITCODE -ne 0) { throw "Could not seal Windows code-server component metadata" }

}
elseif ($RequireWslArchive) {
    throw "Required WSL Source archive is missing: $WslCodeServerArchive"
}

if ($OnDemandComponents -or ($WslCodeServerArchive -and (Test-Path $WslCodeServerArchive))) {
    $OnDemandBuildManifest = Join-Path $ComponentRoot "windows-$ReleaseArch-assets.json"
    $OnDemandBuildPayload = [ordered]@{ assets = @(); version = $ReleaseVersion } |
        ConvertTo-Json -Depth 4
    [IO.File]::WriteAllText(
        $OnDemandBuildManifest,
        "$OnDemandBuildPayload`n",
        [Text.UTF8Encoding]::new($false)
    )
    $ResourcesDir = Join-Path $AppDir "resources"
    New-Item -ItemType Directory -Force -Path $ResourcesDir | Out-Null
    & node (Join-Path $RepoRoot "scripts/release-gpui/on-demand-manifest.mjs") seal `
        --build-manifest $OnDemandBuildManifest `
        --component-manifest $ComponentManifest `
        --output (Join-Path $ResourcesDir "on-demand-resources.json") `
        --repo "maddada/Ghostex"
    if ($LASTEXITCODE -ne 0) { throw "Could not seal the Windows on-demand manifest" }
}

Write-Host "Staged $AppDir"

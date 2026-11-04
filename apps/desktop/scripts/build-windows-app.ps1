# Build the sidebar, desktop and native Windows session binaries, then stage
# the CEF runtime and bundled resources. The release workflow wraps this
# directory with Velopack to create the signed installer and update packages.
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
#     dist/sidebar/                    <- the CEF page bundles; the
#                                         /dist/sidebar/ path segment is
#                                         load-bearing for the CEF helper's
#                                         first-party URL check.

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$GpuiDir = Resolve-Path (Join-Path $ScriptDir "..")
$RepoRoot = Resolve-Path (Join-Path $GpuiDir "../..")

& (Join-Path $RepoRoot "tooling/prepare-windows-build.ps1")

<#
CDXC:Build 2026-09-18 WHY:
create-deterministic-tar.sh is a Git for Windows script: it takes Windows paths
and branches on a MINGW/MSYS/CYGWIN `uname`. Resolving it through `bash` on PATH
picked WSL's Linux bash on any machine with WSL installed, because Git for
Windows keeps bash.exe in Git\bin, which it deliberately leaves off PATH. WSL
bash cannot open `C:\...` arguments, so packaging died with no diagnostic.
Resolve bash beside git.exe, the same way build-windows-code-server.ps1 does.
Walk up from every git.exe on PATH rather than assuming one fixed depth: a
start launched from a Git Bash shell resolves git.exe to Git\mingw64\bin, two
levels below the install root, while a PowerShell start resolves it to Git\cmd,
one level below.
#>
function Get-GitBash {
    foreach ($GitPath in (Get-Command git.exe -All -ErrorAction Stop | Select-Object -ExpandProperty Source)) {
        $Candidate = Split-Path $GitPath -Parent
        for ($Level = 0; $Level -lt 3 -and $Candidate; $Level += 1) {
            $Candidate = Split-Path $Candidate -Parent
            if (!$Candidate) { break }
            $Bash = Join-Path $Candidate "bin/bash.exe"
            if (Test-Path $Bash) { return $Bash }
        }
    }
    throw "Git for Windows bash.exe was not found beside git.exe."
}
$GitBash = Get-GitBash

<#
CDXC:Build 2026-10-11 WHY:
Every release build here records the units it used in target/release/ghostex-build-units.jsonl (cargo's JSON messages on stdout; diagnostics and progress still render on the console). `cargo xtask start` reads it to delete the stale artifacts cargo never removes (tooling/xtask/src/start/sweep.rs).
#>
function Invoke-CargoReleaseBuild([string[]]$CargoArguments) {
    $ReleaseDir = Join-Path (Get-Location) "target/release"
    New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null
    # `--timings` only writes target/cargo-timings/*.html; it is not a rustc flag, so the
    # artifacts and the kache keys are unchanged. Opt-in (GHOSTEX_CARGO_TIMINGS=1) from the
    # release workflow, as on macOS (tooling/release-gpui/macos.sh).
    $TimingsArguments = @(if ($env:GHOSTEX_CARGO_TIMINGS -eq "1") { "--timings" })
    cargo build --release @CargoArguments @TimingsArguments --message-format=json-render-diagnostics |
        Set-Content -Encoding UTF8 -Path (Join-Path $ReleaseDir "ghostex-build-units.jsonl")
}
$AppName = "Ghostex"
$AppDir = Join-Path $GpuiDir "build/windows/$AppName"
$OnDemandComponents = $env:GHOSTEX_ON_DEMAND_ASSETS -eq "1"
# CDXC:Release 2026-10-20 DECISION: the user chose to ship the native Windows editor as a download-on-first-use component instead of inside the installer (about 727 MB unpacked). An on-demand build that has the verified published archive (GHOSTEX_WINDOWS_NATIVE_CODE_SERVER_ARCHIVE, from release-gpui-code-server-windows.yml) seals it as code-server windows-native-<arch>; a build without it (local starts, a standalone dispatch that compiled the editor) still bundles code-server/, since that archive was never published.
# SEE-ALSO: source_code_server_local_component_platform in apps/desktop/src/app/helpers/source_server/code_server.rs, tooling/release-gpui/windows.ps1, tooling/release-gpui/assemble.mjs.
$NativeCodeServerArchive = $env:GHOSTEX_WINDOWS_NATIVE_CODE_SERVER_ARCHIVE
$NativeCodeOnDemand = $OnDemandComponents -and $NativeCodeServerArchive -and (Test-Path -LiteralPath $NativeCodeServerArchive)
$ReleaseArch = if ($env:GHOSTEX_WINDOWS_ARCH) { $env:GHOSTEX_WINDOWS_ARCH } else { "x64" }
$ReleaseVersion = if ($env:GHOSTEX_GPUI_MARKETING_VERSION) {
    $env:GHOSTEX_GPUI_MARKETING_VERSION
} else {
    (Get-Content -Raw (Join-Path $RepoRoot "package.json") | ConvertFrom-Json).version
}
if ($ReleaseArch -notin @("x64", "arm64")) {
    throw "GHOSTEX_WINDOWS_ARCH must be x64 or arm64, got $ReleaseArch"
}
# CDXC:Release 2026-09-02: tooling/release-gpui/windows.ps1 runs
# this script in two watched release steps. "compile" stops after the Rust
# builds and "stage" skips them (reusing that compile's dist/ and target/
# output); unset runs everything, which is what every other caller does.
# "compile-app" (sidebar bundle + desktop crate) and "compile-runtime" (native
# gxserver, wmx, prompt editor) are the two halves of "compile", which the
# release builds on separate runners (see windows.ps1).
$BuildPhase = if ($env:GHOSTEX_WINDOWS_BUILD_PHASE) { $env:GHOSTEX_WINDOWS_BUILD_PHASE } else { "all" }
if ($BuildPhase -notin @("all", "compile", "compile-app", "compile-runtime", "stage")) {
    throw "GHOSTEX_WINDOWS_BUILD_PHASE must be compile, compile-app, compile-runtime, stage, or unset, got $BuildPhase"
}
$BuildApp = $BuildPhase -in @("all", "compile", "compile-app")
$BuildRuntime = $BuildPhase -in @("all", "compile", "compile-runtime")

# Same CEF cache location contract as build-macos-app.sh: cef-dll-sys's build
# script downloads the CEF binary distribution into CEF_PATH.
$CefCacheDir = Join-Path $GpuiDir "build/cef-cache"
$env:CEF_PATH = $CefCacheDir
# cef-dll-sys uses CEF_PATH/<CEF build>/ only when that folder exists and
# otherwise accepts an older distribution sitting in CEF_PATH, so create the
# pinned build's folder first and stage only from it (see cef-distribution.sh).
# The runtime half links no CEF and runs without the cef-rs reference checkout.
if ($BuildPhase -ne "compile-runtime") {
    $CefRsManifest = Join-Path $RepoRoot ".dependencies/cef-rs/Cargo.toml"
    $CefBuildMatch = Select-String -Path $CefRsManifest -Pattern '^version = "[^"+]*\+([^"]+)"$' |
        Select-Object -First 1
    if (-not $CefBuildMatch) {
        throw "Could not read the pinned CEF build from $CefRsManifest"
    }
    $CefVersionedDir = Join-Path $CefCacheDir $CefBuildMatch.Matches[0].Groups[1].Value
    New-Item -ItemType Directory -Force -Path $CefVersionedDir | Out-Null
}
$env:ZIG_GLOBAL_CACHE_DIR = Join-Path $RepoRoot "build/zig-global-cache"
New-Item -ItemType Directory -Force -Path $env:ZIG_GLOBAL_CACHE_DIR | Out-Null

if ($BuildApp) {
    # 1) Sidebar bundle (same steps as the macOS script).
    Push-Location $RepoRoot
    try {
        bun x tailwindcss -i packages/core-ui/styles/shadcn.css -o packages/core-ui/styles/shadcn.generated.css --minify
        if ($LASTEXITCODE -ne 0) { throw "tailwindcss (sidebar CSS) failed" }
        & (Join-Path $RepoRoot "node_modules/.bin/vite.exe") build --config (Join-Path $GpuiDir "vite.config.ts")
        if ($LASTEXITCODE -ne 0) { throw "vite build failed" }
    }
    finally {
        Pop-Location
    }

    # 2) Rust binaries (bootstrap, main app, CEF helper, and installer launcher). Requires MSVC toolchain, cmake,
    # and ninja (cef-dll-sys builds libcef_dll_wrapper), plus Zig 0.17.x for
    # libghostty-vt (GHOSTEX_ZIG override honored by apps/desktop/build.rs).
    Push-Location $GpuiDir
    try {
        Invoke-CargoReleaseBuild @("--bin", "ghostex-gpui-cef-bootstrap", "--bin", "ghostex-gpui", "--bin", "ghostex-gpui-cef-helper", "--bin", "ghostex-windows-installer", "--bin", "ghostex-windows-installer-pack")
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }
    finally {
        Pop-Location
    }
}

if ($BuildRuntime) {
    Push-Location (Join-Path $RepoRoot "server")
    try {
        # The app ships gxserver.exe alone (ghostex.exe is made on the PC, apps/desktop/src/windows_cli_link.rs); ghostex.exe is still compiled because .github/workflows/release-gpui-windows.yml checks and uploads it.
        Invoke-CargoReleaseBuild @("--bin", "gxserver", "--bin", "ghostex")
        if ($LASTEXITCODE -ne 0) { throw "Native Windows runtime build failed" }
    }
    finally { Pop-Location }
    Push-Location (Join-Path $RepoRoot ".dependencies/wmx")
    try {
        Invoke-CargoReleaseBuild @("--locked")
        if ($LASTEXITCODE -ne 0) { throw "wmx build failed" }
    }
    finally { Pop-Location }
}

# CDXC:PromptEditor 2026-09-16 WHY:
# Ctrl+G needs the standalone helper as well as Code. Omitting it made every Windows build report the prompt editor unavailable.
if ($BuildRuntime) {
    Push-Location $RepoRoot
    try {
        bun apps/editor/scripts/build-editor-web.mjs
        if ($LASTEXITCODE -ne 0) { throw "Prompt editor page build failed" }
    }
    finally { Pop-Location }
    # Built from inside its folder so cargo reads apps/editor/desktop/.cargo/config.toml (the static C runtime).
    Push-Location (Join-Path $RepoRoot "apps/editor/desktop")
    try {
        Invoke-CargoReleaseBuild @()
        if ($LASTEXITCODE -ne 0) { throw "Prompt editor helper build failed" }
    }
    finally { Pop-Location }
}

if ($BuildPhase -like "compile*") {
    Write-Host "Compiled $AppName ($ReleaseArch, $BuildPhase); staging deferred to the stage phase"
    exit 0
}

# CDXC:Release 2026-09-14 WHY:
# The native editor payload is consumed only by staging. Building it during compile made both Windows Rust cache jobs fail on the editor's pinned Node requirement before reaching cargo.
# CDXC:Release 2026-09-16 WHY:
# The release workflow hands the verified native editor component archive to the script through GHOSTEX_WINDOWS_NATIVE_CODE_SERVER_ARCHIVE.
# Without it the script reuses the published component when the code-server tree is clean and builds from source otherwise, so local builds keep working without CI.
if (-not $NativeCodeOnDemand) {
    & (Join-Path $ScriptDir "build-windows-code-server.ps1")
}

# 3) Locate the extracted CEF distribution. cef-dll-sys may export either a
# flat Windows payload or the upstream Release/ + Resources/ layout.
$LibCef = Get-ChildItem -Path $CefVersionedDir -Recurse -File -Filter "libcef.dll" |
    Select-Object -First 1
if (-not $LibCef) {
    throw "cef-rs did not produce libcef.dll under $CefVersionedDir"
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

# CDXC:PlatformSupport 2026-09-14 WHY:
# Native session hosts outlive the app and keep their executable mapped.
# Retire the old runtime before staging so rebuilding does not kill agents or fail on a locked host executable.
$PreviousNativeRuntime = Join-Path $AppDir "resources/native"
if (Test-Path $PreviousNativeRuntime) {
    $RetiredNativeRoot = Join-Path $GpuiDir "build/windows/retired-native-runtimes"
    New-Item -ItemType Directory -Force -Path $RetiredNativeRoot | Out-Null
    Get-ChildItem -LiteralPath $RetiredNativeRoot -Directory | ForEach-Object {
        try { Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction Stop }
        catch { Write-Verbose "A retired native runtime is still in use." }
    }
    Move-Item -LiteralPath $PreviousNativeRuntime -Destination (Join-Path $RetiredNativeRoot ([Guid]::NewGuid().ToString("N")))
}

# 4) Stage the app directory. Clear generated contents without deleting the
# directory inode, because a terminal may still have the staged directory as
# its working directory after the previous app process exits.
if (Test-Path $AppDir) {
    Get-ChildItem -LiteralPath $AppDir -Force | Remove-Item -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $AppDir | Out-Null

# CDXC:CefRuntime 2026-09-28 WHY: with on-demand components Ghostex.exe only launches
# ghostex-gpui-runtime.exe. The runtime delay-loads libcef.dll and downloads the sealed `cef`
# component only when the user installs the web runtime, so the app folder carries no CEF files.
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
# Completion and copy sounds, played from resources/sounds beside the executable.
# Keep in step with build-windows-app-wsl.sh, build-linux-app.sh and the macOS asset list in build-macos-app.sh.
$SoundResources = Join-Path $AppDir "resources/sounds"
New-Item -ItemType Directory -Force -Path $SoundResources | Out-Null
Copy-Item (Join-Path $RepoRoot "media/sounds/*.mp3") $SoundResources

$ComponentRoot = Join-Path $RepoRoot "build/on-demand-components"
$ComponentAssetDir = Join-Path $ComponentRoot "assets"
$ComponentManifest = Join-Path $ComponentRoot "components.json"
if ($OnDemandComponents) {
    $CefComponentStage = Join-Path $ComponentRoot "cef-windows-$ReleaseArch-stage"
    $CefComponentAsset = Join-Path $ComponentAssetDir "cef-$CefComponentVersion-windows-$ReleaseArch.tar.gz"
    New-Item -ItemType Directory -Force -Path $ComponentAssetDir | Out-Null
    # CDXC:Build 2026-10-18 WHY: Windows PowerShell 5.1 (what local `cargo xtask start` runs) writes UTF8 with a BOM, which the Node component sealer rejects.
    [IO.File]::WriteAllText($ComponentManifest, '{"components":{}}' + "`n", [Text.UTF8Encoding]::new($false))
    if (Test-Path $CefComponentStage) { Remove-Item -Recurse -Force $CefComponentStage }
    New-Item -ItemType Directory -Force -Path $CefComponentStage | Out-Null
    foreach ($sourceRoot in @($CefRelease.FullName, $CefResources) | Select-Object -Unique) {
        foreach ($pattern in @("*.dll", "*.pak", "*.dat", "*.bin")) {
            Copy-Item (Join-Path $sourceRoot $pattern) $CefComponentStage -ErrorAction SilentlyContinue
        }
    }
    if ($SwiftshaderIcd) { Copy-Item -LiteralPath $SwiftshaderIcd $CefComponentStage }
    Copy-Item -Recurse -LiteralPath $Locales -Destination (Join-Path $CefComponentStage "locales")
    & $GitBash (Join-Path $RepoRoot "tooling/release-gpui/create-deterministic-tar.sh") $CefComponentStage $CefComponentAsset --windows-component
    if ($LASTEXITCODE -ne 0) { throw "Could not create the deterministic Windows CEF component asset" }
    Remove-Item -Recurse -Force $CefComponentStage
    & node (Join-Path $RepoRoot "tooling/release-gpui/publish-component.mjs") `
        --metadata-only `
        --require-published-match `
        --component cef `
        --version $CefComponentVersion `
        --platform "windows-$ReleaseArch" `
        --asset-dir $ComponentAssetDir `
        --output $ComponentManifest
    if ($LASTEXITCODE -ne 0) { throw "Could not seal Windows CEF component metadata" }
}

$PromptEditorResources = Join-Path $AppDir "resources/GhostexEditor"
New-Item -ItemType Directory -Force -Path $PromptEditorResources | Out-Null
Copy-Item (Join-Path $RepoRoot "apps/editor/desktop/target/release/ghostex-editor.exe") (Join-Path $PromptEditorResources "GhostexEditor.exe")
Copy-Item (Join-Path $RepoRoot "apps/editor/dist/web") (Join-Path $PromptEditorResources "web") -Recurse

# Both environments ship their matching runtime. Source/code-server remains
# an optional WSL component.
$NativeResources = Join-Path $AppDir "resources/native"
New-Item -ItemType Directory -Force -Path $NativeResources | Out-Null
# ghostex.exe is gxserver.exe under the CLI's name: the app makes it a hard link on the PC, because the installer package cannot hold links (CDXC:Release 2026-10-23 in server/src/main.rs).
Copy-Item (Join-Path $RepoRoot "server/target/release/gxserver.exe") $NativeResources
Copy-Item (Join-Path $RepoRoot ".dependencies/wmx/target/release/wmx.exe") $NativeResources
# CDXC:AgentSkills 2026-09-27 WHY:
# The native CLI resolves its offline skill catalog next to resources/native.
# First-run setup must work without a source checkout or a GitHub download.
Copy-Item -LiteralPath (Join-Path $RepoRoot "skills") -Destination (Join-Path $AppDir "resources/skills") -Recurse
& (Join-Path $PSScriptRoot "write-windows-native-identity.ps1") -AppDir $AppDir
$WslArchive = $env:GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE
$WslCodeServerArchive = $env:GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE
$RequireWslArchive = $env:GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME -ne "0"
$RequireSourceArchive = $OnDemandComponents
$WslOnDemandAssets = @()
# CDXC:Release 2026-10-20 WHY: release builds list the WSL gxserver archive in the sealed on-demand manifest instead of bundling it (27 MB): it is the same gxserver-linux-<arch>.tar.gz every release already attaches, and the app downloads it on the first WSL setup or update. Dev builds keep bundling the archive they rebuild from source, which is never uploaded.
# SEE-ALSO: resolve_packaged_gxserver in apps/desktop/src/windows_terminal_backend/platform/packaged_install.rs, tooling/release-gpui/windows.ps1, tooling/release-gpui/assemble.mjs.
if ($OnDemandComponents -and $WslArchive -and (Test-Path $WslArchive)) {
    $WslOnDemandAssets = @([ordered]@{
        bytes = (Get-Item -LiteralPath $WslArchive).Length
        key = "gxserver-linux-$ReleaseArch"
        name = "gxserver-linux-$ReleaseArch.tar.gz"
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $WslArchive).Hash.ToLowerInvariant()
    })
}
elseif ($WslArchive -and (Test-Path $WslArchive)) {
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
        $ComponentVersion = (& node (Join-Path $RepoRoot "tooling/release-gpui/code-server-component-identity.mjs") --root (Join-Path $RepoRoot ".dependencies/code-server")).Trim()
        if ($LASTEXITCODE -ne 0 -or -not $ComponentVersion) {
            throw "Could not resolve the code-server component payload identity"
        }
    }
    $ExpectedArchiveName = "code-server-$ComponentVersion-linux-$ReleaseArch.tar.gz"
    if ((Split-Path -Leaf $WslCodeServerArchive) -ne $ExpectedArchiveName) {
        throw "WSL code-server archive identity mismatch: expected $ExpectedArchiveName"
    }
    & node (Join-Path $RepoRoot "tooling/release-gpui/verify-code-server-archive.mjs") `
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
    & $GitBash (Join-Path $RepoRoot "tooling/release-gpui/create-deterministic-tar.sh") $ComponentStage $ComponentAsset --windows-component
    if ($LASTEXITCODE -ne 0) { throw "Could not create the deterministic Windows code-server component asset" }
    Remove-Item -Recurse -Force $ComponentStage
    if ($NativeCodeOnDemand) {
        # The sealer below reads every code-server-<version>-<platform>.tar.gz in the asset folder, so the native editor joins the same component entry.
        $NativeArchiveName = "code-server-$ComponentVersion-windows-native-$ReleaseArch.tar.gz"
        if ((Split-Path -Leaf $NativeCodeServerArchive) -ne $NativeArchiveName) {
            throw "Native Windows editor archive identity mismatch: expected $NativeArchiveName"
        }
        & node (Join-Path $RepoRoot "tooling/release-gpui/verify-windows-native-code-server-archive.mjs") `
            --archive $NativeCodeServerArchive `
            --version $ComponentVersion `
            --platform "windows-native-$ReleaseArch"
        if ($LASTEXITCODE -ne 0) { throw "Native Windows editor archive verification failed" }
        Copy-Item -LiteralPath $NativeCodeServerArchive, "$NativeCodeServerArchive.sha256" -Destination $ComponentAssetDir
    }
    $ComponentAssetSha = (Get-FileHash -Algorithm SHA256 $ComponentAsset).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText(
        "$ComponentAsset.sha256",
        "$ComponentAssetSha  $(Split-Path -Leaf $ComponentAsset)`n",
        [Text.UTF8Encoding]::new($false)
    )
    & node (Join-Path $RepoRoot "tooling/release-gpui/publish-component.mjs") `
        --metadata-only `
        --require-published-match `
        --component code-server `
        --version $ComponentVersion `
        --platform "windows-$ReleaseArch" `
        --asset-dir $ComponentAssetDir `
        --require-sha256-sidecars `
        --output $ComponentManifest
    if ($LASTEXITCODE -ne 0) { throw "Could not seal Windows code-server component metadata" }

}
elseif ($RequireSourceArchive) {
    throw "Required WSL Source archive is missing: $WslCodeServerArchive"
}

if ($OnDemandComponents -or ($WslCodeServerArchive -and (Test-Path $WslCodeServerArchive))) {
    $OnDemandBuildManifest = Join-Path $ComponentRoot "windows-$ReleaseArch-assets.json"
    $OnDemandBuildPayload = [ordered]@{ assets = $WslOnDemandAssets; version = $ReleaseVersion } |
        ConvertTo-Json -Depth 4
    [IO.File]::WriteAllText(
        $OnDemandBuildManifest,
        "$OnDemandBuildPayload`n",
        [Text.UTF8Encoding]::new($false)
    )
    $ResourcesDir = Join-Path $AppDir "resources"
    New-Item -ItemType Directory -Force -Path $ResourcesDir | Out-Null
    & node (Join-Path $RepoRoot "tooling/release-gpui/on-demand-manifest.mjs") seal `
        --build-manifest $OnDemandBuildManifest `
        --component-manifest $ComponentManifest `
        --output (Join-Path $ResourcesDir "on-demand-resources.json") `
        --repo "maddada/Ghostex"
    if ($LASTEXITCODE -ne 0) { throw "Could not seal the Windows on-demand manifest" }
}

# A build without the published native editor archive packages the editor beside the desktop runtime; the WSL editor remains a separate Linux component.
if (-not $NativeCodeOnDemand) {
    $NativeCodeRoot = Join-Path $GpuiDir "build/native-code-server"
    if (!(Test-Path (Join-Path $NativeCodeRoot "lib/node.exe")) -or
        !(Test-Path (Join-Path $NativeCodeRoot "lib/vscode/out/server-main.js"))) {
        throw "The native Windows editor build did not produce the required staging payload."
    }
    Copy-Item $NativeCodeRoot (Join-Path $AppDir "code-server") -Recurse -Force
    # CDXC:Release 2026-09-16 WHY:
    # The native editor payload is an immutable, version-free component reused across releases, so the app version and commit are stamped on the staged copy here, not at build time.
    # A reused payload therefore reports the current release exactly like a freshly built one. The on-demand copy keeps code-server's own version, like the macOS and WSL editors.
    $StagedPackagePath = Join-Path $AppDir "code-server/package.json"
    $StagedPackage = Get-Content $StagedPackagePath -Raw | ConvertFrom-Json
    $StagedPackage.version = $ReleaseVersion
    # The wrapper reports the code-server revision it was built from; the gitlink resolves even when the submodule is not initialized.
    $StagedPackage | Add-Member -NotePropertyName commit -NotePropertyValue ((& git -C $RepoRoot rev-parse "HEAD:.dependencies/code-server").Trim()) -Force
    [IO.File]::WriteAllText($StagedPackagePath, ($StagedPackage | ConvertTo-Json -Depth 100), [Text.UTF8Encoding]::new($false))
}
Write-Host "Staged $AppDir"

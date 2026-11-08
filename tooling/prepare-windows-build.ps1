param([switch]$Install)

$ErrorActionPreference = 'Stop'
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$Arch = if ($env:GHOSTEX_WINDOWS_ARCH) { $env:GHOSTEX_WINDOWS_ARCH } elseif ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
if ($Arch -notin @('x64', 'arm64')) { throw "Unsupported Windows build architecture: $Arch" }

$Missing = [System.Collections.Generic.List[string]]::new()
foreach ($Tool in @('git', 'bun', 'node', 'rustup')) {
    if (!(Get-Command $Tool -ErrorAction SilentlyContinue)) { $Missing.Add("Install $Tool and open a new PowerShell window.") }
}

$VsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$VsRoot = if (Test-Path -LiteralPath $VsWhere) {
    & $VsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (!$VsRoot) {
    $Missing.Add('Install Visual Studio Build Tools with Desktop development with C++, a Windows SDK, and CMake tools for Windows.')
}
if ($Missing.Count) { throw ($Missing -join "`n") }

# CDXC:Build 2026-11-04: the crates' .cargo/config.toml name kache as the rustc wrapper; in CI, kache-action exports RUSTC_WRAPPER as the kache executable's path. Require whichever wrapper this build will run.
$RustcWrapper = if ($null -ne $env:RUSTC_WRAPPER) { $env:RUSTC_WRAPPER } else { 'kache' }
if ($RustcWrapper -eq 'kache' -and !(Get-Command kache -ErrorAction SilentlyContinue) -and $Install) {
    if (!(Get-Command winget -ErrorAction SilentlyContinue)) { throw 'Install kache with cargo install kache --locked, then run cargo xtask setup-windows again.' }
    & winget install --id kunobi-ninja.kache --exact --source winget --scope user --accept-package-agreements --accept-source-agreements --disable-interactivity
    if ($LASTEXITCODE -ne 0) { throw 'kache installation failed.' }
    $env:PATH += ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
}
if ($RustcWrapper -and !(Get-Command $RustcWrapper -ErrorAction SilentlyContinue)) { $Missing.Add("$RustcWrapper (the rustc wrapper) is missing. Run cargo xtask setup-windows, then open a new PowerShell window.") }

$ToolchainFile = Join-Path $RepoRoot 'apps/desktop/rust-toolchain.toml'
$RustVersion = [regex]::Match((Get-Content -Raw $ToolchainFile), '(?m)^channel\s*=\s*"([^"]+)"').Groups[1].Value
if (!$RustVersion) { throw "Cannot read the Rust version in $ToolchainFile" }
$ServerRust = [regex]::Match((Get-Content -Raw (Join-Path $RepoRoot 'server/rust-toolchain.toml')), '(?m)^channel\s*=\s*"([^"]+)"').Groups[1].Value
if ($ServerRust -ne $RustVersion) { throw 'Desktop and server Rust toolchain pins must match.' }
$InstalledToolchains = & rustup toolchain list
if ($LASTEXITCODE -ne 0) { throw 'Cannot list the installed Rust toolchains.' }
if (!($InstalledToolchains -match "^$([regex]::Escape($RustVersion))-")) {
    & rustup toolchain install $RustVersion --profile minimal
    if ($LASTEXITCODE -ne 0) { throw "Rust $RustVersion installation failed." }
}

if ($Install -and !$env:GHOSTEX_ZIG) { & (Join-Path $PSScriptRoot 'release-gpui/prepare-zig.ps1') -Arch $Arch }
if (!$env:GHOSTEX_ZIG) {
    $CachedZig = Join-Path $RepoRoot 'build/toolchains/zig-0.17.1-x64/zig.exe'
    if (Test-Path -LiteralPath $CachedZig) { $env:GHOSTEX_ZIG = $CachedZig }
    elseif (Get-Command zig -ErrorAction SilentlyContinue) { $env:GHOSTEX_ZIG = (Get-Command zig).Source }
}
$ZigCommand = if ($env:GHOSTEX_ZIG) { Get-Command $env:GHOSTEX_ZIG -CommandType Application -ErrorAction SilentlyContinue }
if (!$ZigCommand) {
    $Missing.Add('Zig 0.17.1 is missing. Run cargo xtask setup-windows, or set GHOSTEX_ZIG to its executable.')
} elseif ((& $ZigCommand.Source version) -ne '0.17.1' -or $LASTEXITCODE -ne 0) {
    $Missing.Add('GHOSTEX_ZIG must point to Zig 0.17.1.')
} else {
    $env:GHOSTEX_ZIG = $ZigCommand.Source
}

# CDXC:Security 2026-11-08 WHY: Launch-VsDevShell re-reads the environment from VsDevCmd's `set` output, which keeps only the first line of a multi-line variable, so the PEM component signing key reached Node as its BEGIN line and sealing CEF failed with "DECODER routines::unsupported" (11.1.0's first Windows builds). Escaping its newlines keeps it on one line; component-signing.mjs turns `\n` back into newlines.
if ($env:GHOSTEX_COMPONENT_SIGNING_KEY -match "`n") {
    $env:GHOSTEX_COMPONENT_SIGNING_KEY = $env:GHOSTEX_COMPONENT_SIGNING_KEY -replace "`r?`n", '\n'
}

# Use the installed C++ toolchain from a normal PowerShell window too.
$VsArch = if ($Arch -eq 'arm64') { 'arm64' } else { 'amd64' }
& (Join-Path $VsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch $VsArch -HostArch amd64 -SkipAutomaticLocation
foreach ($Tool in @('cl', 'cmake', 'ninja', 'rc')) {
    if (!(Get-Command $Tool -ErrorAction SilentlyContinue)) { $Missing.Add("$Tool is missing from the Visual Studio build environment. Install the C++ workload, Windows SDK, and CMake tools.") }
}
if ($Missing.Count) { throw ("Windows build prerequisites are missing:`n" + ($Missing -join "`n")) }
Write-Host "Windows $Arch build tools are ready (Rust $RustVersion, Zig 0.17.1, $RustcWrapper, MSVC, CMake, Ninja)."

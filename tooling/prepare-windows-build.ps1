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

if (!(Get-Command sccache -ErrorAction SilentlyContinue) -and $Install) {
    if (!(Get-Command winget -ErrorAction SilentlyContinue)) { throw 'Install sccache with cargo install sccache --locked, then run bun run setup:windows again.' }
    & winget install --id Mozilla.sccache --exact --source winget --scope user --accept-package-agreements --accept-source-agreements --disable-interactivity
    if ($LASTEXITCODE -ne 0) { throw 'sccache installation failed.' }
    $env:PATH += ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
}
if (!(Get-Command sccache -ErrorAction SilentlyContinue)) { $Missing.Add('sccache is missing. Run bun run setup:windows, then open a new PowerShell window.') }

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
    $CachedZig = Join-Path $RepoRoot 'build/toolchains/zig-0.16.0-x64/zig.exe'
    if (Test-Path -LiteralPath $CachedZig) { $env:GHOSTEX_ZIG = $CachedZig }
    elseif (Get-Command zig -ErrorAction SilentlyContinue) { $env:GHOSTEX_ZIG = (Get-Command zig).Source }
}
$ZigCommand = if ($env:GHOSTEX_ZIG) { Get-Command $env:GHOSTEX_ZIG -CommandType Application -ErrorAction SilentlyContinue }
if (!$ZigCommand) {
    $Missing.Add('Zig 0.16.0 is missing. Run bun run setup:windows, or set GHOSTEX_ZIG to its executable.')
} elseif ((& $ZigCommand.Source version) -ne '0.16.0' -or $LASTEXITCODE -ne 0) {
    $Missing.Add('GHOSTEX_ZIG must point to Zig 0.16.0.')
} else {
    $env:GHOSTEX_ZIG = $ZigCommand.Source
}

# Use the installed C++ toolchain from a normal PowerShell window too.
$VsArch = if ($Arch -eq 'arm64') { 'arm64' } else { 'amd64' }
& (Join-Path $VsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch $VsArch -HostArch amd64 -SkipAutomaticLocation
foreach ($Tool in @('cl', 'cmake', 'ninja', 'rc')) {
    if (!(Get-Command $Tool -ErrorAction SilentlyContinue)) { $Missing.Add("$Tool is missing from the Visual Studio build environment. Install the C++ workload, Windows SDK, and CMake tools.") }
}
if ($Missing.Count) { throw ("Windows build prerequisites are missing:`n" + ($Missing -join "`n")) }
Write-Host "Windows $Arch build tools are ready (Rust $RustVersion, Zig 0.16.0, sccache, MSVC, CMake, Ninja)."

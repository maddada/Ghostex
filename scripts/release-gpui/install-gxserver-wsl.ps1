param(
    [string]$Distro = "",
    [string]$InstallRoot = ".ghostex/gxserver"
)

$ErrorActionPreference = "Stop"
$PackageRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$MetadataPath = Join-Path $PackageRoot "wsl-package.json"
if (-not (Test-Path $MetadataPath)) {
    throw "wsl-package.json is missing beside this installer"
}
$Metadata = Get-Content -Raw $MetadataPath | ConvertFrom-Json
$Archive = Join-Path $PackageRoot $Metadata.payload.name
if (-not (Test-Path $Archive)) {
    throw "WSL gxserver payload is missing: $Archive"
}
$ActualSha = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
if ($ActualSha -ne $Metadata.payload.sha256) {
    throw "WSL gxserver payload checksum mismatch"
}
if ($InstallRoot -notmatch '^[A-Za-z0-9._/-]+$' -or $InstallRoot.StartsWith("/") -or $InstallRoot.Split('/') -contains "..") {
    throw "InstallRoot must be a safe path relative to the WSL home directory"
}

$Wsl = Get-Command wsl.exe -ErrorAction SilentlyContinue
if (-not $Wsl) {
    throw "WSL is not installed. Install and initialize WSL2, then run this script again."
}
$Distros = @(& $Wsl.Source --list --quiet) | ForEach-Object { $_.Trim() } | Where-Object { $_ }
if ($LASTEXITCODE -ne 0 -or $Distros.Count -eq 0) {
    throw "WSL has no initialized Linux distribution"
}
if (-not $Distro) {
    $Distro = $Distros[0]
}
if ($Distros -notcontains $Distro) {
    throw "WSL distribution '$Distro' is not installed"
}

$Machine = (& $Wsl.Source -d $Distro -- uname -m).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "Could not inspect the architecture of WSL distribution '$Distro'"
}
$ExpectedMachines = if ($Metadata.targetArch -eq "arm64") { @("aarch64", "arm64") } else { @("x86_64", "amd64") }
if ($ExpectedMachines -notcontains $Machine) {
    throw "Package architecture $($Metadata.targetArch) does not match WSL architecture $Machine"
}

$WslArchive = (& $Wsl.Source -d $Distro -- wslpath -a $Archive).Trim()
if ($LASTEXITCODE -ne 0 -or -not $WslArchive) {
    throw "Could not translate the package path into WSL"
}
& $Wsl.Source -d $Distro -- sh -lc 'set -eu; install_root="$HOME/$1"; release_dir="$install_root/releases/$2-$3"; archive="$4"; rm -rf "$release_dir"; mkdir -p "$release_dir"; tar -xzf "$archive" -C "$release_dir"; "$release_dir/bin/gxserver" setup --install-root "$install_root" --release-dir "$release_dir"' sh $InstallRoot $Metadata.version $Metadata.payload.sha256.Substring(0, 12) $WslArchive
if ($LASTEXITCODE -ne 0) {
    throw "gxserver setup failed inside WSL distribution '$Distro'"
}

Write-Host "Installed Ghostex gxserver $($Metadata.version) for $($Metadata.targetArch) in WSL distribution '$Distro'."

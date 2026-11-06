# Ghostex installer for Windows: https://ghostex.dev
#
#   irm https://ghostex.dev/install.ps1 | iex
#
# Read it first:  irm https://ghostex.dev/install.ps1
# Source:         https://github.com/maddada/Ghostex/blob/main/tooling/install/install.ps1
#
# What it does:
#   1. Asks GitHub for the latest Ghostex release.
#   2. Picks the x64 or ARM64 installer for this PC.
#   3. Downloads it from GitHub into a new temporary folder.
#   4. Checks its SHA-256 against the digest GitHub publishes for that file,
#      and refuses to run it if they differ or GitHub gives none.
#   5. Runs the installer (per user, no admin), then deletes the download.
#
# It changes no security settings: no Defender changes, no execution policy
# changes, nothing turned off.
#
# Test without installing (downloads and checks, then stops):
#   $env:GHOSTEX_INSTALL_DRY_RUN = '1'; irm https://ghostex.dev/install.ps1 | iex
# Pick the installer yourself: $env:GHOSTEX_INSTALL_ARCH = 'x64' (or 'arm64')

& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'  # The progress bar makes downloads very slow in Windows PowerShell 5.1.
    $repo = 'maddada/Ghostex'
    $dryRun = $env:GHOSTEX_INSTALL_DRY_RUN -in @('1', 'true', 'yes')

    if ($env:OS -ne 'Windows_NT' -and -not $dryRun) {
        throw 'This installer is for Windows. Other platforms: https://ghostex.dev'
    }

    # Windows PowerShell 5.1 can default to old TLS versions that GitHub refuses.
    if ($PSVersionTable.PSVersion.Major -lt 6) {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    }

    # PROCESSOR_ARCHITECTURE alone says AMD64 inside an x64 PowerShell on an ARM64 PC, so ask the OS too.
    $arch = $env:GHOSTEX_INSTALL_ARCH
    if (-not $arch) {
        $arm64 = "$env:PROCESSOR_ARCHITECTURE $env:PROCESSOR_ARCHITEW6432" -match 'ARM64'
        try { $arm64 = $arm64 -or ("$([Runtime.InteropServices.RuntimeInformation]::OSArchitecture)" -eq 'Arm64') } catch {}
        $arch = if ($arm64) { 'arm64' } else { 'x64' }
    }
    if ($arch -notin @('x64', 'arm64')) { throw "GHOSTEX_INSTALL_ARCH must be x64 or arm64, not '$arch'." }

    Write-Host ''
    Write-Host '  Ghostex installer'
    Write-Host ''

    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing
    } catch {
        throw "Could not reach GitHub to find the latest release ($($_.Exception.Message)). Download it from https://github.com/$repo/releases/latest"
    }
    $version = "$($release.tag_name)" -replace '^v', ''
    $name = "ghostex-$version-windows-$arch.exe"
    $asset = @($release.assets | Where-Object { $_.name -eq $name })
    if ($asset.Count -ne 1) { throw "The latest release ($($release.tag_name)) has no $name." }
    $asset = $asset[0]

    # GitHub computes this digest itself when the file is uploaded.
    $expected = "$($asset.digest)".ToLowerInvariant()
    if ($expected -notmatch '^sha256:[0-9a-f]{64}$') {
        throw "GitHub published no SHA-256 digest for $name, so it can't be checked. Not installing."
    }
    $expected = $expected.Substring(7)

    $folder = Join-Path ([IO.Path]::GetTempPath()) "ghostex-install-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $folder | Out-Null
    try {
        $installer = Join-Path $folder $name
        Write-Host "  Version:  $version ($arch)"
        Write-Host "  Downloading $name ($([math]::Round($asset.size / 1MB)) MB)..."
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $installer -UseBasicParsing

        $actual = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            Remove-Item -LiteralPath $installer -Force
            throw "Checksum mismatch for $name (expected $expected, got $actual). Deleted it; not installing."
        }
        Write-Host '  SHA-256 matches the digest GitHub published.'

        if ($dryRun) {
            Write-Host '  Dry run: not running the installer.'
            return
        }

        Write-Host '  Running the installer...'
        $process = Start-Process -FilePath $installer -PassThru
        $null = $process.Handle  # Keeps the exit code readable after the process ends.
        # Wait for the installer only: Start-Process -Wait would also wait for Ghostex, which the installer starts.
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw "The installer exited with code $($process.ExitCode)." }
        Write-Host '  Ghostex is installed. It updates itself from now on.'
    } finally {
        Remove-Item -LiteralPath $folder -Recurse -Force -ErrorAction SilentlyContinue
    }
    Write-Host ''
}

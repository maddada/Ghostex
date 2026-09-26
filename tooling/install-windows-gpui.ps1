param(
    [Parameter(Mandatory = $true)]
    [string]$StagedAppPath,

    [switch]$Elevated
)

$ErrorActionPreference = "Stop"

function Test-IsAdministrator {
    $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $Principal = [Security.Principal.WindowsPrincipal]::new($Identity)
    return $Principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

$StagedAppPath = [IO.Path]::GetFullPath($StagedAppPath)
$StagedExecutable = Join-Path $StagedAppPath "Ghostex.exe"
if (-not (Test-Path -LiteralPath $StagedExecutable -PathType Leaf)) {
    throw "The staged Ghostex executable is missing: $StagedExecutable"
}

if (-not (Test-IsAdministrator)) {
    if ($Elevated) {
        throw "Ghostex installation requires administrator access."
    }

    $Arguments = @(
        "-NoProfile"
        "-ExecutionPolicy"
        "Bypass"
        "-File"
        "`"$PSCommandPath`""
        "-StagedAppPath"
        "`"$StagedAppPath`""
        "-Elevated"
    )
    $Installer = Start-Process `
        -FilePath "powershell.exe" `
        -Verb RunAs `
        -ArgumentList $Arguments `
        -Wait `
        -PassThru
    if ($Installer.ExitCode -ne 0) {
        throw "The elevated Ghostex installer failed with exit code $($Installer.ExitCode)."
    }
    exit 0
}

$ProgramFiles = $env:ProgramW6432
if (-not $ProgramFiles) {
    $ProgramFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
}
if (-not $ProgramFiles) {
    throw "Windows did not report its Program Files directory."
}
$InstallDir = Join-Path $ProgramFiles "Ghostex"
$InstalledExecutable = Join-Path $InstallDir "Ghostex.exe"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
<#
CDXC:Build 2026-09-24 WHY:
Remote Code clients can keep the bundled Node runtime and native editor modules mapped after the desktop closes. Close only processes using this installation's editor executable before mirroring its payload; terminal daemons have a separate lifecycle.
#>
$InstalledEditorExecutable = Join-Path $InstallDir "code-server/lib/node.exe"
$EditorProcesses = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and [string]::Equals($_.Path, $InstalledEditorExecutable, [StringComparison]::OrdinalIgnoreCase)
})
foreach ($EditorProcess in $EditorProcesses) {
    Stop-Process -InputObject $EditorProcess -Force -ErrorAction SilentlyContinue
}
foreach ($EditorProcess in $EditorProcesses) {
    if (-not $EditorProcess.WaitForExit(10000)) {
        throw "The bundled Code editor process $($EditorProcess.Id) did not exit before installation."
    }
}
<#
CDXC:Build 2026-09-23 WHY:
The server removes its HTTP endpoint before its workers finish shutting down, and its mapped image can outlive process-path discovery. Retire a changed image outside the mirror, as with the persistent session provider, so installation does not depend on worker exit or race a reconnecting client. Keep an identical server executable out of the mirror.
#>
$RetiredNativeDir = Join-Path $InstallDir ".retired-native"
$InstalledServer = Join-Path $InstallDir "resources/native/gxserver.exe"
$StagedServer = Join-Path $StagedAppPath "resources/native/gxserver.exe"
$KeepInstalledServer = (Test-Path -LiteralPath $InstalledServer -PathType Leaf) -and
    (Test-Path -LiteralPath $StagedServer -PathType Leaf) -and
    ((Get-FileHash -LiteralPath $InstalledServer -Algorithm SHA256).Hash -eq
        (Get-FileHash -LiteralPath $StagedServer -Algorithm SHA256).Hash)
if (-not $KeepInstalledServer -and (Test-Path -LiteralPath $InstalledServer -PathType Leaf)) {
    $RetiredServerDir = Join-Path $RetiredNativeDir ([Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $RetiredServerDir | Out-Null
    Move-Item -LiteralPath $InstalledServer -Destination (Join-Path $RetiredServerDir "gxserver.exe")
}
$StagedHash = (Get-FileHash -LiteralPath $StagedExecutable -Algorithm SHA256).Hash
<#
CDXC:Build 2026-09-22 WHY:
Persistent Windows sessions keep wmx.exe mapped after the app closes. Windows permits moving that image but cannot overwrite it; retain it outside the mirrored payload so installing a compatible provider preserves live sessions.
#>
$StagedWmx = Join-Path $StagedAppPath "resources/native/wmx.exe"
$InstalledWmx = Join-Path $InstallDir "resources/native/wmx.exe"
if ((Test-Path -LiteralPath $StagedWmx -PathType Leaf) -and (Test-Path -LiteralPath $InstalledWmx -PathType Leaf)) {
    $StagedWmxHash = (Get-FileHash -LiteralPath $StagedWmx -Algorithm SHA256).Hash
    $InstalledWmxHash = (Get-FileHash -LiteralPath $InstalledWmx -Algorithm SHA256).Hash
    if ($StagedWmxHash -ne $InstalledWmxHash) {
        $RetiredVersionDir = Join-Path $RetiredNativeDir ([Guid]::NewGuid().ToString("N"))
        New-Item -ItemType Directory -Path $RetiredVersionDir | Out-Null
        Move-Item -LiteralPath $InstalledWmx -Destination (Join-Path $RetiredVersionDir "wmx.exe")
    }
}
# Exclude both trees when a staged app also contains retained runtime images.
$MirrorExclusions = @('/XD', '.retired-native')
if ($KeepInstalledServer) {
    $MirrorExclusions += @('/XF', $StagedServer)
}
& robocopy.exe $StagedAppPath $InstallDir /MIR /COPY:DAT /DCOPY:DAT /R:2 /W:1 /NFL /NDL /NJH /NJS /NP @MirrorExclusions
$RobocopyExitCode = $LASTEXITCODE
if ($RobocopyExitCode -gt 7) {
    throw "Installing Ghostex into $InstallDir failed with robocopy exit code $RobocopyExitCode."
}
if (-not (Test-Path -LiteralPath $InstalledExecutable -PathType Leaf)) {
    throw "The installed Ghostex executable is missing: $InstalledExecutable"
}

<#
CDXC:Build 2026-09-20 DECISION:
User: a rebuild must always replace the executable in Program Files, so an install may not report success while
the installed app is still the previous binary. Existence alone cannot tell a fresh copy from one that a
locked file or a skipped mirror entry left behind, so compare the installed executable with the staged one
and name whatever still holds it open.
#>
$InstalledHash = (Get-FileHash -LiteralPath $InstalledExecutable -Algorithm SHA256).Hash
if ($InstalledHash -ne $StagedHash) {
    $Holders = @(
        Get-Process -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $InstalledExecutable } |
            ForEach-Object { "$($_.ProcessName) (pid $($_.Id))" }
    )
    $Detail = if ($Holders.Count -gt 0) { " Still holding it open: $($Holders -join ', ')." } else { "" }
    throw "$InstalledExecutable still has the previous build after installing (staged $StagedHash, installed $InstalledHash).$Detail"
}
if (Test-Path -LiteralPath $StagedServer -PathType Leaf) {
    if ((Get-FileHash -LiteralPath $InstalledServer -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $StagedServer -Algorithm SHA256).Hash) {
        throw "The installed gxserver does not match the rebuilt binary."
    }
}
if (Test-Path -LiteralPath $StagedWmx -PathType Leaf) {
    if ((Get-FileHash -LiteralPath $InstalledWmx -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $StagedWmx -Algorithm SHA256).Hash) {
        throw "The installed Windows session provider does not match the rebuilt binary."
    }
}

$ProgramsDir = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonPrograms)
if (-not $ProgramsDir) {
    throw "Windows did not report its all-users Start Menu directory."
}
$ShortcutDir = Join-Path $ProgramsDir "Ghostex"
$ShortcutPath = Join-Path $ShortcutDir "Ghostex.lnk"
New-Item -ItemType Directory -Force -Path $ShortcutDir | Out-Null

$Shell = New-Object -ComObject WScript.Shell
$Shortcut = $Shell.CreateShortcut($ShortcutPath)
$Shortcut.TargetPath = $InstalledExecutable
$Shortcut.WorkingDirectory = $InstallDir
$Shortcut.Description = "Ghostex"
$Shortcut.IconLocation = "$InstalledExecutable,0"
$Shortcut.Save()

Write-Host "Installed Ghostex to $InstallDir (Ghostex.exe verified as the rebuilt binary, SHA256 $($StagedHash.Substring(0, 12)))"
Write-Host "Created Start Menu shortcut at $ShortcutPath"
exit 0

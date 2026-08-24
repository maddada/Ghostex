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
& robocopy.exe $StagedAppPath $InstallDir /MIR /COPY:DAT /DCOPY:DAT /R:2 /W:1 /NFL /NDL /NJH /NJS /NP
$RobocopyExitCode = $LASTEXITCODE
if ($RobocopyExitCode -gt 7) {
    throw "Installing Ghostex into $InstallDir failed with robocopy exit code $RobocopyExitCode."
}
if (-not (Test-Path -LiteralPath $InstalledExecutable -PathType Leaf)) {
    throw "The installed Ghostex executable is missing: $InstalledExecutable"
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

Write-Host "Installed Ghostex to $InstallDir"
Write-Host "Created Start Menu shortcut at $ShortcutPath"
exit 0

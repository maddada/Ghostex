# CDXC:Release 2026-11-09 WHY:
# Ghostex's Windows code-signing certificate is Certum "Open Source Code Signing
# in the Cloud": the private key never leaves Certum's SimplySign cloud, and the
# only supported way to use it from signtool is SimplySign Desktop, which mounts
# the cloud key as a virtual smart card once you log in with the account e-mail
# and a one-time code from the SimplySign mobile app. Certum offers no CLI or
# API login for this product, so the release logs in the way other projects do
# on GitHub's Windows runners (jay0lee/certum-cloud-code-sign, Mumcord PR #343):
# compute the one-time code from the TOTP seed in the SimplySign activation QR
# code (RFC 6238, HMAC-SHA256, 6 digits, 30 seconds), type it into SimplySign
# Desktop's login window, and wait for the certificate to appear in
# Cert:\CurrentUser\My. Our own script rather than a third-party action, so no
# outside code ever sees the seed.
#
#   -Install  download SimplySign Desktop (pinned version and SHA-256, plus a
#             check that Asseco signed the MSI) and install it silently.
#   -Login    log in (at most two attempts, so a wrong seed cannot lock the
#             account) and export GHOSTEX_SIGNTOOL (the x64 signtool, which also
#             runs on ARM64: the ARM64 signtool cannot load SimplySign's x64
#             smart-card driver).
#   -Logout   stop SimplySign Desktop (an always() step at the end of the job).
#
# Inputs (environment): CERTUM_SIMPLYSIGN_EMAIL, CERTUM_SIMPLYSIGN_TOTP (the
# full otpauth:// URI from the activation QR code, or its bare base32 secret),
# GHOSTEX_CERTUM_CERT_SHA1 (the certificate's SHA-1 thumbprint),
# GHOSTEX_CERTUM_LOGIN_SLOT (optional: "even" or "odd"; see Wait-LoginWindow).
# Nothing here ever prints the seed or a code; both are masked as well.
param(
    [switch]$Install,
    [switch]$Login,
    [switch]$Logout,
    [switch]$SelfTest
)
$ErrorActionPreference = "Stop"

$SimplySignVersion = "9.4.5.95"
$SimplySignMsiSha256 = "6309e8b46f1f4e786288e777aef515a2c30d885165b71a21191e9941badebd6b"
$SimplySignMsiUrl = "https://files.certum.eu/software/SimplySignDesktop/Windows/$SimplySignVersion/SimplySignDesktop-$SimplySignVersion-64-bit-en.msi"
$SimplySignExe = "C:\Program Files\Certum\SimplySign Desktop\SimplySignDesktop.exe"

function Get-Base32Bytes([string]$Text) {
    $Alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
    $Clean = ($Text -replace '[\s=-]', '').ToUpperInvariant()
    if (-not $Clean) { throw "The TOTP secret is empty" }
    $Bytes = [System.Collections.Generic.List[byte]]::new()
    $Buffer = 0
    $Bits = 0
    foreach ($Char in $Clean.ToCharArray()) {
        $Index = $Alphabet.IndexOf($Char)
        if ($Index -lt 0) { throw "The TOTP secret is not valid base32" }
        $Buffer = (($Buffer -shl 5) -bor $Index) -band 0xFFFF
        $Bits += 5
        if ($Bits -ge 8) {
            $Bits -= 8
            $Bytes.Add([byte](($Buffer -shr $Bits) -band 0xFF))
        }
    }
    return , $Bytes.ToArray()
}

# Reads the secret and its parameters. A bare base32 secret gets SimplySign's
# parameters; an otpauth:// URI states its own, so prefer storing the URI.
function Get-TotpSettings([string]$Value) {
    $Settings = @{ Algorithm = "SHA256"; Digits = 6; Period = 30; Secret = $Value.Trim() }
    if ($Settings.Secret -like "otpauth://*") {
        $Query = ([Uri]$Settings.Secret).Query.TrimStart("?")
        $Settings.Secret = $null
        foreach ($Pair in $Query -split "&") {
            $Key, $Raw = $Pair -split "=", 2
            $Decoded = [Uri]::UnescapeDataString($Raw)
            switch ($Key.ToLowerInvariant()) {
                "secret" { $Settings.Secret = $Decoded }
                "algorithm" { $Settings.Algorithm = $Decoded.ToUpperInvariant() -replace '-', '' }
                "digits" { $Settings.Digits = [int]$Decoded }
                "period" { $Settings.Period = [int]$Decoded }
            }
        }
        if (-not $Settings.Secret) { throw "The otpauth URI has no secret parameter" }
    }
    if ($Settings.Algorithm -notin @("SHA1", "SHA256", "SHA512")) { throw "Unsupported TOTP algorithm $($Settings.Algorithm)" }
    return $Settings
}

function Get-TotpCode([byte[]]$Key, [string]$Algorithm, [int]$Digits, [long]$Counter) {
    $Hmac = switch ($Algorithm) {
        "SHA1" { [Security.Cryptography.HMACSHA1]::new($Key) }
        "SHA256" { [Security.Cryptography.HMACSHA256]::new($Key) }
        "SHA512" { [Security.Cryptography.HMACSHA512]::new($Key) }
    }
    try {
        $CounterBytes = [BitConverter]::GetBytes([long]$Counter)
        if ([BitConverter]::IsLittleEndian) { [Array]::Reverse($CounterBytes) }
        $Hash = $Hmac.ComputeHash($CounterBytes)
    }
    finally { $Hmac.Dispose() }
    $Offset = $Hash[$Hash.Length - 1] -band 0x0F
    $Binary = (([int]($Hash[$Offset] -band 0x7F)) -shl 24) -bor ([int]$Hash[$Offset + 1] -shl 16) -bor ([int]$Hash[$Offset + 2] -shl 8) -bor [int]$Hash[$Offset + 3]
    return ($Binary % [int][Math]::Pow(10, $Digits)).ToString().PadLeft($Digits, "0")
}

# RFC 6238 appendix B test vectors (8 digits), so a broken TOTP implementation
# fails here instead of spending one of the two login attempts.
function Test-Totp {
    $Vectors = @(
        @{ Algorithm = "SHA1"; Key = "12345678901234567890"; Time = 59; Code = "94287082" },
        @{ Algorithm = "SHA256"; Key = "12345678901234567890123456789012"; Time = 59; Code = "46119246" },
        @{ Algorithm = "SHA256"; Key = "12345678901234567890123456789012"; Time = 1111111109; Code = "68084774" },
        @{ Algorithm = "SHA512"; Key = "1234567890123456789012345678901234567890123456789012345678901234"; Time = 59; Code = "90693936" }
    )
    foreach ($Vector in $Vectors) {
        $Code = Get-TotpCode -Key ([Text.Encoding]::ASCII.GetBytes($Vector.Key)) -Algorithm $Vector.Algorithm -Digits 8 -Counter ([long][Math]::Floor($Vector.Time / 30))
        if ($Code -ne $Vector.Code) { throw "TOTP self-test failed for $($Vector.Algorithm) at $($Vector.Time)" }
    }
    $Base32 = Get-Base32Bytes "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
    if ([Text.Encoding]::ASCII.GetString($Base32) -ne "12345678901234567890") { throw "Base32 self-test failed" }
    Write-Host "TOTP self-test passed (RFC 6238 SHA-1, SHA-256 and SHA-512 vectors, base32)."
}

function Add-Mask([string]$Value) {
    if ($Value -and $env:GITHUB_ACTIONS) { Write-Host "::add-mask::$Value" }
}

function Install-SimplySign {
    if (Test-Path -LiteralPath $SimplySignExe) {
        Write-Host "SimplySign Desktop is already installed."
        return
    }
    $Dir = Join-Path ($env:RUNNER_TEMP ?? [IO.Path]::GetTempPath()) "simplysign"
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    $Msi = Join-Path $Dir "SimplySignDesktop-$SimplySignVersion.msi"
    for ($Attempt = 1; $Attempt -le 3; $Attempt++) {
        try {
            Invoke-WebRequest -Uri $SimplySignMsiUrl -OutFile $Msi -UseBasicParsing
            break
        }
        catch {
            if ($Attempt -eq 3) { throw "Downloading SimplySign Desktop $SimplySignVersion failed: $($_.Exception.Message)" }
            Write-Host "Download attempt $Attempt failed; retrying."
            Start-Sleep -Seconds (5 * $Attempt)
        }
    }
    $Sha = (Get-FileHash -Algorithm SHA256 -LiteralPath $Msi).Hash.ToLowerInvariant()
    if ($Sha -ne $SimplySignMsiSha256) {
        throw "SimplySign Desktop $SimplySignVersion has SHA-256 $Sha, expected $SimplySignMsiSha256 (Certum replaced the file; check it and update the pin)"
    }
    $MsiSignature = Get-AuthenticodeSignature -LiteralPath $Msi
    if ($MsiSignature.Status -ne "Valid" -or $MsiSignature.SignerCertificate.Subject -notmatch "Asseco") {
        throw "The SimplySign Desktop MSI is not validly signed by Asseco (status $($MsiSignature.Status), signer $($MsiSignature.SignerCertificate.Subject))"
    }
    $Log = Join-Path $Dir "install.log"
    $Process = Start-Process -FilePath msiexec.exe -ArgumentList @("/i", "`"$Msi`"", "/qn", "/norestart", "/l*", "`"$Log`"") -Wait -PassThru -NoNewWindow
    if ($Process.ExitCode -notin @(0, 3010)) {
        if (Test-Path $Log) { Get-Content $Log -Tail 40 | ForEach-Object { Write-Host $_ } }
        throw "Installing SimplySign Desktop failed with msiexec exit code $($Process.ExitCode)"
    }
    if (-not (Test-Path -LiteralPath $SimplySignExe)) { throw "SimplySign Desktop is not at $SimplySignExe after installing" }
    Remove-Item -LiteralPath $Msi -Force
    Write-Host "Installed SimplySign Desktop $SimplySignVersion."
}

# Loaded only for -Login, so -SelfTest also runs off Windows.
function Import-DesktopAutomation {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class GhostexWin32 {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
}
"@
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
}

# Diagnostics for a login that does not mount the certificate. Window titles,
# the focused window and the SimplySign window's static text (labels, error
# messages; edit fields are skipped, so a typed code is never read) go to the
# log; a screenshot is taken only before anything is typed.
$DiagnosticsDir = Join-Path ($env:RUNNER_TEMP ?? [IO.Path]::GetTempPath()) "certum-diagnostics"

function Write-WindowState([string]$Label) {
    $Foreground = [GhostexWin32]::GetForegroundWindow()
    $ForegroundPid = [uint32]0
    [GhostexWin32]::GetWindowThreadProcessId($Foreground, [ref]$ForegroundPid) | Out-Null
    $ForegroundProcess = Get-Process -Id $ForegroundPid -ErrorAction SilentlyContinue
    Write-Host "[$Label] focused window: $($ForegroundProcess.ProcessName) '$($ForegroundProcess.MainWindowTitle)'"
    Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | ForEach-Object {
        Write-Host "[$Label] window: $($_.ProcessName) '$($_.MainWindowTitle)'"
    }
}

function Write-SimplySignText([string]$Label) {
    foreach ($Process in @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -like "*SimplySign*" -and $_.MainWindowHandle -ne 0 })) {
        try {
            $Root = [System.Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
            $Names = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) |
                Where-Object { $_.Current.ControlType -ne [System.Windows.Automation.ControlType]::Edit -and $_.Current.Name } |
                ForEach-Object { $_.Current.Name } | Select-Object -Unique -First 25
            Write-Host "[$Label] SimplySign window '$($Process.MainWindowTitle)' shows: $($Names -join ' | ')"
        }
        catch { Write-Host "[$Label] could not read the SimplySign window: $($_.Exception.Message)" }
    }
}

function Save-Screenshot([string]$Name) {
    try {
        New-Item -ItemType Directory -Force -Path $DiagnosticsDir | Out-Null
        $Bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $Bitmap = [System.Drawing.Bitmap]::new($Bounds.Width, $Bounds.Height)
        $Graphics = [System.Drawing.Graphics]::FromImage($Bitmap)
        $Graphics.CopyFromScreen($Bounds.Left, $Bounds.Top, 0, 0, $Bitmap.Size)
        $Bitmap.Save((Join-Path $DiagnosticsDir "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
        $Graphics.Dispose(); $Bitmap.Dispose()
    }
    catch { Write-Host "Screenshot $Name failed: $($_.Exception.Message)" }
}

function Get-SimplySignWindow {
    Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.MainWindowHandle -ne 0 -and ($_.MainWindowTitle -like "*SimplySign*" -or $_.ProcessName -like "*SimplySign*")
    } | Select-Object -First 1
}

function Enter-SimplySignWindow {
    $Window = Get-SimplySignWindow
    if (-not $Window) { return $false }
    [GhostexWin32]::ShowWindow($Window.MainWindowHandle, 9) | Out-Null
    # An Alt press lets this process take the foreground (Windows refuses
    # SetForegroundWindow to a background process otherwise).
    [GhostexWin32]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [GhostexWin32]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    [GhostexWin32]::SetForegroundWindow($Window.MainWindowHandle) | Out-Null
    (New-Object -ComObject WScript.Shell).AppActivate($Window.Id) | Out-Null
    Start-Sleep -Milliseconds 500
    return $true
}

function Send-Text([string]$Text) {
    # SendKeys treats + ^ % ~ ( ) { } [ ] as commands.
    [System.Windows.Forms.SendKeys]::SendWait(($Text -replace '([+^%~(){}\[\]])', '{$1}'))
}

# Waits until the current 30-second code has at least 15 seconds left, so it
# cannot expire while it is typed and sent. A code is accepted only once, so
# the x64 and ARM64 jobs, which log in to the same account at about the same
# time, use alternate windows (GHOSTEX_CERTUM_LOGIN_SLOT even/odd) and can never
# type the same code.
function Wait-LoginWindow([int]$Period) {
    while ($true) {
        $Now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
        $Counter = [long][Math]::Floor($Now / $Period)
        $Remaining = $Period - ($Now % $Period)
        $SlotOk = switch ($env:GHOSTEX_CERTUM_LOGIN_SLOT) {
            "even" { $Counter % 2 -eq 0 }
            "odd" { $Counter % 2 -eq 1 }
            default { $true }
        }
        if ($SlotOk -and $Remaining -ge 15) { return $Counter }
        Start-Sleep -Seconds ([Math]::Max(1, $Remaining))
    }
}

function Find-Certificate([string]$Thumbprint) {
    Get-ChildItem Cert:\CurrentUser\My -ErrorAction SilentlyContinue |
        Where-Object { $_.Thumbprint -eq $Thumbprint -and $_.HasPrivateKey } |
        Select-Object -First 1
}

function Find-Signtool {
    $Candidates = @(Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue) |
        Sort-Object { [version]($_.Directory.Parent.Name -replace '[^0-9.]', '') } -Descending
    if (-not $Candidates) { throw "No x64 signtool.exe in the Windows 10/11 SDK" }
    return $Candidates[0].FullName
}

function Invoke-Login {
    $Email = $env:CERTUM_SIMPLYSIGN_EMAIL
    $Totp = $env:CERTUM_SIMPLYSIGN_TOTP
    $Thumbprint = ($env:GHOSTEX_CERTUM_CERT_SHA1 -replace '[^0-9A-Fa-f]', '').ToUpperInvariant()
    if (-not $Email -or -not $Totp) { throw "CERTUM_SIMPLYSIGN_EMAIL and CERTUM_SIMPLYSIGN_TOTP are required to log in to SimplySign" }
    if ($Thumbprint.Length -ne 40) { throw "GHOSTEX_CERTUM_CERT_SHA1 must be the certificate's 40-digit SHA-1 thumbprint" }
    Add-Mask $Email
    Add-Mask $Totp
    $Settings = Get-TotpSettings $Totp
    Add-Mask $Settings.Secret
    $Key = Get-Base32Bytes $Settings.Secret
    Test-Totp
    Import-DesktopAutomation

    # The ARM64 runner image (windows-11-vs2026-arm) starts with Windows'
    # first-run "Choose privacy settings for your device" screen covering the
    # desktop and holding the focus (process WWAHost): in test run 42722612240
    # the login was typed into it, and Tab x7 + Enter (certum-cloud-code-sign's
    # dismissal) did not clear it in 42730081540. Closing its process does; a
    # System Properties dialog and the WSL console can be up as well.
    Write-WindowState "before login"
    $FirstRun = @(Get-Process -Name WWAHost -ErrorAction SilentlyContinue)
    if ($FirstRun.Count -gt 0) {
        Save-Screenshot "desktop-at-start"
        $FirstRun | Stop-Process -Force -ErrorAction SilentlyContinue
        Write-Host "Closed the Windows first-run screen (WWAHost)."
    }
    Get-Process -Name SystemPropertiesAdvanced, SystemPropertiesPerformance -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Get-Process -Name wsl -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | ForEach-Object { $_.CloseMainWindow() | Out-Null }
    Start-Sleep -Seconds 2
    if ($FirstRun.Count -gt 0) {
        Save-Screenshot "desktop-after-closing-first-run"
        Write-WindowState "after closing the first-run screen"
    }
    (New-Object -ComObject Shell.Application).MinimizeAll()
    Start-Sleep -Seconds 1
    # The first start launches the tray service; the second shows the login
    # window. SimplySign is an x64 program, so on ARM64 it runs emulated and
    # starts more slowly: each start gets up to 20 seconds to show its window.
    for ($Launch = 1; $Launch -le 5 -and -not (Get-SimplySignWindow); $Launch++) {
        Start-Process -FilePath $SimplySignExe
        $Shown = [DateTime]::UtcNow.AddSeconds(20)
        while ([DateTime]::UtcNow -lt $Shown -and -not (Get-SimplySignWindow)) { Start-Sleep -Seconds 2 }
    }
    if (-not (Get-SimplySignWindow)) {
        Write-WindowState "SimplySign window missing"
        Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -like "*SimplySign*" -or $_.ProcessName -like "*Certum*" } | ForEach-Object {
            Write-Host "[SimplySign window missing] process: $($_.ProcessName) id=$($_.Id) window=$($_.MainWindowHandle) title='$($_.MainWindowTitle)' responding=$($_.Responding)"
        }
        Save-Screenshot "simplysign-window-missing"
        throw "SimplySign Desktop did not open its login window"
    }
    Write-SimplySignText "login window"

    $Certificate = $null
    for ($Attempt = 1; $Attempt -le 2 -and -not $Certificate; $Attempt++) {
        if ($Attempt -gt 1) {
            Write-Host "Login attempt 1 did not mount the certificate; trying once more with a new code."
            if (Enter-SimplySignWindow) {
                [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
                Start-Sleep -Milliseconds 500
                [System.Windows.Forms.SendKeys]::SendWait("{ESC}")
                Start-Sleep -Seconds 1
            }
            if (-not (Get-SimplySignWindow)) {
                Start-Process -FilePath $SimplySignExe
                Start-Sleep -Seconds 4
            }
        }
        $Counter = Wait-LoginWindow -Period $Settings.Period
        if (-not (Enter-SimplySignWindow)) { throw "The SimplySign Desktop login window disappeared" }
        Write-WindowState "attempt $Attempt, before typing"
        $FocusPid = [uint32]0
        [GhostexWin32]::GetWindowThreadProcessId([GhostexWin32]::GetForegroundWindow(), [ref]$FocusPid) | Out-Null
        if ((Get-Process -Id $FocusPid -ErrorAction SilentlyContinue).ProcessName -notlike "*SimplySign*") {
            if ($Attempt -eq 1) { Save-Screenshot "attempt-1-focus-elsewhere" }
            throw "SimplySign Desktop is not the focused window, so the login was not typed (see the window list above)"
        }
        # Only before the first attempt: later the fields still hold what was typed.
        if ($Attempt -eq 1) { Save-Screenshot "attempt-1-before-typing" }
        $Code = Get-TotpCode -Key $Key -Algorithm $Settings.Algorithm -Digits $Settings.Digits -Counter $Counter
        Add-Mask $Code
        [System.Windows.Forms.SendKeys]::SendWait("^a")
        Send-Text $Email
        [System.Windows.Forms.SendKeys]::SendWait("{TAB}")
        [System.Windows.Forms.SendKeys]::SendWait("^a")
        Send-Text $Code
        [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
        $Code = $null
        Write-Host "Submitted the SimplySign login (attempt $Attempt); waiting for the certificate."
        $Deadline = [DateTime]::UtcNow.AddSeconds(90)
        $Reported = $false
        while ([DateTime]::UtcNow -lt $Deadline -and -not ($Certificate = Find-Certificate $Thumbprint)) {
            Start-Sleep -Seconds 3
            if (-not $Reported -and [DateTime]::UtcNow -gt $Deadline.AddSeconds(-75)) {
                $Reported = $true
                Write-Host "[attempt $Attempt, 15 s after submitting] SimplySign login window $(if (Get-SimplySignWindow) { 'is still open' } else { 'has closed (login accepted)' })"
                Write-SimplySignText "attempt $Attempt, 15 s after submitting"
            }
        }
        if (-not $Certificate) {
            Write-WindowState "attempt $Attempt, no certificate"
            Write-SimplySignText "attempt $Attempt, no certificate"
            Get-PnpDevice -Class SmartCardReader, SmartCard -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "[attempt $Attempt] device: $($_.Class) '$($_.FriendlyName)' $($_.Status)" }
        }
    }
    [Array]::Clear($Key, 0, $Key.Length)
    if (-not $Certificate) {
        $Seen = @(Get-ChildItem Cert:\CurrentUser\My -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Thumbprint) $($_.Subject) key=$($_.HasPrivateKey)" })
        throw "SimplySign Desktop did not mount certificate $Thumbprint after two logins (current user store: $($Seen -join '; '))"
    }
    $Signtool = Find-Signtool
    Write-Host "SimplySign mounted $($Certificate.Subject) (valid until $($Certificate.NotAfter.ToString('u'))); signtool: $Signtool"
    if ($env:GITHUB_ENV) {
        Add-Content -LiteralPath $env:GITHUB_ENV -Value "GHOSTEX_SIGNTOOL=$Signtool"
    }
}

function Invoke-Logout {
    Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -like "*SimplySign*" } | ForEach-Object {
        $_.CloseMainWindow() | Out-Null
        Start-Sleep -Seconds 1
        Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Stopped SimplySign Desktop."
}

if ($SelfTest) { Test-Totp }
if ($Install) { Install-SimplySign }
if ($Login) { Invoke-Login }
if ($Logout) { Invoke-Logout }

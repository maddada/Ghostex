# CDXC:Release 2026-11-09 WHY:
# Authenticode-signs files with Ghostex's Certum cloud certificate through
# signtool, once certum-simplysign-login.ps1 has mounted it. windows.ps1 calls
# it for the staged app binaries before `vpk pack`, for Velopack's Setup.exe
# after it is rewritten, and for the customer-facing installer after bundling;
# `vpk pack --signTemplate` calls it with -VelopackHook for every PE file
# Velopack would sign (the Update.exe and launcher stub it generates).
#
# -VelopackHook skips the Setup.exe (windows.ps1 rewrites its package after
# `vpk pack`, which would break a signature made now, so it is signed after
# that) and files that already carry a signature (the app binaries signed
# before the pack, and third-party files signed by their vendors).
#
# Every file must come out with a Valid signature from the configured
# certificate and an RFC 3161 timestamp; anything else fails the release.
param(
    [Parameter(Mandatory = $true, Position = 0, ValueFromRemainingArguments = $true)][string[]]$Path,
    [switch]$VelopackHook
)
$ErrorActionPreference = "Stop"

$Signtool = $env:GHOSTEX_SIGNTOOL
$Thumbprint = ($env:GHOSTEX_CERTUM_CERT_SHA1 -replace '[^0-9A-Fa-f]', '').ToUpperInvariant()
$TimestampUrl = if ($env:GHOSTEX_CERTUM_TIMESTAMP_URL) { $env:GHOSTEX_CERTUM_TIMESTAMP_URL } else { "http://time.certum.pl" }
if (-not $Signtool -or -not (Test-Path -LiteralPath $Signtool)) {
    throw "GHOSTEX_SIGNTOOL is not set; run certum-simplysign-login.ps1 -Login first"
}
if ($Thumbprint.Length -ne 40) { throw "GHOSTEX_CERTUM_CERT_SHA1 must be the certificate's 40-digit SHA-1 thumbprint" }

$Files = @()
foreach ($File in $Path) {
    $Name = Split-Path -Leaf $File
    if ($VelopackHook -and $Name -like "*-Setup.exe") {
        Write-Host "Leaving $Name to be signed after its package is stored"
        continue
    }
    if ($VelopackHook -and (Get-AuthenticodeSignature -LiteralPath $File).SignerCertificate) {
        Write-Host "Keeping the signed $Name"
        continue
    }
    $Files += (Resolve-Path -LiteralPath $File).Path
}
if ($Files.Count -eq 0) { exit 0 }

# The timestamp server and the SimplySign cloud are network calls; retry the
# files that are still unsigned a few times before failing.
$Pending = $Files
for ($Attempt = 1; $Attempt -le 3 -and $Pending.Count -gt 0; $Attempt++) {
    & $Signtool sign /sha1 $Thumbprint /fd sha256 /tr $TimestampUrl /td sha256 /d "Ghostex" /du "https://github.com/maddada/Ghostex" @Pending
    $Pending = @($Pending | Where-Object { (Get-AuthenticodeSignature -LiteralPath $_).SignerCertificate.Thumbprint -ne $Thumbprint })
    if ($Pending.Count -gt 0 -and $Attempt -lt 3) {
        Write-Host "signtool left $($Pending.Count) file(s) unsigned (exit $LASTEXITCODE); retrying in $(15 * $Attempt) s"
        Start-Sleep -Seconds (15 * $Attempt)
    }
}
foreach ($File in $Files) {
    $Signature = Get-AuthenticodeSignature -LiteralPath $File
    if ($Signature.Status -ne "Valid" -or $Signature.SignerCertificate.Thumbprint -ne $Thumbprint) {
        throw "Certum signing failed for $File (status $($Signature.Status), signer $($Signature.SignerCertificate.Thumbprint))"
    }
    if (-not $Signature.TimeStamperCertificate) {
        throw "The Certum signature of $File has no timestamp"
    }
    Write-Host "Signed $(Split-Path -Leaf $File) with Certum"
}

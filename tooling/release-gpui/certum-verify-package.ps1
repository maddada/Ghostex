# CDXC:Release 2026-11-10 DECISION ("windows has to be signed release please!"):
# the last gate of a Certum-signed Windows build (windows.ps1 runs it before
# writing manifest.json). Every program the customer gets must carry a Valid,
# timestamped Authenticode signature from the Certum certificate: the installer,
# and each .exe/.dll/.node in the full update package, read back from the
# package itself so a file Velopack's sign hook skipped cannot slip through.
# The native editor under code-server/ is third-party and excluded, as when
# signing. Also usable by hand on Windows after a release, on the downloaded
# ghostex-<version>-windows-<arch>.exe and Ghostex-<version>-win-<arch>-stable-full.nupkg.
param(
    [Parameter(Mandatory = $true)][string]$Package,
    [Parameter(Mandatory = $true)][string]$Installer,
    [string]$WorkDir = (Join-Path ($env:RUNNER_TEMP ?? [IO.Path]::GetTempPath()) "certum-signature-check"),
    [string]$Thumbprint = $env:GHOSTEX_CERTUM_CERT_SHA1
)
$ErrorActionPreference = "Stop"

$Thumbprint = ($Thumbprint -replace '[^0-9A-Fa-f]', '').ToUpperInvariant()
if ($Thumbprint.Length -ne 40) { throw "Pass -Thumbprint or set GHOSTEX_CERTUM_CERT_SHA1 to the certificate's 40-digit SHA-1 thumbprint" }
if (Test-Path $WorkDir) { Remove-Item -Recurse -Force $WorkDir }
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null

Add-Type -AssemblyName System.IO.Compression.FileSystem
$Checked = @()
$Zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $Package).Path)
try {
    $Index = 0
    foreach ($Entry in $Zip.Entries) {
        $Path = [Uri]::UnescapeDataString($Entry.FullName)
        if ($Path -notmatch '\.(exe|dll|node)$' -or $Path -match '[\\/]code-server[\\/]') { continue }
        $Index++
        $Target = Join-Path $WorkDir "$Index-$([IO.Path]::GetFileName($Path))"
        [System.IO.Compression.ZipFileExtensions]::ExtractToFile($Entry, $Target)
        $Checked += [pscustomobject]@{ Name = $Path; File = $Target }
    }
}
finally { $Zip.Dispose() }
$Checked += [pscustomobject]@{ Name = (Split-Path -Leaf $Installer); File = (Resolve-Path -LiteralPath $Installer).Path }
foreach ($Required in @("Ghostex.exe", "ghostex-gpui-runtime.exe", "ghostex-gpui-cef-helper.exe", "GhostexEditor.exe", "gxserver.exe", "wmx.exe", "Squirrel.exe")) {
    if (-not ($Checked | Where-Object { $_.Name -like "*/$Required" })) { throw "The full update package has no $Required to check" }
}
$Unsigned = @()
foreach ($Item in $Checked) {
    $Signature = Get-AuthenticodeSignature -LiteralPath $Item.File
    $Ok = $Signature.Status -eq "Valid" -and $Signature.SignerCertificate.Thumbprint -eq $Thumbprint -and $Signature.TimeStamperCertificate
    Write-Host "$(if ($Ok) { 'signed  ' } else { 'UNSIGNED' }) $($Item.Name) (status $($Signature.Status), signer $($Signature.SignerCertificate.Thumbprint), timestamp $($Signature.TimeStamperCertificate.Subject))"
    if (-not $Ok) { $Unsigned += $Item.Name }
}
Remove-Item -Recurse -Force $WorkDir
if ($Unsigned.Count -gt 0) {
    throw "These Windows files are not signed with the Certum certificate $($Thumbprint): $($Unsigned -join ', ')"
}
Write-Host "Checked $($Checked.Count) Windows files: every one is Valid, signed by $Thumbprint and timestamped"

param(
    [Parameter(Mandatory = $true)][string]$Version,
    [ValidateSet("x64", "arm64")][string]$Arch = "x64",
    [string]$Output = ""
)

$ErrorActionPreference = "Stop"
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version must be MAJOR.MINOR.PATCH, got $Version"
}
$NativeArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
$ExpectedNativeArch = if ($Arch -eq "arm64") { "arm64" } else { "x64" }
if ($NativeArch -ne $ExpectedNativeArch) {
    throw "Windows $Arch releases require a native $ExpectedNativeArch runner; this runner is $NativeArch"
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "../..")
if (-not $Output) {
    $Output = Join-Path $RepoRoot "build/release-gpui/$Version/windows-$Arch"
}
$AllowedRoot = Join-Path $RepoRoot "build/release-gpui"
$ResolvedParent = [System.IO.Path]::GetFullPath((Split-Path -Parent $Output))
if (-not $ResolvedParent.StartsWith([System.IO.Path]::GetFullPath($AllowedRoot))) {
    throw "Release output must stay under $AllowedRoot"
}
if (Test-Path $Output) { Remove-Item -Recurse -Force $Output }
New-Item -ItemType Directory -Force -Path $Output | Out-Null

& bash (Join-Path $ScriptDir "prepare-references.sh")
if ($LASTEXITCODE -ne 0) { throw "GPUI reference preparation failed" }

$env:GHOSTEX_WINDOWS_ARCH = $Arch
$env:GHOSTEX_GPUI_MARKETING_VERSION = $Version
$env:GHOSTEX_ON_DEMAND_ASSETS = "1"
& (Join-Path $RepoRoot "apps/desktop/scripts/build-windows-app.ps1")
if ($LASTEXITCODE -ne 0) { throw "Windows GPUI build failed" }

$AppDir = Join-Path $RepoRoot "apps/desktop/build/windows/Ghostex"
foreach ($required in @("Ghostex.exe", "ghostex-gpui-runtime.exe", "ghostex-gpui-cef-helper.exe")) {
    if (-not (Test-Path (Join-Path $AppDir $required))) {
        throw "Windows staged app is missing $required"
    }
}
if (Test-Path (Join-Path $AppDir "libcef.dll")) {
    throw "Windows release build still bundles libcef.dll"
}
$WslArchive = Join-Path $AppDir "resources/wsl/gxserver-linux-$Arch.tar.gz"
$WslArchiveSha = "$WslArchive.sha256"
$OnDemandManifestPath = Join-Path $AppDir "resources/on-demand-resources.json"
if (-not (Test-Path $OnDemandManifestPath)) {
    throw "Windows staged app is missing its sealed component manifest"
}
$OnDemandManifest = Get-Content -Raw $OnDemandManifestPath | ConvertFrom-Json
$CefComponent = $OnDemandManifest.components.cef
$CefAsset = $CefComponent.platforms."windows-$Arch"
if (-not $CefComponent.componentVersion -or $CefComponent.downloadTag -ne "cef-$($CefComponent.componentVersion)" -or $CefAsset.sha256 -cnotmatch '^[0-9a-f]{64}$') {
    throw "Windows staged app has an invalid CEF component entry"
}
if ($env:GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME -ne "0") {
    if (-not (Test-Path $WslArchive)) {
        throw "Windows staged app is missing its WSL gxserver runtime: $WslArchive"
    }
    if (-not (Test-Path $WslArchiveSha)) {
        throw "Windows staged app is missing its WSL gxserver checksum: $WslArchiveSha"
    }
    $ExpectedWslSha = (Get-Content -Raw $WslArchiveSha).Trim()
    $ActualWslSha = (Get-FileHash -Algorithm SHA256 $WslArchive).Hash.ToLowerInvariant()
    if ($ExpectedWslSha -cnotmatch '^[0-9a-f]{64}$' -or $ExpectedWslSha -cne $ActualWslSha) {
        throw "Windows staged WSL gxserver checksum does not match its runtime archive"
    }
    $BundledWslSourceArchives = @(Get-ChildItem (Join-Path $AppDir "resources/wsl") -File -Filter "code-server-*-linux-$Arch.tar.gz" -ErrorAction SilentlyContinue)
    if ($BundledWslSourceArchives.Count -ne 0) {
        throw "Windows staged app still bundles its optional WSL Source runtime"
    }
    $CodeServerComponent = $OnDemandManifest.components.'code-server'
    $CodeServerAsset = $CodeServerComponent.platforms."windows-$Arch"
    if (-not $CodeServerComponent.componentVersion -or $CodeServerComponent.downloadTag -ne "code-server-$($CodeServerComponent.componentVersion)" -or $CodeServerAsset.sha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw "Windows staged app has an invalid code-server component entry"
    }
}

$ComponentManifestPath = Join-Path $RepoRoot "build/on-demand-components/components.json"
$ComponentManifest = Get-Content -Raw $ComponentManifestPath | ConvertFrom-Json
$CodeServerComponentVersion = $ComponentManifest.components.'code-server'.componentVersion
$ComponentPublishes = @(
    @{ Name = "cef"; Version = $ComponentManifest.components.cef.componentVersion }
)
if ($CodeServerComponentVersion) {
    $ComponentPublishes += @{ Name = "code-server"; Version = $CodeServerComponentVersion }
}
foreach ($ComponentPublish in $ComponentPublishes) {
    $PublishArgs = @(
        (Join-Path $RepoRoot "scripts/release-gpui/publish-component.mjs")
        "--component"
        $ComponentPublish.Name
        "--version"
        $ComponentPublish.Version
        "--asset-dir"
        (Join-Path $RepoRoot "build/on-demand-components/assets")
        "--output"
        $ComponentManifestPath
    )
    if ($ComponentPublish.Name -eq "code-server") {
        $PublishArgs += "--require-sha256-sidecars"
    }
    & node @PublishArgs
    if ($LASTEXITCODE -ne 0) { throw "Publishing the Windows $($ComponentPublish.Name) component failed" }
}

$SigningPfx = $env:GHOSTEX_WINDOWS_SIGNING_PFX
$SigningPassword = $env:GHOSTEX_WINDOWS_SIGNING_PASSWORD
$RequireSigning = $env:GHOSTEX_WINDOWS_REQUIRE_SIGNING -ne "0"
if ($RequireSigning -and (-not $SigningPfx -or -not (Test-Path $SigningPfx) -or -not $SigningPassword)) {
    throw "GHOSTEX_WINDOWS_SIGNING_PFX and GHOSTEX_WINDOWS_SIGNING_PASSWORD are required"
}
$SignTool = $null
if ($RequireSigning) {
    $SignTool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe |
        Where-Object { $_.FullName -match "\\$ExpectedNativeArch\\signtool\.exe$" } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if (-not $SignTool) { throw "A native $ExpectedNativeArch signtool.exe was not found" }
    foreach ($binary in @("Ghostex.exe", "ghostex-gpui-runtime.exe", "ghostex-gpui-cef-helper.exe")) {
        $binaryPath = Join-Path $AppDir $binary
        & $SignTool.FullName sign /fd SHA256 /td SHA256 /tr http://timestamp.digicert.com /f $SigningPfx /p $SigningPassword $binaryPath
        if ($LASTEXITCODE -ne 0) { throw "Authenticode signing failed for $binary" }
        & $SignTool.FullName verify /pa /all $binaryPath
        if ($LASTEXITCODE -ne 0) { throw "Authenticode verification failed for $binary" }
    }
}

$Vpk = Get-Command vpk -ErrorAction SilentlyContinue
if (-not $Vpk) {
    throw "Velopack CLI 1.2.0 is required to create the Windows release"
}
$Channel = "win-$Arch-stable"
$Runtime = "win-$Arch"
$PackId = "Ghostex"
$VpkOutput = Join-Path $Output "velopack"
New-Item -ItemType Directory -Force -Path $VpkOutput | Out-Null

$Changelog = Get-Content -Raw (Join-Path $RepoRoot "CHANGELOG.md")
$EscapedVersion = [regex]::Escape($Version)
$ReleaseNotesMatch = [regex]::Match(
    $Changelog,
    "(?ms)^## $EscapedVersion -.*?(?=^## |\z)"
)
if (-not $ReleaseNotesMatch.Success) {
    throw "CHANGELOG.md has no $Version section"
}
$ReleaseNotesPath = Join-Path $Output "release-notes.md"
$ReleaseNotesMatch.Value.Trim() | Set-Content -Encoding UTF8 $ReleaseNotesPath

# Seed the output with the previous release when one exists so Velopack can
# generate its normal delta package. A brand-new architecture/channel has no
# feed yet and correctly starts with a full package only.
$GithubHeaders = @{ "User-Agent" = "Ghostex-release" }
if ($env:GITHUB_TOKEN) {
    $GithubHeaders["Authorization"] = "Bearer $($env:GITHUB_TOKEN)"
    $env:VPK_TOKEN = $env:GITHUB_TOKEN
}
$PublishedReleases = Invoke-RestMethod `
    -Headers $GithubHeaders `
    -Uri "https://api.github.com/repos/maddada/Ghostex/releases?per_page=10"
$FeedName = "releases.$Channel.json"
$HasPreviousFeed = $PublishedReleases | Where-Object {
    -not $_.draft -and -not $_.prerelease -and ($_.assets.name -contains $FeedName)
} | Select-Object -First 1
if ($HasPreviousFeed) {
    & $Vpk.Source download github `
        --channel $Channel `
        --outputDir $VpkOutput `
        --repoUrl "https://github.com/maddada/Ghostex"
    if ($LASTEXITCODE -ne 0) { throw "Velopack could not download the previous $Channel release" }
}

if ($RequireSigning) {
    $env:VPK_SIGN_PARAMS = "/fd SHA256 /td SHA256 /tr http://timestamp.digicert.com /f `"$SigningPfx`" /p `"$SigningPassword`""
}
& $Vpk.Source pack `
    --channel $Channel `
    --mainExe "Ghostex.exe" `
    --outputDir $VpkOutput `
    --packAuthors "Ghostex" `
    --packDir $AppDir `
    --packId $PackId `
    --packTitle "Ghostex" `
    --packVersion $Version `
    --releaseNotes $ReleaseNotesPath `
    --runtime $Runtime `
    --shortcuts "Desktop,StartMenuRoot"
if ($LASTEXITCODE -ne 0) { throw "Velopack packaging failed" }
Remove-Item Env:VPK_SIGN_PARAMS -ErrorAction SilentlyContinue
Remove-Item Env:VPK_TOKEN -ErrorAction SilentlyContinue

$GeneratedInstaller = Get-ChildItem $VpkOutput -File -Filter "*Setup.exe" | Select-Object -First 1
$GeneratedPortable = Get-ChildItem $VpkOutput -File -Filter "*Portable.zip" | Select-Object -First 1
if (-not $GeneratedInstaller -or -not $GeneratedPortable) {
    throw "Velopack did not produce both Setup.exe and Portable.zip"
}
$Installer = Join-Path $Output "ghostex-$Version-windows-$Arch.exe"
$Archive = Join-Path $Output "ghostex-$Version-windows-$Arch-portable.zip"
$InstallerLauncher = Join-Path $RepoRoot "apps/desktop/target/release/ghostex-windows-installer.exe"
if (-not (Test-Path $InstallerLauncher)) {
    throw "Windows build did not produce the Ghostex installer launcher"
}

# CDXC:WindowsSeamlessInstaller 2026-08-16:
# Velopack's Setup.exe correctly supports update and same-version repair, but
# it pauses on an "already installed" confirmation before doing either. Keep
# the signed Velopack setup as the transactional inner installer and bundle it
# behind Ghostex's small launcher. The launcher uses normal interactive setup
# for a first install or downgrade; for a same/newer
# downloaded version it supplies Velopack's supported --silent confirmation,
# waits for replacement, and relaunches the stable installed Ghostex.exe. When
# release signing is enabled, the inner setup and final customer-facing
# executable retain independent Authenticode signatures.
$InnerInstaller = Join-Path $VpkOutput "Ghostex-Velopack-Setup.exe"
Move-Item $GeneratedInstaller.FullName $InnerInstaller
if ($RequireSigning) {
    & $SignTool.FullName verify /pa /all $InnerInstaller
    if ($LASTEXITCODE -ne 0) { throw "Authenticode verification failed for the inner Velopack installer" }
}
Copy-Item $InstallerLauncher $Installer
$InstallerStream = [IO.File]::Open($Installer, [IO.FileMode]::Append, [IO.FileAccess]::Write, [IO.FileShare]::None)
$InnerStream = [IO.File]::OpenRead($InnerInstaller)
try {
    $InnerLength = [UInt64]$InnerStream.Length
    $InnerStream.CopyTo($InstallerStream)
    $Magic = [Text.Encoding]::ASCII.GetBytes("GXSETUP1TRAILER!")
    $InstallerStream.Write($Magic, 0, $Magic.Length)
    $LengthBytes = [BitConverter]::GetBytes($InnerLength)
    if (-not [BitConverter]::IsLittleEndian) { [Array]::Reverse($LengthBytes) }
    $InstallerStream.Write($LengthBytes, 0, $LengthBytes.Length)
}
finally {
    $InnerStream.Dispose()
    $InstallerStream.Dispose()
}
$ExpectedBundledLength = (Get-Item $InstallerLauncher).Length + [Int64]$InnerLength + 24
if ((Get-Item $Installer).Length -ne $ExpectedBundledLength) {
    throw "Ghostex installer launcher did not bundle the complete Velopack setup payload"
}
Remove-Item $InnerInstaller
Move-Item $GeneratedPortable.FullName $Archive

$UpdateArtifacts = @()
$CurrentPackagePattern = "^$([regex]::Escape($PackId))-$EscapedVersion-$([regex]::Escape($Channel))-(full|delta)\.nupkg$"
$CurrentPackages = Get-ChildItem $VpkOutput -File -Filter "*.nupkg" | Where-Object {
    $_.Name -match $CurrentPackagePattern
}
if (-not ($CurrentPackages | Where-Object { $_.Name -match '-full\.nupkg$' })) {
    throw "Velopack did not produce the current full update package"
}
foreach ($package in $CurrentPackages) {
    $destination = Join-Path $Output $package.Name
    Move-Item $package.FullName $destination
    $UpdateArtifacts += $destination
}
foreach ($feed in @($FeedName, "assets.$Channel.json", "RELEASES-$Channel")) {
    $source = Join-Path $VpkOutput $feed
    if (Test-Path $source) {
        $destination = Join-Path $Output $feed
        Move-Item $source $destination
        $UpdateArtifacts += $destination
    }
}
if (-not (Test-Path (Join-Path $Output $FeedName))) {
    throw "Velopack did not produce $FeedName"
}
Remove-Item -Recurse -Force $VpkOutput
Remove-Item $ReleaseNotesPath

if ($RequireSigning) {
    & $SignTool.FullName sign /fd SHA256 /td SHA256 /tr http://timestamp.digicert.com /f $SigningPfx /p $SigningPassword $Installer
    if ($LASTEXITCODE -ne 0) { throw "Authenticode signing failed for the Ghostex installer" }
    & $SignTool.FullName verify /pa /all $Installer
    if ($LASTEXITCODE -ne 0) { throw "Authenticode verification failed for the Ghostex installer" }
}
$InstallerCheckStream = [IO.File]::OpenRead($Installer)
try {
    $TailLength = [Math]::Min([Int64](1024 * 1024), $InstallerCheckStream.Length)
    $InstallerCheckStream.Seek(-$TailLength, [IO.SeekOrigin]::End) | Out-Null
    $TailBytes = [byte[]]::new([int]$TailLength)
    $TailBytesRead = $InstallerCheckStream.Read($TailBytes, 0, $TailBytes.Length)
    $TailText = [Text.Encoding]::ASCII.GetString($TailBytes)
    if ($TailBytesRead -ne $TailBytes.Length -or -not $TailText.Contains("GXSETUP1TRAILER!")) {
        throw "Ghostex installer signature step did not preserve the Velopack setup trailer"
    }
}
finally {
    $InstallerCheckStream.Dispose()
}

$Artifacts = @($Installer, $Archive) + $UpdateArtifacts | ForEach-Object {
    $item = Get-Item $_
    [ordered]@{
        name = $item.Name
        sha256 = (Get-FileHash -Algorithm SHA256 $item.FullName).Hash.ToLowerInvariant()
        size = $item.Length
    }
}
[ordered]@{
    artifacts = $Artifacts
    platform = "windows-$Arch"
    schemaVersion = 1
    version = $Version
} | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 (Join-Path $Output "manifest.json")

Write-Host "Built Windows $Arch release payload in $Output"

# CDXC:Release 2026-09-02:
# -Phase lets the release workflow run this script as two watched steps so the
# Rust compile no longer waits on the gxserver job whose tarball only the
# packaging half reads:
#   all      (default) the whole release, exactly as before; local and
#            single-shot callers keep using this.
#   compile  GPUI and wmx reference checkout, sidebar bundle, cargo build. Reads no
#            runtime artifact. Leaves its results in apps/desktop/target and
#            apps/desktop/dist and exits.
#   package  build the native editor, then stage the app directory from the
#            compiled binaries, seal components, verify, sign, vpk pack,
#            bundle the launcher, write manifest.json. Requires a completed
#            `compile` on the same checkout.
# The command sequence inside each phase is byte-identical to `all`; the
# phases only gate which contiguous part of it runs.
# CDXC:Release 2026-10-07 WHY:
# `compile` is two independent halves, and the release runs them on two runners
# of the same label so the slowest job (Windows ARM64) stops compiling the
# native runtime after the desktop app (5m on gxserver alone in 10.14.0):
#   compile-app      GPUI references, sidebar bundle, desktop cargo build.
#   compile-runtime  wmx reference, then gxserver/ghostex, wmx, and the prompt
#                    editor (page + helper). The release workflow uploads its
#                    outputs at their repo paths and the packaging job
#                    downloads them in place before `package`.
# `compile` (and `all`) still runs both halves in the original order, which is
# what local builds and the sccache warm jobs use.
# CDXC:Release 2026-10-17 WHY:
# SignPath signs through a GitHub Actions step (signpath/github-action-submit-signing-request),
# not a local signtool call, so a signed release splits `package` into four
# phases with a SignPath round trip after each of the first three:
#   package-collect   stage and seal the app, then a throwaway `vpk pack` whose
#                     sign hook copies out every PE file Velopack would sign
#                     (our binaries plus the Update.exe and launcher stub it
#                     generates) -> SignPath round 1.
#   package-pack      the real `vpk pack`; its sign hook swaps each file for its
#                     round-1 signed copy, matched by the SHA-256 of the
#                     unsigned bytes, so the nupkg and delta hold
#                     signed binaries. The inner Velopack Setup.exe is left for
#                     SignPath round 2.
#   package-bundle    bundle the signed inner setup behind the launcher and
#                     move the update packages into place -> SignPath round 3
#                     signs the customer-facing installer.
#   package-finalize  put the signed installer in place, check the trailer,
#                     write manifest.json.
# Re-signing after `vpk pack` would change the nupkg bytes its feed and delta
# already hashed, which is why the binaries are signed in between two packs.
# `package` stays the unsigned all-in-one packaging used without a SignPath token.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [ValidateSet("x64", "arm64")][string]$Arch = "x64",
    [string]$Output = "",
    [ValidateSet("all", "compile", "compile-app", "compile-runtime", "package", "package-collect", "package-pack", "package-bundle", "package-finalize")][string]$Phase = "all"
)

$ErrorActionPreference = "Stop"
$RunCompileApp = $Phase -in @("all", "compile", "compile-app")
$RunCompileRuntime = $Phase -in @("all", "compile", "compile-runtime")
$RunPackage = $Phase -in @("all", "package", "package-collect", "package-pack", "package-bundle", "package-finalize")
$RunStage = $Phase -in @("all", "package", "package-collect")
$RunCollect = $Phase -eq "package-collect"
$RunPack = $Phase -in @("all", "package", "package-pack")
$RunBundle = $Phase -in @("all", "package", "package-bundle")
$RunFinalize = $Phase -in @("all", "package", "package-finalize")
$SignedPhases = $Phase -in @("package-collect", "package-pack", "package-bundle", "package-finalize")
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
# The SignPath round-trip folders sit beside the release output, never inside
# it: the output directory is what the workflow uploads and attests.
$SigningRoot = "$Output-signpath"
if ($RunStage) {
    foreach ($dir in @($Output, $SigningRoot)) {
        if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    }
    New-Item -ItemType Directory -Force -Path $Output | Out-Null
}

if ($RunCompileApp) {
    & bash (Join-Path $ScriptDir "prepare-references.sh")
    if ($LASTEXITCODE -ne 0) { throw "GPUI reference preparation failed" }
}
if ($RunCompileRuntime) {
    & git -C $RepoRoot submodule update --init --depth=1 -- .dependencies/wmx
    if ($LASTEXITCODE -ne 0) { throw "wmx reference preparation failed" }
}

# CDXC:Release 2026-09-27 WHY: 10.4.0 run 36301375274 stopped in create-deterministic-tar.sh because the inherited environment reached 29,263 bytes (PATH, the runner's OIDC and runtime tokens, the VS dev shell's saved PATH), over its 28,000-byte guard against CreateProcess's 32K limit. Nothing this script starts requests an OIDC token (attestation is its own workflow step with a fresh environment), so the OIDC request token is dropped here; ACTIONS_RUNTIME_TOKEN stays because sccache's GitHub cache needs it. This file is a Windows-only release input, so the fix could land mid-release without rebuilding the macOS and Linux packages that were already live.
Remove-Item Env:ACTIONS_ID_TOKEN_REQUEST_TOKEN -ErrorAction SilentlyContinue
Remove-Item Env:ACTIONS_ID_TOKEN_REQUEST_URL -ErrorAction SilentlyContinue

$env:GHOSTEX_WINDOWS_ARCH = $Arch
$env:GHOSTEX_GPUI_MARKETING_VERSION = $Version
$env:GHOSTEX_ON_DEMAND_ASSETS = "1"
# build-windows-app.ps1 runs both halves when this is unset.
$env:GHOSTEX_WINDOWS_BUILD_PHASE = switch ($Phase) {
    "compile" { "compile" }
    "compile-app" { "compile-app" }
    "compile-runtime" { "compile-runtime" }
    "package" { "stage" }
    "package-collect" { "stage" }
    default { $null }
}
if ($RunCompileApp -or $RunCompileRuntime -or $RunStage) {
    & (Join-Path $RepoRoot "apps/desktop/scripts/build-windows-app.ps1")
    if ($LASTEXITCODE -ne 0) { throw "Windows GPUI build failed" }
}
Remove-Item Env:GHOSTEX_WINDOWS_BUILD_PHASE -ErrorAction SilentlyContinue

if (-not $RunPackage) {
    Write-Host "Compiled the Windows $Arch release inputs; run -Phase package to stage and package them"
    exit 0
}

$AppDir = Join-Path $RepoRoot "apps/desktop/build/windows/Ghostex"
if ($RunStage) {
    foreach ($required in @("Ghostex.exe", "ghostex-gpui-runtime.exe", "ghostex-gpui-cef-helper.exe", "resources/native/gxserver.exe", "resources/native/wmx.exe", "resources/GhostexEditor/GhostexEditor.exe", "resources/GhostexEditor/web/index.html", "resources/sounds/arcade.mp3", "resources/sounds/copy.mp3")) {
        if (-not (Test-Path (Join-Path $AppDir $required))) {
            throw "Windows staged app is missing $required"
        }
    }
    if (Test-Path (Join-Path $AppDir "libcef.dll")) {
        throw "Windows release build still bundles libcef.dll"
    }
    $WslArchive = Join-Path $AppDir "resources/wsl/gxserver-linux-$Arch.tar.gz"
    $OnDemandManifestPath = Join-Path $AppDir "resources/on-demand-resources.json"
    if (-not (Test-Path $OnDemandManifestPath)) {
        throw "Windows staged app is missing its sealed component manifest"
    }
    $OnDemandManifest = Get-Content -Raw $OnDemandManifestPath | ConvertFrom-Json
    # CEF (the web runtime) is optional: the app starts without it and downloads this sealed component
    # only when the user installs it from a web view or Plugins, so the entry must be valid and published.
    $CefComponent = $OnDemandManifest.components.cef
    $CefAsset = $CefComponent.platforms."windows-$Arch"
    if (-not $CefComponent.componentVersion -or $CefComponent.downloadTag -ne "cef-$($CefComponent.componentVersion)" -or $CefAsset.sha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw "Windows staged app has an invalid CEF component entry"
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
    # The native editor is downloaded on first use, never bundled (see build-windows-app.ps1).
    if ($env:GHOSTEX_WINDOWS_NATIVE_CODE_SERVER_ARCHIVE) {
        if (Test-Path (Join-Path $AppDir "code-server")) {
            throw "Windows staged app still bundles its native editor"
        }
        $NativeCodeServerAsset = $CodeServerComponent.platforms."windows-native-$Arch"
        if (-not $NativeCodeServerAsset -or $NativeCodeServerAsset.sha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw "Windows staged app has no valid on-demand native editor entry"
        }
        $ActualNativeSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $env:GHOSTEX_WINDOWS_NATIVE_CODE_SERVER_ARCHIVE).Hash.ToLowerInvariant()
        if ($NativeCodeServerAsset.sha256 -cne $ActualNativeSha) {
            throw "Windows sealed native editor sha256 does not match the verified windows-native-$Arch archive"
        }
    }
    # The WSL gxserver is a sealed on-demand asset, never a bundled file (see build-windows-app.ps1).
    if (Test-Path $WslArchive) {
        throw "Windows staged app still bundles its WSL gxserver runtime: $WslArchive"
    }
    if ($env:GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME -ne "0") {
        $WslAsset = $OnDemandManifest.assets."gxserver-linux-$Arch"
        if (-not $WslAsset -or $WslAsset.name -cne "gxserver-linux-$Arch.tar.gz" -or $WslAsset.sha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw "Windows staged app has no valid on-demand WSL gxserver asset entry"
        }
        if ($env:GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE) {
            $ActualWslSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $env:GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE).Hash.ToLowerInvariant()
            if ($WslAsset.sha256 -cne $ActualWslSha) {
                throw "Windows sealed WSL gxserver sha256 does not match the release's gxserver-linux-$Arch.tar.gz"
            }
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
            (Join-Path $RepoRoot "tooling/release-gpui/publish-component.mjs")
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

}

$Vpk = Get-Command vpk -ErrorAction SilentlyContinue
if (-not $Vpk) {
    throw "Velopack CLI 1.2.0 is required to create the Windows release"
}
$Channel = "win-$Arch-stable"
$Runtime = "win-$Arch"
$PackId = "Ghostex"
$EscapedVersion = [regex]::Escape($Version)
$FeedName = "releases.$Channel.json"
$VpkOutput = Join-Path $Output "velopack"
$Installer = Join-Path $Output "ghostex-$Version-windows-$Arch.exe"
$InnerInstallerName = "Ghostex-Velopack-Setup.exe"
$InnerInstaller = Join-Path $VpkOutput $InnerInstallerName
# Stores Velopack's Setup.exe package uncompressed and bundles it LZMA-compressed (CDXC:Release 2026-10-23 in apps/desktop/src/bin/ghostex_windows_installer_pack.rs).
$InstallerPackTool = Join-Path $RepoRoot "apps/desktop/target/release/ghostex-windows-installer-pack.exe"
# Each SignPath round trip uploads one folder and downloads its signed copy
# into the matching *-signed folder (release-gpui-windows.yml).
$CollectDir = Join-Path $SigningRoot "collect"
$CollectSignedDir = Join-Path $SigningRoot "collect-signed"
$SetupDir = Join-Path $SigningRoot "setup"
$SetupSignedDir = Join-Path $SigningRoot "setup-signed"
$InstallerDir = Join-Path $SigningRoot "installer"
$InstallerSignedDir = Join-Path $SigningRoot "installer-signed"
$VpkSignHook = Join-Path $ScriptDir "vpk-signpath-hook.ps1"
$VpkSignTemplate = "pwsh -NoProfile -NonInteractive -File `"$VpkSignHook`" {{file}}"
# The native editor (code-server, VS Code server, Node.js and their native
# modules) is third-party code: SignPath Foundation signs only the project's
# own binaries, so it ships as its upstream authors built it.
$VpkSignExclude = '[\\/]code-server[\\/]'

# The test-signing policy's certificate chains to a SignPath test root that no
# runner trusts, so only release-signing can demand a fully valid chain.
function Assert-SignPathSignature([string]$Path) {
    $Signature = Get-AuthenticodeSignature -LiteralPath $Path
    if (-not $Signature.SignerCertificate -or $Signature.Status -in @("NotSigned", "HashMismatch", "NotSupportedFileFormat", "Incompatible")) {
        throw "SignPath did not Authenticode-sign $Path (status $($Signature.Status))"
    }
    if ($env:GHOSTEX_WINDOWS_SIGNING_POLICY -ne "test-signing" -and $Signature.Status -ne "Valid") {
        throw "The Authenticode signature of $Path is not valid (status $($Signature.Status))"
    }
}

function Invoke-VelopackPack([string]$PackOutput, [bool]$WithSignHook) {
    $Changelog = Get-Content -Raw (Join-Path $RepoRoot "CHANGELOG.md")
    $ReleaseNotesMatch = [regex]::Match(
        $Changelog,
        "(?ms)^## $EscapedVersion -.*?(?=^## |\z)"
    )
    if (-not $ReleaseNotesMatch.Success) {
        throw "CHANGELOG.md has no $Version section"
    }
    $ReleaseNotesPath = Join-Path $Output "release-notes.md"
    $ReleaseNotesMatch.Value.Trim() | Set-Content -Encoding UTF8 $ReleaseNotesPath
    $SignArgs = @()
    if ($WithSignHook) { $SignArgs = @("--signTemplate", $VpkSignTemplate, "--signExclude", $VpkSignExclude) }
    # CDXC:Release 2026-10-23 DECISION: the user stopped publishing the Windows portable zip ("i dont want portable anymore for any release"), so Velopack makes none. Copies people already unpacked keep updating in place from the same feed and update packages, so the app's portable-mode code stays; the release checks that read the zip read the full update package instead (assemble.mjs, amend-existing.mjs).
    & $Vpk.Source pack `
        --channel $Channel `
        --mainExe "Ghostex.exe" `
        --outputDir $PackOutput `
        --packAuthors "Ghostex" `
        --packDir $AppDir `
        --packId $PackId `
        --packTitle "Ghostex" `
        --packVersion $Version `
        --releaseNotes $ReleaseNotesPath `
        --runtime $Runtime `
        --shortcuts "Desktop,StartMenuRoot" `
        --noPortable `
        @SignArgs
    if ($LASTEXITCODE -ne 0) { throw "Velopack packaging failed" }
    Remove-Item $ReleaseNotesPath
}

if ($RunCollect) {
    # A throwaway pack: only the files its sign hook copies out are kept. No
    # previous release is seeded because the delta package is not needed here.
    $CollectPackOutput = Join-Path $SigningRoot "collect-pack"
    New-Item -ItemType Directory -Force -Path $CollectDir, $CollectPackOutput | Out-Null
    $env:GHOSTEX_VPK_SIGN_MODE = "collect"
    $env:GHOSTEX_VPK_SIGN_DIR = $CollectDir
    Invoke-VelopackPack -PackOutput $CollectPackOutput -WithSignHook $true
    Remove-Item Env:GHOSTEX_VPK_SIGN_MODE, Env:GHOSTEX_VPK_SIGN_DIR
    Remove-Item -Recurse -Force $CollectPackOutput
    $Collected = @(Get-ChildItem $CollectDir -Recurse -File)
    if ($Collected.Count -eq 0) { throw "The Velopack sign hook collected no files to sign" }
    foreach ($required in @("Ghostex.exe", "ghostex-gpui-runtime.exe", "ghostex-gpui-cef-helper.exe", "GhostexEditor.exe", "gxserver.exe", "wmx.exe", "Squirrel.exe")) {
        if (-not ($Collected | Where-Object { $_.Name -eq $required })) {
            throw "The Velopack sign hook did not collect $required"
        }
    }
    Write-Host "Collected $($Collected.Count) Windows $Arch files for SignPath:"
    $Collected | ForEach-Object { Write-Host "  $($_.FullName.Substring($CollectDir.Length + 1))" }
}

if ($RunPack) {
    if (Test-Path $VpkOutput) { Remove-Item -Recurse -Force $VpkOutput }
    New-Item -ItemType Directory -Force -Path $VpkOutput | Out-Null
    if ($SignedPhases) {
        $SignedFiles = @(Get-ChildItem $CollectSignedDir -Recurse -File)
        if ($SignedFiles.Count -ne @(Get-ChildItem $CollectDir -Recurse -File).Count) {
            throw "SignPath returned $($SignedFiles.Count) files for the $(@(Get-ChildItem $CollectDir -Recurse -File).Count) collected ones"
        }
        foreach ($file in $SignedFiles) { Assert-SignPathSignature $file.FullName }
        # Put the signed binaries into the staged app before packing, then
        # re-seal build-identity.json: the app compares its fingerprint with
        # the SHA-256 of the installed gxserver.exe and wmx.exe
        # (windows_terminal_backend/native_package.rs), and signing changed them.
        $StagedBinaries = Get-ChildItem $AppDir -Recurse -File | Where-Object {
            $_.Extension -in @(".exe", ".dll", ".node") -and $_.FullName -notmatch $VpkSignExclude
        }
        foreach ($binary in $StagedBinaries) {
            $sha = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary.FullName).Hash.ToLowerInvariant()
            $signed = Join-Path (Join-Path $CollectSignedDir $sha) $binary.Name
            if (Test-Path -LiteralPath $signed) {
                Copy-Item -LiteralPath $signed -Destination $binary.FullName -Force
            }
            elseif (-not (Get-AuthenticodeSignature -LiteralPath $binary.FullName).SignerCertificate) {
                throw "SignPath returned no signed copy of $($binary.FullName)"
            }
        }
        & (Join-Path $RepoRoot "apps/desktop/scripts/write-windows-native-identity.ps1") -AppDir $AppDir
    }

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

    if ($SignedPhases) {
        $env:GHOSTEX_VPK_SIGN_MODE = "replace"
        $env:GHOSTEX_VPK_SIGN_DIR = $CollectSignedDir
    }
    Invoke-VelopackPack -PackOutput $VpkOutput -WithSignHook $SignedPhases
    Remove-Item Env:GHOSTEX_VPK_SIGN_MODE, Env:GHOSTEX_VPK_SIGN_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:VPK_TOKEN -ErrorAction SilentlyContinue

    $GeneratedInstaller = Get-ChildItem $VpkOutput -File -Filter "*Setup.exe" | Select-Object -First 1
    if (-not $GeneratedInstaller) {
        throw "Velopack did not produce Setup.exe"
    }
    # Before SignPath signs it: rewriting the package afterwards would break the signature.
    & $InstallerPackTool store $GeneratedInstaller.FullName
    if ($LASTEXITCODE -ne 0) { throw "Storing the Velopack Setup.exe package uncompressed failed" }
    if ($SignedPhases) {
        New-Item -ItemType Directory -Force -Path $SetupDir | Out-Null
        Move-Item $GeneratedInstaller.FullName (Join-Path $SetupDir $InnerInstallerName)
    }
    else {
        Move-Item $GeneratedInstaller.FullName $InnerInstaller
    }
}

if ($RunBundle) {
    $InstallerLauncher = Join-Path $RepoRoot "apps/desktop/target/release/ghostex-windows-installer.exe"
    if (-not (Test-Path $InstallerLauncher) -or -not (Test-Path $InstallerPackTool)) {
        throw "Windows build did not produce the Ghostex installer launcher and its packing tool"
    }

    # CDXC:PlatformSupport 2026-08-16:
    # Velopack's Setup.exe correctly supports update and same-version repair, but
    # it pauses on an "already installed" confirmation before doing either. Keep
    # the signed Velopack setup as the transactional inner installer and bundle it
    # behind Ghostex's small launcher. The launcher uses normal interactive setup
    # for a first install or downgrade; for a same/newer
    # downloaded version it supplies Velopack's supported --silent confirmation,
    # waits for replacement, and relaunches the stable installed Ghostex.exe. When
    # release signing is enabled, the inner setup and final customer-facing
    # executable retain independent Authenticode signatures. The launcher carries
    # the signed setup LZMA-compressed (CDXC:Release 2026-10-23 in
    # apps/desktop/src/bin/ghostex_windows_installer_pack.rs).
    $InnerSource = $InnerInstaller
    $BundledInstaller = $Installer
    if ($SignedPhases) {
        $InnerSource = Join-Path $SetupSignedDir $InnerInstallerName
        Assert-SignPathSignature $InnerSource
        New-Item -ItemType Directory -Force -Path $InstallerDir | Out-Null
        $BundledInstaller = Join-Path $InstallerDir (Split-Path -Leaf $Installer)
    }
    # The tool unpacks what it wrote and compares it with the setup before it succeeds.
    & $InstallerPackTool bundle $InstallerLauncher $InnerSource $BundledInstaller
    if ($LASTEXITCODE -ne 0) { throw "Ghostex installer launcher did not bundle the complete Velopack setup payload" }
    Remove-Item $InnerSource

    $CurrentPackagePattern = "^$([regex]::Escape($PackId))-$EscapedVersion-$([regex]::Escape($Channel))-(full|delta)\.nupkg$"
    $CurrentPackages = Get-ChildItem $VpkOutput -File -Filter "*.nupkg" | Where-Object {
        $_.Name -match $CurrentPackagePattern
    }
    if (-not ($CurrentPackages | Where-Object { $_.Name -match '-full\.nupkg$' })) {
        throw "Velopack did not produce the current full update package"
    }
    foreach ($package in $CurrentPackages) {
        Move-Item $package.FullName (Join-Path $Output $package.Name)
    }
    foreach ($feed in @($FeedName, "assets.$Channel.json", "RELEASES-$Channel")) {
        $source = Join-Path $VpkOutput $feed
        if (Test-Path $source) {
            Move-Item $source (Join-Path $Output $feed)
        }
    }
    if (-not (Test-Path (Join-Path $Output $FeedName))) {
        throw "Velopack did not produce $FeedName"
    }
    Remove-Item -Recurse -Force $VpkOutput
}

if (-not $RunFinalize) {
    Write-Host "Finished the Windows $Arch $Phase phase"
    exit 0
}

if ($SignedPhases) {
    $SignedInstaller = Join-Path $InstallerSignedDir (Split-Path -Leaf $Installer)
    Assert-SignPathSignature $SignedInstaller
    Move-Item -Force $SignedInstaller $Installer
}
$InstallerCheckStream = [IO.File]::OpenRead($Installer)
try {
    $TailLength = [Math]::Min([Int64](1024 * 1024), $InstallerCheckStream.Length)
    $InstallerCheckStream.Seek(-$TailLength, [IO.SeekOrigin]::End) | Out-Null
    $TailBytes = [byte[]]::new([int]$TailLength)
    $TailBytesRead = $InstallerCheckStream.Read($TailBytes, 0, $TailBytes.Length)
    $TailText = [Text.Encoding]::ASCII.GetString($TailBytes)
    if ($TailBytesRead -ne $TailBytes.Length -or -not $TailText.Contains("GXSETUP2TRAILER!")) {
        throw "Ghostex installer signature step did not preserve the Velopack setup trailer"
    }
}
finally {
    $InstallerCheckStream.Dispose()
}

$CurrentPackagePattern = "^$([regex]::Escape($PackId))-$EscapedVersion-$([regex]::Escape($Channel))-(full|delta)\.nupkg$"
$UpdateArtifacts = @(Get-ChildItem $Output -File -Filter "*.nupkg" | Where-Object { $_.Name -match $CurrentPackagePattern } | ForEach-Object { $_.FullName })
foreach ($feed in @($FeedName, "assets.$Channel.json", "RELEASES-$Channel")) {
    $path = Join-Path $Output $feed
    if (Test-Path $path) { $UpdateArtifacts += $path }
}
$Artifacts = @($Installer) + $UpdateArtifacts | ForEach-Object {
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

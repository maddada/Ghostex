param(
    [switch]$SkipDependencies,
    [switch]$SkipCompile
)

# CDXC:Release 2026-09-16 WHY:
# The native Windows editor (VS Code REH for win32) is an immutable, version-free
# component published as code-server-<componentVersion>-windows-native-<arch>.tar.gz
# on the shared code-server component tag, so a release only compiles it when the
# identity changed. This script therefore has three ways to fill
# apps/desktop/build/native-code-server:
#   consume   GHOSTEX_WINDOWS_NATIVE_CODE_SERVER_ARCHIVE names a verified prebuilt
#             archive (the release workflow downloads the component job's artifact);
#             without it, a clean code-server tree reuses the published asset when
#             `gh` can see it, so local builds get the same speedup.
#   build     the source build, used when nothing is published or the tree has
#             local edits. GHOSTEX_WINDOWS_CODE_SERVER_COMPONENT_ONLY=1 additionally
#             packages the result as the deterministic component asset.
# The fingerprint no longer includes the app version: version and commit are
# stamped on the staged copy by build-windows-app.ps1, so a reused payload gets
# the current release's stamp too.
# SEE-ALSO: .github/workflows/release-gpui-code-server-windows.yml,
# tooling/release-gpui/verify-windows-native-code-server-archive.mjs,
# tooling/release-gpui/code-server-component-identity.mjs (CODE_SERVER_RECIPE_INPUTS).

$ErrorActionPreference = "Stop"
# Keep esbuild's parallel native bundlers within a practical Windows memory budget.
if (!$env:GOMEMLIMIT) { $env:GOMEMLIMIT = "2GiB" }
if (!$env:GOGC) { $env:GOGC = "50" }
if (!$env:GOMAXPROCS) { $env:GOMAXPROCS = "4" }
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../../..")).Path
$CodeRoot = Join-Path $RepoRoot ".dependencies/code-server"
$VscodeRoot = Join-Path $CodeRoot "lib/vscode"
$OutputRoot = Join-Path $RepoRoot "apps/desktop/build/native-code-server"
$Tooling = Join-Path $RepoRoot "tooling/release-gpui"
$Platform = (& node -p "process.platform").Trim()
if ($Platform -ne "win32") { throw "The native Windows editor must be built on Windows." }
$Arch = (& node -p "process.arch").Trim()
if ($env:GHOSTEX_WINDOWS_ARCH -and $env:GHOSTEX_WINDOWS_ARCH -ne $Arch) {
    throw "Use native $env:GHOSTEX_WINDOWS_ARCH Node.js to build that Windows editor architecture."
}
if ($LASTEXITCODE -ne 0 -or $Arch -notin @("x64", "arm64")) {
    throw "Building the Windows editor requires native Windows Node.js."
}
if (!(Test-Path (Join-Path $CodeRoot "package.json"))) {
    throw "Initialize the code-server submodule before building the Windows editor."
}
$ComponentPlatform = "windows-native-$Arch"
$ComponentOnly = $env:GHOSTEX_WINDOWS_CODE_SERVER_COMPONENT_ONLY -eq "1"
$PrebuiltArchive = $env:GHOSTEX_WINDOWS_NATIVE_CODE_SERVER_ARCHIVE
$RequiredNode = (Get-Content (Join-Path $CodeRoot ".node-version") -Raw).Trim()

function Invoke-Checked {
    param([string]$Executable, [string[]]$Arguments)
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Executable failed with exit code $LASTEXITCODE."
    }
}

function Get-GitBash {
    $GitPath = (Get-Command git.exe -ErrorAction Stop).Source
    $GitRoot = Split-Path (Split-Path $GitPath -Parent) -Parent
    $Bash = Join-Path $GitRoot "bin/bash.exe"
    if (!(Test-Path $Bash)) { throw "Git for Windows bash.exe was not found beside git.exe." }
    return $Bash
}

function Get-ComponentVersion {
    $Resolved = (& node (Join-Path $Tooling "code-server-component-identity.mjs") --root $CodeRoot).Trim()
    if ($LASTEXITCODE -ne 0 -or !$Resolved) {
        throw "Could not resolve the code-server component payload identity."
    }
    if ($env:GHOSTEX_CODE_SERVER_COMPONENT_VERSION -and $env:GHOSTEX_CODE_SERVER_COMPONENT_VERSION -ne $Resolved) {
        throw "Configured code-server component version does not match its payload identity: $env:GHOSTEX_CODE_SERVER_COMPONENT_VERSION != $Resolved"
    }
    return $Resolved
}

function Write-Stamp {
    [IO.File]::WriteAllText($Stamp, $Fingerprint, [Text.UTF8Encoding]::new($false))
}

function Install-PrebuiltEditor {
    param([string]$ArchivePath, [string]$ComponentVersion)
    $ExpectedName = "code-server-$ComponentVersion-$ComponentPlatform.tar.gz"
    if ((Split-Path -Leaf $ArchivePath) -ne $ExpectedName) {
        throw "Native Windows editor archive identity mismatch: expected $ExpectedName, got $(Split-Path -Leaf $ArchivePath)"
    }
    if (!(Test-Path "$ArchivePath.sha256")) {
        throw "Native Windows editor archive checksum sidecar is missing: $ArchivePath.sha256"
    }
    Invoke-Checked "node" @(
        (Join-Path $Tooling "verify-windows-native-code-server-archive.mjs"),
        "--archive", $ArchivePath,
        "--version", $ComponentVersion,
        "--platform", $ComponentPlatform
    )
    $Tar = (Get-Command tar.exe -ErrorAction Stop).Source
    if (Test-Path $OutputRoot) { Remove-Item -Recurse -Force $OutputRoot }
    New-Item -ItemType Directory -Force $OutputRoot | Out-Null
    Invoke-Checked $Tar @("-xzf", $ArchivePath, "-C", $OutputRoot)
    foreach ($path in @("lib/node.exe", "lib/vscode/out/server-main.js", "lib/vscode/product.json", "package.json")) {
        if (!(Test-Path (Join-Path $OutputRoot $path))) {
            throw "The native Windows editor component is incomplete after extraction: $path"
        }
    }
    Write-Stamp
    Write-Host "Native Windows editor unpacked from component $ComponentVersion ($ComponentPlatform) at $OutputRoot"
}

function Export-EditorComponent {
    param([string]$ComponentVersion)
    $AssetDir = if ($env:GHOSTEX_ON_DEMAND_COMPONENT_ASSET_DIR) {
        [IO.Path]::GetFullPath($env:GHOSTEX_ON_DEMAND_COMPONENT_ASSET_DIR)
    } else {
        Join-Path $RepoRoot "build/on-demand-components/assets"
    }
    New-Item -ItemType Directory -Force $AssetDir | Out-Null
    $AssetName = "code-server-$ComponentVersion-$ComponentPlatform.tar.gz"
    $Asset = Join-Path $AssetDir $AssetName
    $Stage = Join-Path $RepoRoot "build/on-demand-components/$ComponentPlatform-editor-stage"
    if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
    New-Item -ItemType Directory -Force $Stage | Out-Null
    # The deterministic tar helper rewrites timestamps and modes in place, so it runs on a copy of the build cache.
    & robocopy $OutputRoot $Stage /E /NFL /NDL /NJH /NJS /NP | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "Could not stage the native Windows editor component (robocopy exit code $LASTEXITCODE)." }
    & (Get-GitBash) (Join-Path $Tooling "create-deterministic-tar.sh") $Stage.Replace('\', '/') $Asset.Replace('\', '/') --windows-component
    if ($LASTEXITCODE -ne 0) { throw "Could not create the deterministic native Windows editor component asset." }
    Remove-Item -Recurse -Force $Stage
    $AssetSha = (Get-FileHash -Algorithm SHA256 $Asset).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$Asset.sha256", "$AssetSha  $AssetName`n", [Text.UTF8Encoding]::new($false))
    Invoke-Checked "node" @(
        (Join-Path $Tooling "verify-windows-native-code-server-archive.mjs"),
        "--archive", $Asset,
        "--version", $ComponentVersion,
        "--platform", $ComponentPlatform
    )
    Write-Host "Packaged native Windows editor component $Asset"
}

# The local fingerprint covers everything that determines the payload and nothing
# that does not: the code-server pin and its VS Code gitlink, local edits to
# either tree, this script, the pinned Node version, and the architecture.
$Revision = (& git -C $CodeRoot rev-parse HEAD).Trim()
$VscodeRevision = (& git -C $CodeRoot rev-parse "HEAD:lib/vscode").Trim()
$SourceDiff = (& git -C $CodeRoot diff --binary HEAD -- src ci patches package.json package-lock.json) -join "\n"
$VscodeDiff = if (Test-Path (Join-Path $VscodeRoot ".git")) { (& git -C $VscodeRoot diff --binary HEAD) -join "\n" } else { "" }
$ScriptHash = (Get-FileHash $PSCommandPath -Algorithm SHA256).Hash
$Inputs = "$Revision|$VscodeRevision|$SourceDiff|$VscodeDiff|$ScriptHash|$RequiredNode|$Arch"
$Sha = [Security.Cryptography.SHA256]::Create()
try { $Fingerprint = -join ($Sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Inputs)) | ForEach-Object { $_.ToString("x2") }) }
finally { $Sha.Dispose() }
$TreeClean = [string]::IsNullOrEmpty($SourceDiff) -and [string]::IsNullOrEmpty($VscodeDiff)
$Stamp = Join-Path $OutputRoot "ghostex-build-fingerprint"
if (!$SkipCompile -and (Test-Path $Stamp) -and
    (Get-Content $Stamp -Raw).Trim() -eq $Fingerprint -and
    (Test-Path (Join-Path $OutputRoot "lib/node.exe")) -and
    (Test-Path (Join-Path $OutputRoot "lib/vscode/out/server-main.js"))) {
    Write-Host "Native Windows editor is current."
    if ($ComponentOnly) { Export-EditorComponent (Get-ComponentVersion) }
    return
}

if ($PrebuiltArchive) {
    if (!(Test-Path $PrebuiltArchive)) {
        throw "The prebuilt native Windows editor archive is missing: $PrebuiltArchive"
    }
    Install-PrebuiltEditor -ArchivePath $PrebuiltArchive -ComponentVersion (Get-ComponentVersion)
    return
}

if (!$ComponentOnly -and !$SkipCompile -and $TreeClean -and (Get-Command gh -ErrorAction SilentlyContinue)) {
    $ComponentVersion = Get-ComponentVersion
    $Tag = "code-server-$ComponentVersion"
    $ArchiveName = "code-server-$ComponentVersion-$ComponentPlatform.tar.gz"
    # Component tags live in the components repository (GHOSTEX_COMPONENTS_REPO, resolved by components-repo.mjs).
    $ComponentsRepo = (& node (Join-Path $RepoRoot "tooling/release-gpui/components-repo.mjs")).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $ComponentsRepo) { throw "Could not resolve the components repository." }
    $PreviousErrorActionPreference = $ErrorActionPreference
    try {
        # Windows PowerShell turns native stderr into an error before the exit-code check.
        $ErrorActionPreference = "Continue"
        $Published = @(& gh release view $Tag --repo $ComponentsRepo --json assets --jq '.assets[].name' 2>$null | ForEach-Object { "$_" })
    }
    finally { $ErrorActionPreference = $PreviousErrorActionPreference }
    if ($LASTEXITCODE -eq 0 -and ($Published -contains $ArchiveName) -and ($Published -contains "$ArchiveName.sha256")) {
        $DownloadDir = Join-Path $RepoRoot "build/on-demand-components/$ComponentPlatform-editor-download"
        if (Test-Path $DownloadDir) { Remove-Item -Recurse -Force $DownloadDir }
        New-Item -ItemType Directory -Force $DownloadDir | Out-Null
        Invoke-Checked "gh" @(
            "release", "download", $Tag,
            "--repo", $ComponentsRepo,
            "--pattern", $ArchiveName,
            "--pattern", "$ArchiveName.sha256",
            "--dir", $DownloadDir,
            "--clobber"
        )
        Install-PrebuiltEditor -ArchivePath (Join-Path $DownloadDir $ArchiveName) -ComponentVersion $ComponentVersion
        Remove-Item -Recurse -Force $DownloadDir
        return
    }
    Write-Host "No published native Windows editor component for $Tag; building it from source."
}

# Source build prerequisites.
if (!(Get-Command signtool.exe -ErrorAction SilentlyContinue)) {
    $SdkRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits/10/bin"
    $SdkBin = Get-ChildItem $SdkRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName "$Arch" } |
        Where-Object { Test-Path (Join-Path $_ "signtool.exe") } |
        Select-Object -First 1
    if (!$SdkBin) { throw "The Windows SDK signtool.exe is required to package the native editor." }
    $env:PATH = "$SdkBin;$env:PATH"
}
$NodeVersion = (& node -p "process.versions.node").Trim()
if ($NodeVersion -ne $RequiredNode) {
    throw "The Windows editor requires Node $RequiredNode; found $NodeVersion."
}
$Bash = Get-GitBash
if (!(Test-Path (Join-Path $VscodeRoot "package.json"))) {
    throw "Initialize the code-server and nested VS Code submodules before building the Windows editor."
}
# build-vscode.sh bakes VERSION into product.json. The Linux and Darwin components
# bake code-server's own version there, never the app version, so the payload
# stays identical across releases.
$env:VERSION = (Get-Content (Join-Path $CodeRoot "package.json") -Raw | ConvertFrom-Json).version

Push-Location $CodeRoot
try {
    if (!$SkipDependencies) {
        Invoke-Checked "npm.cmd" @("ci", "--ignore-scripts", "--no-audit", "--no-fund")
        Push-Location $VscodeRoot
        try { Invoke-Checked "npm.cmd" @("ci", "--no-audit", "--no-fund") }
        finally { Pop-Location }
    }
    if (!$SkipCompile) {
        Invoke-Checked "node.exe" @("node_modules/typescript/bin/tsc")
        $env:VSCODE_TARGET = "win32-$Arch"
        $env:MINIFY = "true"
        # Copilot's build rewrites its manifest's whitespace. Preserve source formatting when its JSON is unchanged.
        $CopilotManifest = Join-Path $VscodeRoot "extensions/copilot/package.json"
        $CopilotBytes = [IO.File]::ReadAllBytes($CopilotManifest)
        $CopilotJson = ([Text.Encoding]::UTF8.GetString($CopilotBytes) | ConvertFrom-Json | ConvertTo-Json -Depth 100 -Compress)
        try { Invoke-Checked $Bash @("ci/build/build-vscode.sh") }
        finally {
            $BuiltJson = (Get-Content $CopilotManifest -Raw | ConvertFrom-Json | ConvertTo-Json -Depth 100 -Compress)
            if ($BuiltJson -ceq $CopilotJson) { [IO.File]::WriteAllBytes($CopilotManifest, $CopilotBytes) }
        }
    }
    $VscodeOutput = Join-Path $CodeRoot "lib/vscode-reh-web-win32-$Arch"
    foreach ($path in @("out/server-main.js", "node.exe", "product.json")) {
        if (!(Test-Path (Join-Path $VscodeOutput $path))) {
            throw "The Windows editor build is incomplete: $path"
        }
    }
    # CDXC:CodeEditor 2026-09-14 WHY:
    # Native projects need a Windows REH payload and native Node dependencies, rather than the Linux archive staged for WSL.
    # Stage separately so neither environment can accidentally launch the other's executable files.
    New-Item -ItemType Directory -Force $OutputRoot | Out-Null
    Copy-Item (Join-Path $CodeRoot "out") $OutputRoot -Recurse -Force
    New-Item -ItemType Directory -Force (Join-Path $OutputRoot "lib") | Out-Null
    $TargetVscode = Join-Path $OutputRoot "lib/vscode"
    New-Item -ItemType Directory -Force $TargetVscode | Out-Null
    Copy-Item (Join-Path $VscodeOutput "*") $TargetVscode -Recurse -Force
    Copy-Item (Join-Path $VscodeOutput "node.exe") (Join-Path $OutputRoot "lib/node.exe") -Force
    New-Item -ItemType Directory -Force (Join-Path $OutputRoot "src/browser") | Out-Null
    Copy-Item (Join-Path $CodeRoot "src/browser/*") (Join-Path $OutputRoot "src/browser") -Recurse -Force
    foreach ($name in @("LICENSE", "package.json", "package-lock.json")) {
        Copy-Item (Join-Path $CodeRoot $name) $OutputRoot -Force
    }
    Copy-Item (Join-Path $VscodeRoot "ThirdPartyNotices.txt") $OutputRoot -Force
    Push-Location $OutputRoot
    try {
        Invoke-Checked "npm.cmd" @("ci", "--omit=dev", "--ignore-scripts", "--no-audit", "--no-fund")
    }
    finally { Pop-Location }
    Write-Stamp
    Write-Host "Native Windows editor staged at $OutputRoot"
}
finally { Pop-Location }

if ($ComponentOnly) { Export-EditorComponent (Get-ComponentVersion) }

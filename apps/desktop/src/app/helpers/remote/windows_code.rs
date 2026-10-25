use crate::app::helpers::*;
use crate::*;
use std::path::Path;

/// CDXC:CodeEditor 2026-10-20 WHY:
/// A PowerShell remote runs the native editor its own Ghostex has; this computer never uploads one. Release builds download that editor on first use into the component store (`code-server\<version folder>\windows-native-<arch>`, see `source_code_server_local_component_platform`), development builds carry it beside the app. Try the folder the app recorded in `$gxAppDir` (the install that last ran), then the per-user install (`%LOCALAPPDATA%\Ghostex\current`), then Program Files, then the component store. When none has it the user opens the Code view on that computer once (`SOURCE_CODE_SERVER_REMOTE_EDITOR_MISSING_MESSAGE`). Supersedes the 2026-09-23 note that Windows ships the editor beside the app.
pub(crate) fn gpui_remote_windows_code_setup() -> String {
    format!(
        r#"{}
$gxApp=Split-Path (Split-Path (Split-Path $gxExe -Parent) -Parent) -Parent
$gxPF=$env:ProgramW6432
if (!$gxPF) {{ $gxPF=$env:ProgramFiles }}
$gxCodeCandidates=@()
if ($gxAppDir) {{ $gxCodeCandidates+=Join-Path $gxAppDir 'code-server' }}
$gxCodeCandidates+=@((Join-Path $env:LOCALAPPDATA 'Ghostex/current/code-server'), (Join-Path $gxPF 'Ghostex/code-server'), (Join-Path $gxApp 'code-server'), (Join-Path $gxData 'code-server/package'))
$gxCodeCandidates+=@(Get-ChildItem -Path (Join-Path $env:LOCALAPPDATA 'GhostexData/components/code-server/*/windows-native-*') -Directory -ErrorAction SilentlyContinue | ForEach-Object {{ $_.FullName }})
$gxCode=$gxCodeCandidates | Where-Object {{ (Test-Path -LiteralPath (Join-Path $_ 'lib/node.exe') -PathType Leaf) -and (Test-Path -LiteralPath (Join-Path $_ 'out/node/entry.js') -PathType Leaf) -and (Test-Path -LiteralPath (Join-Path $_ 'lib/vscode/out/server-main.js') -PathType Leaf) }} | Select-Object -First 1
if (!$gxCode) {{ throw 'The Windows Ghostex installation is missing its native Code editor. Open the Code view on that computer once to install it.' }}
"#,
        gpui_remote_windows_cli_setup()
    )
}

/// CDXC:PromptEditor 2026-09-23 WHY:
/// Windows SSH launches Code elevated; its default pipe ACL grants Administrators write access but excludes the account's non-elevated agent. Restrict the owned pipe to the account SID and SYSTEM so both tokens can connect.
/// Windows PowerShell supplies the framework PipeSecurity APIs; keeping this initializer small also respects OpenSSH's command-line limit.
///
/// CDXC:CodeEditor 2026-10-19 WHY:
/// Windows OpenSSH leaves the session's PowerShell running when the client disconnects, so waiting only for Code to exit kept it (and its port) alive for days after the computer that opened it was gone. The client keeps SSH stdin open for the editor's whole life after the script (`source_code_server_spawn_remote_runtime`), and this script ends Code when that stdin closes, the way `source_code_supervisor.js` does on Linux and macOS hosts.
///
/// The editor's launch key (CDXC:CodeEditor 2026-10-25 in helpers/source_server/code_server.rs) is set in the editor's own environment by this script, which reaches the remote over SSH input, so it never appears on a command line.
pub(crate) fn gpui_remote_windows_code_launch(project_path: &Path, runtime_key: &str) -> String {
    format!(
        r#"{}
$gxRuntime=Join-Path $gxData 'code-server/runtime'
$gxUserData=Join-Path $gxRuntime 'user-data'
$gxExtensions=Join-Path $gxRuntime 'extensions'
[IO.Directory]::CreateDirectory($gxUserData) | Out-Null
[IO.Directory]::CreateDirectory($gxExtensions) | Out-Null
$gxHash=[Security.Cryptography.SHA256]::Create()
try {{ $gxDigest=$gxHash.ComputeHash([Text.Encoding]::UTF8.GetBytes($gxUserData.Replace('/','\').ToLowerInvariant())) }} finally {{ $gxHash.Dispose() }}
$gxPipe='\\.\pipe\ghostex-code-'+([BitConverter]::ToString($gxDigest).Replace('-','').ToLowerInvariant())
$gxNode=Join-Path $gxCode 'lib/node.exe'
$gxEntry=Join-Path $gxCode 'out/node/entry.js'
$gxStart=[Diagnostics.ProcessStartInfo]::new()
$gxStart.FileName=$gxNode
$gxStart.WorkingDirectory={}
$gxStart.Arguments='"'+$gxEntry+'" --auth none --bind-addr 127.0.0.1:{} --disable-telemetry --disable-update-check --disable-workspace-trust --disable-getting-started-override --ignore-last-opened --app-name "ghostex Code" --user-data-dir "'+$gxUserData+'" --extensions-dir "'+$gxExtensions+'" --session-socket "'+$gxPipe+'"'
$gxStart.UseShellExecute=$false
$gxStart.CreateNoWindow=$true
$gxStart.EnvironmentVariables['GHOSTEX_CODE_SERVER_KEY']={}
$gxChild=[Diagnostics.Process]::Start($gxStart)
try {{
    & {{
{}
    }} -PipePath $gxPipe
    $gxInput=[Console]::OpenStandardInput()
    $gxByte=[byte[]]::new(1)
    $gxRead=$gxInput.ReadAsync($gxByte,0,1)
    while (!$gxChild.HasExited) {{
        if ($gxRead.Wait(500)) {{
            if ($gxRead.Result -eq 0) {{ break }}
            $gxRead=$gxInput.ReadAsync($gxByte,0,1)
        }}
    }}
    if (!$gxChild.HasExited) {{ $gxChild.Kill(); $gxChild.WaitForExit() }}
    exit $gxChild.ExitCode
}} finally {{ if (!$gxChild.HasExited) {{ $gxChild.Kill() }} }}
"#,
        gpui_remote_windows_code_setup(),
        gpui_powershell_quote(&project_path.to_string_lossy()),
        SOURCE_CODE_SERVER_REMOTE_PORT,
        gpui_powershell_quote(runtime_key),
        include_str!("windows_code_pipe.ps1")
    )
}

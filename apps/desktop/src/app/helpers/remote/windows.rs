use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, write::GzEncoder};
use std::io::Write;

/// The inside of a PowerShell single-quoted string for `value`: every character PowerShell reads as
/// a single quote (U+0027 and the typographic U+2018, U+2019, U+201A, U+201B) is doubled, so a
/// path or name holding a curly quote cannot close the literal.
/// CDXC:PlatformSupport 2026-10-25 SEE-ALSO:
/// server/src/platform/shell.rs `powershell_single_quoted_content` (gxserver) and apps/mobile/app/src/remote/powershell.ts (phone) must double the same five characters.
pub(crate) fn gpui_powershell_single_quoted_content(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        if matches!(ch, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            escaped.push(ch);
        }
        escaped.push(ch);
    }
    escaped
}

pub(crate) fn gpui_powershell_quote(value: &str) -> String {
    format!("'{}'", gpui_powershell_single_quoted_content(value))
}

pub(crate) const GPUI_REMOTE_POWERSHELL_PRELUDE: &str = "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Console]::InputEncoding=[Text.UTF8Encoding]::new($false); [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); $OutputEncoding=[Console]::OutputEncoding; ";

/// CDXC:RemoteMachines 2026-09-14 WHY:
/// Native Windows OpenSSH can use cmd or PowerShell as its login shell and limits command-line length.
/// Compress the script before encoding it so both shells see a fixed invocation and attachment stdin remains available to the terminal or upload.
pub(crate) fn gpui_remote_powershell_command(script: &str) -> String {
    let mut compressed = GzEncoder::new(Vec::new(), Compression::fast());
    compressed
        .write_all(script.as_bytes())
        .expect("compress remote script");
    let payload = STANDARD.encode(compressed.finish().expect("finish remote script"));
    let bootstrap = format!(
        "{GPUI_REMOTE_POWERSHELL_PRELUDE}$gxBytes=[Convert]::FromBase64String('{payload}'); $gxStream=[IO.MemoryStream]::new($gxBytes); $gxZip=[IO.Compression.GZipStream]::new($gxStream,[IO.Compression.CompressionMode]::Decompress); $gxReader=[IO.StreamReader]::new($gxZip,[Text.Encoding]::UTF8); $gxScript=$gxReader.ReadToEnd(); $gxReader.Dispose(); & ([ScriptBlock]::Create($gxScript))"
    );
    let encoded = STANDARD.encode(
        bootstrap
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    format!("powershell.exe -NoLogo -NoProfile -EncodedCommand {encoded}")
}

pub(crate) const GPUI_REMOTE_WINDOWS_STORAGE: &str = r#"
$gxData=Join-Path $env:LOCALAPPDATA 'Ghostex/Data'
$gxState=Join-Path $env:LOCALAPPDATA 'Ghostex/State'
$gxConfig=Join-Path $env:APPDATA 'Ghostex'
if ($env:GHOSTEX_HOME -and [IO.Path]::IsPathRooted($env:GHOSTEX_HOME)) {
    $gxData=$env:GHOSTEX_HOME; $gxState=Join-Path $env:GHOSTEX_HOME 'state'; $gxConfig=$env:GHOSTEX_HOME
}
"#;

pub(crate) fn gpui_remote_windows_cli_setup() -> String {
    format!(
        r#"{GPUI_REMOTE_WINDOWS_STORAGE}
$gxAppDir=$null
$gxAppDirFile=Join-Path $gxData 'gxserver/windows-app-dir'
if (Test-Path -LiteralPath $gxAppDirFile -PathType Leaf) {{ $gxAppDir=(Get-Content -LiteralPath $gxAppDirFile -Raw).Trim() }}
$gxCandidates=@((Join-Path $gxData 'gxserver/package/bin/ghostex.exe'))
if ($gxAppDir) {{ $gxCandidates+=Join-Path $gxAppDir 'resources/native/ghostex.exe' }}
$gxCandidates+=@((Join-Path $env:LOCALAPPDATA 'Ghostex/current/resources/native/ghostex.exe'), (Join-Path $env:ProgramFiles 'Ghostex/resources/native/ghostex.exe'), (Join-Path $env:USERPROFILE '.local/bin/ghostex.exe'))
$gxCommand=Get-Command ghostex.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if ($gxCommand) {{ $gxCandidates+=$gxCommand.Source }}
$gxExe=$gxCandidates | Where-Object {{ Test-Path -LiteralPath $_ -PathType Leaf }} | Select-Object -First 1
if (!$gxExe) {{ exit 127 }}
"#
    )
}

pub(crate) fn gpui_remote_windows_environment_probe() -> String {
    format!(
        r#"{GPUI_REMOTE_WINDOWS_STORAGE}
$gxSettings=$null
$gxSettingsPath=Join-Path $gxConfig 'native-sidebar-settings.json'
if (Test-Path -LiteralPath $gxSettingsPath) {{ $gxSettings=Get-Content -LiteralPath $gxSettingsPath -Raw | ConvertFrom-Json }}
$gxBackend='powershell'
if ($gxSettings.windowsTerminalBackend -eq 'wsl') {{ $gxBackend='wsl' }}
Write-Output '__GHOSTEX_REMOTE_WINDOWS__'
Write-Output ('__GHOSTEX_REMOTE_ENV__'+(@{{backend=$gxBackend; distribution=$gxSettings.windowsWslDistribution}} | ConvertTo-Json -Compress))
"#
    )
}

pub(crate) fn gpui_remote_windows_token_read_command() -> String {
    format!(
        r#"{}
& $gxExe server start --json | Out-Null
if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}
$gxToken=Join-Path $gxState 'gxserver/auth/token'
if (!(Test-Path -LiteralPath $gxToken)) {{ exit 126 }}
Write-Output '__GHOSTEX_REMOTE_TOKEN_START__'
[Console]::Write([IO.File]::ReadAllText($gxToken))
Write-Output "`n__GHOSTEX_REMOTE_TOKEN_END__"
"#,
        gpui_remote_windows_cli_setup()
    )
}

pub(crate) const GPUI_REMOTE_WINDOWS_PLATFORM_PROBE: &str = r#"
Write-Output '__GHOSTEX_REMOTE_PLATFORM_START__'
Write-Output 'windows'
Write-Output $env:PROCESSOR_ARCHITECTURE
Write-Output ''
Write-Output '__GHOSTEX_REMOTE_PLATFORM_END__'
"#;

pub(crate) const GPUI_REMOTE_WINDOWS_PORTS: &str = r#"
Get-NetTCPConnection -State Listen | ForEach-Object {
    Write-Output ('p'+$_.OwningProcess)
    $gxProcess=Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue
    if ($gxProcess) { Write-Output ('c'+$gxProcess.ProcessName) }
    Write-Output ('n['+$_.LocalAddress+']:'+$_.LocalPort)
}
"#;

pub(crate) fn gpui_remote_token_read_command_for(
    target: &super::types::GpuiRemoteExecutionTarget,
) -> String {
    if matches!(
        target,
        super::types::GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        gpui_remote_windows_token_read_command()
    } else {
        super::connect::gpui_remote_token_read_command().to_string()
    }
}

pub(crate) fn gpui_remote_platform_probe_command_for(
    target: &super::types::GpuiRemoteExecutionTarget,
) -> &'static str {
    if matches!(
        target,
        super::types::GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        GPUI_REMOTE_WINDOWS_PLATFORM_PROBE
    } else {
        super::install::gpui_remote_install_target_probe_command()
    }
}

pub(crate) fn gpui_remote_ports_command_for(
    target: &super::types::GpuiRemoteExecutionTarget,
) -> &'static str {
    if matches!(
        target,
        super::types::GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        GPUI_REMOTE_WINDOWS_PORTS
    } else {
        super::ports::GPUI_REMOTE_LISTENING_PORTS_COMMAND
    }
}

pub(crate) fn gpui_remote_windows_upload_command(name: &str, folder: bool) -> String {
    let operation = if folder {
        r#"$archive=$gxPath+'.tar.gz'
$output=[IO.File]::Open($archive,[IO.FileMode]::CreateNew)
try { [Console]::OpenStandardInput().CopyTo($output) } finally { $output.Dispose() }
try {
  [IO.Directory]::CreateDirectory($gxPath) | Out-Null
  & tar.exe -xzf $archive -C $gxPath --strip-components=1
  if ($LASTEXITCODE -ne 0) { throw 'Could not extract uploaded folder.' }
} finally { Remove-Item -LiteralPath $archive -Force }
"#
    } else {
        r#"$output=[IO.File]::Open($gxPath,[IO.FileMode]::CreateNew)
try { [Console]::OpenStandardInput().CopyTo($output) } finally { $output.Dispose() }
"#
    };
    format!(
        "$gxDir=Join-Path ([IO.Path]::GetTempPath()) 'ghostex-gpui-attachments'; [IO.Directory]::CreateDirectory($gxDir) | Out-Null; $gxPath=Join-Path $gxDir {};\n{operation}\nWrite-Output ('__GHOSTEX_UPLOAD__'+$gxPath)",
        gpui_powershell_quote(name)
    )
}

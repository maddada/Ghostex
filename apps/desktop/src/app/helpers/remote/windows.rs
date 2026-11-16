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

/// The longest command `gpui_remote_powershell_command` sends without the gzip loader: cmd.exe,
/// which Windows OpenSSH runs commands through unless `DefaultShell` names another shell, refuses
/// a command line over 8,191 characters.
const GPUI_REMOTE_POWERSHELL_PLAIN_COMMAND_MAX_CHARS: usize = 8_000;

fn gpui_powershell_encoded_command(script: &str) -> String {
    let encoded = STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    format!("powershell.exe -NoLogo -NoProfile -EncodedCommand {encoded}")
}

/// CDXC:RemoteMachines 2026-09-14 WHY:
/// Native Windows OpenSSH can use cmd or PowerShell as its login shell and limits command-line length.
/// `-EncodedCommand` gives both shells a fixed invocation with no quoting and leaves stdin free for the terminal or an upload.
///
/// CDXC:RemoteMachines 2026-11-14 WHY (GitHub #613):
/// Microsoft Defender's AMSI scan reports "base64 text, gunzipped in memory, run through
/// `[ScriptBlock]::Create`" as `VirTool:PowerShell/MaleficAms.W` and blocks it before anything runs,
/// so every terminal attach to a Windows host with Defender on failed, and the reconnect loop
/// raised an alert every few seconds. A script that fits encodes as itself, which Defender reads as
/// the plain command it is; only a script too long for cmd.exe's command line still goes through
/// the gzip loader. The terminal attach is a few hundred characters and never takes the loader.
pub(crate) fn gpui_remote_powershell_command(script: &str) -> String {
    let plain =
        gpui_powershell_encoded_command(&format!("{GPUI_REMOTE_POWERSHELL_PRELUDE}{script}"));
    if plain.len() <= GPUI_REMOTE_POWERSHELL_PLAIN_COMMAND_MAX_CHARS {
        return plain;
    }
    let mut compressed = GzEncoder::new(Vec::new(), Compression::fast());
    compressed
        .write_all(script.as_bytes())
        .expect("compress remote script");
    let payload = STANDARD.encode(compressed.finish().expect("finish remote script"));
    gpui_powershell_encoded_command(&format!(
        "{GPUI_REMOTE_POWERSHELL_PRELUDE}$gxBytes=[Convert]::FromBase64String('{payload}'); $gxStream=[IO.MemoryStream]::new($gxBytes); $gxZip=[IO.Compression.GZipStream]::new($gxStream,[IO.Compression.CompressionMode]::Decompress); $gxReader=[IO.StreamReader]::new($gxZip,[Text.Encoding]::UTF8); $gxScript=$gxReader.ReadToEnd(); $gxReader.Dispose(); & ([ScriptBlock]::Create($gxScript))"
    ))
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

#[cfg(test)]
mod powershell_command_tests {
    use super::*;

    fn decoded_script(command: &str) -> String {
        let encoded = command
            .strip_prefix("powershell.exe -NoLogo -NoProfile -EncodedCommand ")
            .expect("a fixed powershell.exe -EncodedCommand invocation");
        let bytes = STANDARD.decode(encoded).expect("base64");
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&units).expect("UTF-16LE")
    }

    /// GitHub #613: Defender blocks the gzip + `ScriptBlock::Create` loader as
    /// `VirTool:PowerShell/MaleficAms.W`, so the terminal attach must reach the host as the plain
    /// command it is.
    #[test]
    fn a_terminal_attach_is_sent_as_its_own_plain_script() {
        let attach = format!(
            "[Console]::Write([string][char]27 + ']2;{}' + [char]7); $env:WMX_DIR='C:\\Users\\Some One\\AppData\\Local\\Ghostex\\Runtime\\windows-sessions'; & 'C:\\Users\\Some One\\AppData\\Local\\Ghostex\\current\\resources\\native\\wmx.exe' attach --require-existing 'S27-P9m6x-G94id' --prompt-editor 'code-server'",
            super::super::ssh_process::TEMP_REMOTE_SSH_READY_TITLE
        );
        let command = gpui_remote_powershell_command(&attach);
        let script = decoded_script(&command);
        assert_eq!(script, format!("{GPUI_REMOTE_POWERSHELL_PRELUDE}{attach}"));
        for loader_part in [
            "FromBase64String",
            "GZipStream",
            "ScriptBlock",
            "MemoryStream",
        ] {
            assert!(!script.contains(loader_part), "{loader_part} in {script}");
        }
        assert!(command.len() <= GPUI_REMOTE_POWERSHELL_PLAIN_COMMAND_MAX_CHARS);
    }

    #[test]
    fn only_a_script_too_long_for_cmd_goes_through_the_gzip_loader() {
        let long = "Write-Output 'x'\n".repeat(400);
        let command = gpui_remote_powershell_command(&long);
        assert!(decoded_script(&command).contains("GZipStream"));
        assert!(command.len() <= GPUI_REMOTE_POWERSHELL_PLAIN_COMMAND_MAX_CHARS);
        // The setup probes the connection runs on every connect stay plain too.
        for script in [
            gpui_remote_windows_environment_probe(),
            gpui_remote_windows_token_read_command(),
            GPUI_REMOTE_WINDOWS_PLATFORM_PROBE.to_string(),
        ] {
            assert!(
                !decoded_script(&gpui_remote_powershell_command(&script)).contains("GZipStream")
            );
        }
    }
}

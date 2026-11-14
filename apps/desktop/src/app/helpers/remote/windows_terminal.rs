//! Windows-local terminal launch programs for remote SSH sessions.

use crate::app::helpers::*;

fn windows_process_argument(argument: &str) -> String {
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        quoted.extend(std::iter::repeat_n(
            '\\',
            if character == '"' {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        quoted.push(character);
        slashes = 0;
    }
    quoted.extend(std::iter::repeat_n('\\', slashes * 2));
    quoted.push('"');
    quoted
}

/// CDXC:RemoteMachines 2026-09-23 WHY:
/// Windows PowerShell's native argument conversion can strip quotes inside the remote shell program. Supply a fully quoted Windows command line to Start-Process so paths and scripts arrive at OpenSSH byte-for-byte while stdin and stdout stay attached to ConPTY.
pub(crate) fn gpui_windows_remote_ssh_terminal_command(arguments: &[String]) -> String {
    let arguments = arguments
        .iter()
        .map(|argument| windows_process_argument(argument))
        .collect::<Vec<_>>()
        .join(" ");
    gpui_remote_powershell_command(&format!(
        "$gxStart=@{{FilePath={};ArgumentList={};NoNewWindow=$true;PassThru=$true;Wait=$true}}; if ($env:GHOSTEX_REMOTE_SSH_STDERR) {{ $gxStart.RedirectStandardError=$env:GHOSTEX_REMOTE_SSH_STDERR }}; $gxSsh=Start-Process @gxStart; exit $gxSsh.ExitCode",
        gpui_powershell_quote(&gpui_remote_ssh_executable()),
        gpui_powershell_quote(&arguments),
    ))
}

pub(crate) fn gpui_remote_attach_terminal_process_command(
    ssh_command: &str,
    ssh_host: &str,
    ssh_port: Option<u16>,
) -> String {
    let script = r#"
$gxHost=__HOST__
$gxPort=__PORT__
function Test-GxRemote {
    if ($gxHost.StartsWith('100.')) {
        $tailscale=Get-Command tailscale.exe -CommandType Application -ErrorAction SilentlyContinue
        if ($tailscale) { & $tailscale.Source ping --c 1 --timeout 2s $gxHost *> $null; return $LASTEXITCODE -eq 0 }
    }
    $client=[Net.Sockets.TcpClient]::new()
    try { $pending=$client.ConnectAsync($gxHost,$gxPort); return $pending.Wait(3000) -and $client.Connected }
    catch { return $false }
    finally { $client.Dispose() }
}
$gxError=[IO.Path]::GetTempFileName()
$env:GHOSTEX_REMOTE_SSH_STDERR=$gxError
$gxAuthFailures=0
$gxFastFailures=0
# GitHub #613: fast failures back off 2, 4, 8, 16 then 30 s and the sixth in a row parks until
# Enter; see gpui_remote_attach_terminal_process_command in attach_terminal_process.rs.
$gxFastFailureCap=6
try {
    while ($true) {
        $gxStarted=[DateTime]::UtcNow
        __SSH__
        $gxExit=$LASTEXITCODE
        $gxDuration=([DateTime]::UtcNow-$gxStarted).TotalSeconds
        $gxErrorText=[IO.File]::ReadAllText($gxError)
        if ($gxErrorText) { [Console]::Error.Write($gxErrorText) }
        if ($gxExit -eq 255 -and $gxErrorText.Contains('Permission denied (')) {
            $gxAuthFailures++
            [Console]::WriteLine("`nRemote SSH rejected the login (attempt $gxAuthFailures of 3).")
            if ($gxAuthFailures -ge 3) {
                [Console]::WriteLine('Auto-reconnect cannot log back in. Click this session in the Ghostex sidebar to recover it, or press Enter to retry. Ctrl+C stops this terminal.')
                if ($null -eq [Console]::ReadLine()) { Start-Sleep -Seconds 30 }
                $gxAuthFailures=0
            } else { Start-Sleep -Seconds 30 }
            continue
        }
        $gxAuthFailures=0
        if ($gxDuration -ge 8) {
            $gxFastFailures=0
            [Console]::WriteLine("`nRemote attach ended (exit $gxExit). Reconnecting...")
        } else {
            $gxFastFailures++
            if ($gxFastFailures -ge $gxFastFailureCap) {
                [Console]::WriteLine("`nGhostex stopped reconnecting this terminal: the remote attach failed $gxFastFailures times in a row,")
                [Console]::WriteLine("each time within seconds of connecting to $gxHost (last exit code $gxExit).")
                [Console]::WriteLine('The remote machine refused or ended the attach command; its own message is above')
                [Console]::WriteLine('(for example, antivirus blocking the command). Retrying would only repeat it.')
                [Console]::WriteLine('Press Enter to try again. Ctrl+C stops this terminal.')
                if ($null -eq [Console]::ReadLine()) { Start-Sleep -Seconds 600 }
                $gxFastFailures=$gxFastFailureCap-1
            } else {
                $gxWait=30
                if ($gxFastFailures -lt 5) { $gxWait=@(0,2,4,8,16)[$gxFastFailures] }
                [Console]::WriteLine("`nRemote attach ended (exit $gxExit) within seconds. Retrying in ${gxWait}s (attempt $($gxFastFailures+1) of $gxFastFailureCap)...")
                Start-Sleep -Seconds $gxWait
            }
        }
        while (!(Test-GxRemote)) { Start-Sleep -Seconds 2 }
        Start-Sleep -Seconds 1
    }
} finally {
    Remove-Item -LiteralPath $gxError -Force -ErrorAction SilentlyContinue
    Remove-Item Env:GHOSTEX_REMOTE_SSH_STDERR -ErrorAction SilentlyContinue
}
"#
        .replace("__HOST__", &gpui_powershell_quote(ssh_host))
        .replace("__PORT__", &ssh_port.unwrap_or(22).to_string())
        .replace("__SSH__", ssh_command);
    // The native terminal backend supplies -EncodedCommand. Wrapping this local
    // program in another encoded PowerShell process expands ordinary SSH attach
    // commands past Windows' process command-line limit before ConPTY can start.
    script
}

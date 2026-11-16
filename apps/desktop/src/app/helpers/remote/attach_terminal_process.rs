// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote attach terminal process
// command construction. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;

pub(crate) fn gpui_remote_shell_command_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "_/:=@%+.,-".contains(ch))
    {
        value.to_string()
    } else {
        gpui_shell_single_quote(value)
    }
}

#[cfg(not(windows))]
pub(crate) fn gpui_remote_attach_terminal_process_command(
    ssh_command: &str,
    ssh_host: &str,
    ssh_port: Option<u16>,
) -> String {
    /*
    CDXC:RemoteMachines 2026-08-13:
    Remote-attach terminals must never degrade to a vanilla local shell when
    the SSH transport drops (lid close, wifi loss, captive portal, transit).
    The launch wrapper keeps the pane attached to the remote session by
    reconnecting after every attach exit. Each retry is gated on a cheap
    reachability probe so a network outage is handled gracefully instead of
    burning full SSH handshakes; the probe polls at a flat cadence and never
    parks on a network outage, while closing the tab or quitting terminates the
    loop. The remote zmx session persists server-side
    independently, and re-running `ghostex attach` revives a missing provider
    before attaching, so reconnection restores the session and its scrollback.

    For Tailscale CGNAT (100.x) targets `tailscale ping` is the authoritative
    "WireGuard handshake is live" signal and cuts through captive-portal
    ambiguity; other hosts fall back to a bounded TCP connect probe, and the
    absence of either tool degrades to letting SSH's ConnectTimeout bound each
    attempt. The probe target and ssh command stay process-local; no host,
    token, command, or path crosses the CEF boundary.
    */
    let quoted_host = gpui_shell_single_quote(ssh_host);
    let port_assignment = match ssh_port {
        Some(port) => format!(
            "__gx_attach_port={}\n",
            gpui_shell_single_quote(&port.to_string())
        ),
        None => String::new(),
    };
    let probe_lines = [
        "__gx_probe() {",
        "  case \"$__gx_attach_host\" in 100.*)",
        "    __gx_ts=\"\"",
        "    if command -v tailscale >/dev/null 2>&1; then __gx_ts=tailscale",
        "    elif [ -x /Applications/Tailscale.app/Contents/MacOS/Tailscale ]; then __gx_ts=/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        "    fi",
        "    if [ -n \"$__gx_ts\" ]; then",
        "      \"$__gx_ts\" ping --c 1 --timeout 2s \"$__gx_attach_host\" >/dev/null 2>&1 && return 0",
        "      return 1",
        "    fi",
        "    ;;",
        "  esac",
        "  if command -v nc >/dev/null 2>&1; then",
        "    __gx_p=\"${__gx_attach_port:-22}\"",
        "    nc -z -w 3 \"$__gx_attach_host\" \"$__gx_p\" >/dev/null 2>&1 && return 0",
        "    return 1",
        "  fi",
        "  return 0",
        "}",
    ];
    /*
    CDXC:RemoteMachines 2026-08-14:
    Probe polling stays at a flat 2s so a returning link reconnects within
    ~2 seconds; only the probe runs while the network is down (one cheap
    process + one packet), so this cadence cannot flood anything. Network
    outages never park: a 2s probe is one packet and ~50ms of CPU, and a
    system-suspended wrapper consumes nothing (the process is frozen), so
    probing stays effectively free no matter how long the laptop sleeps or
    stays offline. A wall-clock budget is wrong here because sleep time would
    count against it and park the loop the moment the lid opens. Auth
    rejections keep their own hard 30s/3-attempt budget.

    CDXC:RemoteMachines 2026-11-14 (GitHub #613):
    An attach that fails within seconds against a reachable host fails the
    same way next time: the remote refuses the command (Defender blocked the
    Windows attach as malware), the session is gone, or the remote Ghostex is
    broken. Retrying it every 5s without end ran the failing command thousands
    of times a day on the remote, for every session open on this computer,
    terminal shown or not. Fast failures back off 2, 4, 8, 16 then 30s, and
    the sixth in a row parks the wrapper with a message naming why it stopped
    until the user presses Enter. A manual retry that fails fast parks again
    at once; an attach that lasts resets the count.
    */
    let backoff_lines = [
        "__gx_fastfail_cap=6",
        "__gx_backoff_seconds() {",
        "  case \"$1\" in 1) echo 2 ;; 2) echo 4 ;; 3) echo 8 ;; 4) echo 16 ;; *) echo 30 ;; esac",
        "}",
    ];
    /*
    CDXC:RemoteMachines 2026-08-14:
    The attach command's stderr is streamed live through tee while also being
    captured so each reconnect attempt can be classified. An SSH transport
    authentication rejection never rapid-retries: it backs off hard for three
    attempts and then parks with guidance to use the sidebar recovery (the
    app-owned re-prepare path re-arms askpass/credentials correctly), because
    a wrapper-side retry cannot re-read the Keychain. Fast non-auth failures
    (< 8s) escalate through the backoff ladder above and park at its cap, so a
    reachable-but-failing path can never hammer the remote sshd; only a real
    session drop resets the failure counters.
    */
    let attach_lines = [
        "__gx_attach() {".to_string(),
        format!("  {} 2> >(tee \"$__gx_err\" >&2)", ssh_command),
        "}".to_string(),
    ];
    let loop_lines = [
        "__gx_err=\"$(mktemp \"${TMPDIR:-/tmp}/gx-reattach.XXXXXX\")\"",
        "__gx_cleanup() { rm -f \"$__gx_err\"; }",
        "trap __gx_cleanup EXIT",
        "trap 'exit 129' HUP",
        "trap 'exit 130' INT",
        "trap 'exit 143' TERM",
        "__gx_authfails=0",
        "__gx_fastfails=0",
        "while true; do",
        "  __gx_started=$(date +%s)",
        "  __gx_attach",
        "  __gx_exit=$?",
        "  __gx_dur=$(( $(date +%s) - __gx_started ))",
        "  if [ \"$__gx_exit\" -eq 255 ] && grep -q \"Permission denied (\" \"$__gx_err\" 2>/dev/null; then",
        "    __gx_authfails=$((__gx_authfails + 1))",
        "    printf '\\nRemote SSH rejected the login (attempt %s of 3).\\n' \"$__gx_authfails\"",
        "    if [ \"$__gx_authfails\" -ge 3 ]; then",
        "      printf '\\nAuto-reconnect cannot log back in - the saved login is not accepted.\\n'",
        "      printf 'Click this session in the Ghostex sidebar to recover it,\\n'",
        "      printf 'or press Enter to retry. Ctrl+C stops this terminal.\\n'",
        "      if ! read -s -r __gx_junk; then sleep 30; fi",
        "      __gx_authfails=0",
        "    else",
        "      sleep 30",
        "    fi",
        "    continue",
        "  fi",
        "  __gx_authfails=0",
        "  if [ \"$__gx_dur\" -ge 8 ]; then",
        "    __gx_fastfails=0",
        "    printf '\\nRemote attach ended (exit %s). Reconnecting...\\n' \"$__gx_exit\"",
        "  else",
        "    __gx_fastfails=$((__gx_fastfails + 1))",
        "    if [ \"$__gx_fastfails\" -ge \"$__gx_fastfail_cap\" ]; then",
        "      printf '\\nGhostex stopped reconnecting this terminal: the remote attach failed %s times in a row,\\n' \"$__gx_fastfails\"",
        "      printf 'each time within seconds of connecting to %s (last exit code %s).\\n' \"$__gx_attach_host\" \"$__gx_exit\"",
        "      printf 'The remote machine refused or ended the attach command; its own message is above\\n'",
        "      printf '(for example, antivirus blocking the command). Retrying would only repeat it.\\n'",
        "      printf 'Press Enter to try again. Ctrl+C stops this terminal.\\n'",
        "      if ! read -s -r __gx_junk; then sleep 600; fi",
        "      __gx_fastfails=$((__gx_fastfail_cap - 1))",
        "    else",
        "      __gx_wait=$(__gx_backoff_seconds \"$__gx_fastfails\")",
        "      printf '\\nRemote attach ended (exit %s) within seconds. Retrying in %ss (attempt %s of %s)...\\n' \"$__gx_exit\" \"$__gx_wait\" \"$((__gx_fastfails + 1))\" \"$__gx_fastfail_cap\"",
        "      sleep \"$__gx_wait\"",
        "    fi",
        "  fi",
        "  while ! __gx_probe; do",
        "    sleep 2",
        "  done",
        "  sleep 1",
        "done",
    ];
    let mut body = String::new();
    body.push_str(&format!(
        "printf '\\033]2;{TEMP_REMOTE_LOCAL_READY_TITLE}\\007'\n"
    ));
    body.push_str(&format!("__gx_attach_host={}\n", quoted_host));
    body.push_str(&port_assignment);
    for line in probe_lines
        .iter()
        .copied()
        .chain(backoff_lines.iter().copied())
        .chain(attach_lines.iter().map(String::as_str))
        .chain(loop_lines.iter().copied())
    {
        body.push_str(line);
        body.push('\n');
    }
    let shell = if cfg!(target_os = "linux") {
        "/bin/bash"
    } else {
        "/bin/zsh"
    };
    format!("{shell} -c {}", gpui_shell_single_quote(body.trim_end()))
}

#[cfg(all(test, not(windows)))]
mod reconnect_cap_tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// GitHub #613: an attach the remote refuses at once must back off and then stop with a
    /// message, never retry every few seconds for as long as the session stays open.
    #[test]
    fn an_attach_that_fails_at_once_backs_off_then_parks_with_the_reason() {
        let work = std::env::temp_dir().join(format!(
            "gx-reconnect-cap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&work).unwrap();
        let attempts = work.join("attempts");
        // The probe needs a reachable port; the fake attach stubs `sleep` so the ladder runs
        // instantly, records each attempt, fails at once, and ends the wrapper at the 8th.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let fake_ssh = format!(
            "sleep() {{ echo \"slept $1\" >> {log}; }}; echo attempt >> {log}; [ \"$(grep -c attempt {log})\" -ge 8 ] && exit 99; false",
            log = gpui_shell_single_quote(&attempts.to_string_lossy())
        );
        let command =
            gpui_remote_attach_terminal_process_command(&fake_ssh, "127.0.0.1", Some(port));
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let log = std::fs::read_to_string(&attempts).unwrap_or_default();
        std::fs::remove_dir_all(&work).ok();
        drop(listener);

        assert_eq!(log.matches("attempt").count(), 8, "{log}\n{stdout}");
        // Back off 2, 4, 8, 16, 30 s between the first six attempts, then park instead of a 7th
        // 30 s retry; with no terminal input the park waits ten minutes, not seconds.
        let sleeps = log
            .lines()
            .filter_map(|line| line.strip_prefix("slept "))
            .filter(|seconds| *seconds != "1")
            .collect::<Vec<_>>();
        assert_eq!(sleeps, ["2", "4", "8", "16", "30", "600", "600"], "{log}");
        assert!(
            stdout.contains("Retrying in 2s (attempt 2 of 6)"),
            "{stdout}"
        );
        assert!(
            stdout.contains("Retrying in 30s (attempt 6 of 6)"),
            "{stdout}"
        );
        // A manual retry that fails fast parks again at once instead of restarting the ladder.
        assert_eq!(
            stdout
                .matches("Ghostex stopped reconnecting this terminal: the remote attach failed")
                .count(),
            2,
            "{stdout}"
        );
        assert!(
            stdout.contains("within seconds of connecting to 127.0.0.1"),
            "{stdout}"
        );
        assert!(stdout.contains("Press Enter to try again."), "{stdout}");
    }
}

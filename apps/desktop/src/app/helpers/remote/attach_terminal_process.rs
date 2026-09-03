// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote attach terminal process
// command construction. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

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
    process + one packet), so this cadence cannot flood anything. The backoff
    ladder applies only to fast ssh failures against a reachable host
    (2s steady, 5s cap) and auth rejections keep their own hard 30s/3-attempt
    budget. Network outages never park: a 2s probe is one packet and ~50ms of
    CPU, and a system-suspended wrapper consumes nothing (the process is
    frozen), so probing stays effectively free no matter how long the laptop
    sleeps or stays offline. A wall-clock budget is wrong here because sleep
    time would count against it and park the loop the moment the lid opens.
    Only auth rejections park, because they require a human or the app's
    recovery flow.
    */
    let backoff_lines = [
        "__gx_backoff() {",
        "  case \"$1\" in 0|1|2|3) sleep 2 ;; *) sleep 5 ;; esac",
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
    (< 8s) escalate through the same backoff ladder as the probe loop so a
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
        "  printf '\\nRemote attach ended (exit %s). Reconnecting...\\n' \"$__gx_exit\"",
        "  if [ \"$__gx_dur\" -ge 8 ]; then",
        "    __gx_fastfails=0",
        "  else",
        "    __gx_fastfails=$((__gx_fastfails + 1))",
        "  fi",
        "  if [ \"$__gx_fastfails\" -gt 0 ]; then __gx_backoff \"$__gx_fastfails\"; fi",
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
    format!("/bin/zsh -c {}", gpui_shell_single_quote(body.trim_end()))
}

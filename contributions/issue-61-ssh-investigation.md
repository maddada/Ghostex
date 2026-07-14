# Issue #61 — SSH to another macOS account on the same Mac: investigation

**Symptom:** SSH from Ghostex fails when connecting to a different macOS user account on the same Mac, even though the equivalent `ssh` command works from a normal terminal.

**Where the code lives:** the remote-machine connect flow is native-owned (Swift), in
`native/macos/ghostexHost/Sources/ghostexHost/RemoteGxserverClient.swift`. Connecting to "another account on the same Mac" is done by adding a Remote machine whose host is the Mac itself and whose user is the other account. All SSH is run non-interactively from the GUI helper process `ghostexHost`.

## How Ghostex builds the SSH command

- `sshClientOptions` (≈L1361): `-o UseKeychain=yes -o AddKeysToAgent=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new`.
- Auth policy branch:
  - **Password machine** (a password was saved in Keychain): `BatchMode=no`, `NumberOfPasswordPrompts=1`, `PreferredAuthentications=publickey,password,keyboard-interactive`, and an **askpass** helper that reads the password from the macOS Keychain.
  - **Key-only machine** (no saved password): **`BatchMode=yes`**.
- `sshTargetArguments` (≈L1540): adds `-i <identityFile>` (if set), `-p <port>` (if set), then `user@host`. **Identity, port, and user are all wired correctly** — this is not the bug.
- Remote command runs under a login+interactive shell: `zsh -lic '<cmd>'` (to pick up Homebrew/mise PATH).
- `processLaunchInputIsSafe` (≈L1517): only rejects NUL bytes. **Not over-strict** — not the bug.

## Root cause(s)

### 1. Key-only machines force `BatchMode=yes` (primary)
Connecting to another local account almost always authenticates with **that account's password** — key-based auth between local accounts is uncommon. With `BatchMode=yes`, ssh disables every interactive method, so it returns `Permission denied` immediately, without prompting. A terminal, having no `BatchMode`, prompts for the password and succeeds. This is the direct explanation for "works in terminal, fails in Ghostex."

The only route to password auth today is to pre-save a password (which turns the machine into a "password machine"). If the user hasn't saved one, there is no path to password auth at all.

> The `BatchMode=yes` fast-fail is **intentional** (documented at ≈L1370): a TTY-less GUI ssh must not hang on a prompt it cannot service. So this is a design trade-off, not an accidental bug — changing it needs a maintainer decision (see options below).

### 2. GUI environment lacks custom SSH-agent sockets (secondary)
For key-only machines, the ssh subprocess inherits `ghostexHost`'s environment (`environment = nil` → inherit). A GUI app launched by LaunchServices/launchd does **not** get variables set only in shell rc files. Third-party agents (1Password, gpg-agent) that export `SSH_AUTH_SOCK` from `~/.zshrc` are therefore invisible to Ghostex, so agent-backed keys that work in a terminal are never offered. (macOS's built-in launchd ssh-agent socket is usually present, so default keychain keys still work.)

### 3. Opaque error messages (fixed here)
`sanitizedProcessFailure` mapped `permission denied` → a generic "SSH authentication failed," with no cause and no next step. This failed acceptance criterion (b): "Connection errors are shown clearly enough to diagnose auth or config issues."

## What was changed in this pass (low-risk, no behavior change)

`sanitizedProcessFailure` now maps stderr to specific, actionable messages:

| stderr contains | message |
|---|---|
| remote host identification has changed | host key changed → verify + clear `~/.ssh/known_hosts` |
| host key verification failed | connect once from a terminal to record the host key |
| **permission denied** | **save the machine's SSH password (common for another local account), or add a key the account accepts** |
| connection refused | enable Remote Login (Sharing) and check the port |
| could not resolve hostname | check host/IP |
| no route to host / network unreachable | check connectivity |
| timed out | (unchanged) |

This makes the failure diagnosable and, crucially, points a user connecting to another local account at the exact fix (save a password for the machine). It changes only the human-readable message string.

## Recommended follow-ups (need a maintainer decision + repro data)

1. **Auth for key-only machines.** Decide whether to let a key-only machine fall back to a saved-password / askpass flow automatically, or to prompt the user to save a password when the first key-only attempt returns `Permission denied`. Do **not** simply drop `BatchMode` (would hang a TTY-less ssh). Preferred: on `Permission denied` for a key-only machine, surface a "This machine needs a password — save one?" affordance.
2. **Custom SSH agents.** Consider sourcing the user's login-shell `SSH_AUTH_SOCK` (e.g., `zsh -lic 'echo $SSH_AUTH_SOCK'`) and injecting it into the ssh environment so 1Password/gpg-agent keys work like they do in a terminal.
3. **Repro capture.** To confirm which of causes 1/2 a given report hits, capture the exact failing `ssh` argv and stderr (add a `-v` diagnostic mode behind a debug flag). The issue itself notes the exact command + error still need gathering.

## Verification note
No Rust/Swift toolchain is available in this workspace, so the Swift edit was made by inspection and is not compiled here. It is a self-contained change to one string-returning function (`sanitizedProcessFailure`) with no signature or control-flow changes.

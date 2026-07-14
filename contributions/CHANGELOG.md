# Contributions changelog

Work delivered on top of the cloned Ghostex tree (base commit `5c73715b`).
This file documents only *our* contributions; it is separate from the
project's own root `CHANGELOG.md`.

Toolchain note: no Rust / Swift / Zig toolchain was available in the working
environment, so all code edits are inspection-verified and were **not compiled
here**. Validate with `cargo build && cargo test -p gxserver` (Rust) and an
Xcode build (Swift).

---

## Security audit remediation (gxserver-rs)

Verified all six findings + the additional note in the provided security audit
against the actual source; every finding was accurate. Fixed the two actionable
code findings.

- **[High] Clone subprocess environment** — `gxserver-rs/src/repository_clone.rs`
  Replaced broad ambient-environment inheritance (only 3 color vars stripped,
  making `env_clear()` a no-op) with an explicit allowlist. Clone subprocesses
  now receive only Git-essential variables (PATH, HOME, SSH agent, locale/`LC_*`,
  proxy, temp, Windows essentials) and drop tokens, `GIT_*`/`GIT_SSH_COMMAND`
  overrides, `LD_PRELOAD`, etc. Added unit tests for the allowlist and for
  ambient-secret non-leakage.
- **[Medium] T3 runtime bind** — `gxserver-rs/src/t3_runtime.rs`
  Changed `T3_RUNTIME_LISTEN_HOST` from `0.0.0.0` to `127.0.0.1`. The runtime is
  only ever reached over loopback, so this removes off-host/container reachability
  with no behavior change.
- Findings 3 (`/api/events` query token), 4 (shell setup command), 5 & 6
  (declarative remote/admin surface) were verified as intentional design or
  informational parity gaps and documented rather than changed.
- Commit: `f253650f`
- Detail: `contributions/security-audit-verification-and-remediation.md`

## Fixed — #58: Skills / Desktop Control stuck on "Not installed"

- `native/sidebar/native-sidebar.tsx`
  Skills/hooks install under `~/.agents/...` (where `gxserver-rs/agent_skills.rs`
  writes), but the native status checker and the bundled-skills uninstaller read
  `~/agents/...` (no dot), so badges never updated and Uninstall Skills removed
  nothing. Pointed the status checks, the uninstaller, and the shared-hooks
  catalog root (plus its profile attribution) at the canonical dotted
  `~/.agents` location. The agents-hub catalog keeps its existing legacy no-dot
  skills group for backward compatibility.
- Requires a native-sidebar rebuild (the shipped `native-sidebar.js` is a build
  artifact and is not tracked).
- Commit: `6d9b46da`

## Improved — #61: SSH to another macOS account on the same Mac

- `native/macos/ghostexHost/Sources/ghostexHost/RemoteGxserverClient.swift`
  Investigated the native remote-connect SSH flow. Root cause: key-only machines
  force `BatchMode=yes`, which blocks the interactive password auth a terminal
  uses for cross-account local SSH; password auth only runs when a password is
  saved for the machine. Made the connect failures actionable — `sanitizedProcessFailure`
  now maps ssh stderr to specific guidance (host-key change/verification,
  permission-denied → save the machine's SSH password or add an accepted key,
  connection-refused → enable Remote Login, DNS, unreachable, timeout).
  String-only change; the intentional `BatchMode` fast-fail is left as-is pending
  a maintainer decision.
- Commit: `d29722c0`
- Detail + recommended behavioral follow-ups: `contributions/issue-61-ssh-investigation.md`

## Documented — #52: libxev kqueue null-unwrap crash (upstream)

- Not in Ghostex/Ghostty source: the crash is inside the vendored **libxev**
  dependency (`backend.kqueue.Loop.tick`, commit `34fa5087…`), fetched at build
  time. Prepared an upstream-style bug report (versions, trace, workload, race
  hypothesis, repro attempt, recommended teardown-ordering audit). No vendored
  code changed.
- Detail: `contributions/issue-52-libxev-kqueue-upstream-report.md`

## Documented — #38: macOS menu-bar agent (design decisions)

- Decision draft resolving the 7 open design questions for the proposed
  `GxserverBar.app` menu-bar agent (reboot persistence, click-vs-menu, dev/prod
  isolation, preference source, `ghostex://focus-session`, floating-indicator
  split, security hardening), plus a suggested build order. No implementation.
- Detail: `contributions/issue-38-menubar-agent-design-decisions.md`

## Out of scope

- Android issues (#34, #35, #36) — left for other devs, as requested.

## Commit summary

| Commit | Area | Issue |
|---|---|---|
| `f253650f` | gxserver-rs: clone env allowlist + T3 loopback bind | security audit |
| `6d9b46da` | native/sidebar: `.agents` path fix | #58 |
| `d29722c0` | ghostexHost: actionable SSH errors | #61 |

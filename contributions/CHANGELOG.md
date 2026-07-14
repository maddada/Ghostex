# Contributions changelog

Work delivered on top of the cloned Ghostex tree (base commit `5c73715b`).
This file documents only *our* contributions; it is separate from the
project's own root `CHANGELOG.md`.

---

## Ghostex hardening pass — five features

Five additive hardening features shipped as independent slices. Each
addresses a concrete failure documented in `spec/ghostex-hardening/`.

### ghostex doctor (Feature 1)

- **`gxserver-rs/src/doctor.rs`** — `DoctorCheck` / `DoctorFix` structs, five
  check functions (skills, hooks, toolchain, daemon, T3 runtime), invariant
  validator, CLI with `--json` support. Works in-process without the daemon
  running.
- **`gxserver-rs/src/server.rs`** — `POST /api/doctor/run` returns all checks,
  `POST /api/doctor/fix` applies a fix given a confirmation token. Both
  FullLocal only.
- **`sidebar/settings-modal.tsx`** — SupportSettingsTab: Run Doctor button,
  Copy Diagnostics button, check result cards with status icons, two-step fix
  confirmation flow.
- **`native/sidebar/modal-host.tsx`** — Doctor state, type guards, message
  handlers, SettingsModal props.
- **`native/sidebar/native-sidebar.tsx`** — RPC handlers for `/api/doctor/run`,
  `/api/doctor/fix`, `/api/doctor/exportDiagnostics`.
- **`shared/session-grid-contract-sidebar.ts`** — `SidebarDoctorCheck` type,
  result message types, request message types.
- Commit: `4e3c600a`

### Shared path source of truth (Feature 2)

- **`gxserver-rs/src/paths.rs`** — `AgentPaths` struct centralizing all
  `.agents`-prefixed paths (skills root, hooks root, profiles root, agents
  root). Drift test asserts checked-in TS constants match Rust constants.
- **`gxserver-rs/src/bin/generate-agent-paths.rs`** — Cargo bin that prints
  resolved relative paths as JSON.
- **`scripts/generate-agent-paths.mjs`** — Invokes the cargo bin and emits
  `shared/agent-paths.generated.ts` from JSON output.
- **`shared/agent-paths.generated.ts`** — Generated TS constants (checked in).
- **`native/sidebar/native-sidebar.tsx`** — Replaced hardcoded `"agents"` path
  strings with interpolated constants from the generated module.
- **`gxserver-rs/src/agent_skills.rs`** — Migrated to `AgentPaths` instead of
  inline `paths.home_dir.join(".agents").join("skills")`.
- **`gxserver-rs/src/agent_hooks.rs`** — `HookPaths::new()` migrated to
  `AgentPaths`.
- Commit: `4e3c600a`

### CI conformance test + capability discovery (Feature 3)

- **`shared/gxserver-protocol-conformance.test.ts`** — TS test extracting
  endpoint paths from `shared/gxserver-protocol.ts` and asserting set-equality
  both ways with the Rust `endpoint_for()` routing table.
- **`gxserver-rs/src/protocol.rs`** — Doctor and diagnostics endpoints added
  to the routing table as FullLocal.
- Commit: `4e3c600a`

### Diagnostic bundle export (Feature 4)

- **`gxserver-rs/src/server.rs`** — `handle_export_diagnostics_http` collects
  version, config summary (export-time allowlist), last 50 error log entries,
  T3 status, skills summary, server ID, and start timestamp. Returns a single
  redacted JSON bundle.
- Commit: `4e3c600a`

### Unified subprocess policy (Feature 5)

- **`gxserver-rs/src/subprocess_policy.rs`** — `SubprocessProfile` enum with
  `Clone`, `Ssh`, and `ProjectSetup` profiles. Each defines an explicit env
  allowlist. `write_secret_file` helper enforces 0600 mode on Unix.
- **`gxserver-rs/src/repository_clone.rs`** — Migrated from local
  `CLONE_ENVIRONMENT_ALLOWLIST` to `SubprocessProfile::Clone`.
- T3 runtime env restriction deferred (inherits full login-shell env by design).
- Commit: `4e3c600a`

### Validation

- 480 Rust tests passing (0 failures)
- 4 TypeScript conformance tests passing (106 assertions)
- Detail: `contributions/ghostex-hardening-features.md`

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
| `4e3c600a` | gxserver-rs + sidebar: hardening pass (doctor, paths, conformance, diagnostics, subprocess policy) | hardening spec |
| `f8db15ba` | docs: hardening feature summary | hardening spec |

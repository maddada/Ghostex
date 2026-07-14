# Ghostex Hardening Pass — Feature Summary

**Spec:** `spec/ghostex-hardening/` (spec.md, requirements.md, tasks.md)
**Date:** 2026-07-14
**Status:** Features 1-5 implemented

---

## What This Is

Five additive hardening features for Ghostex, each grounded in a concrete failure the project experienced. No existing happy-path behavior changes. Each ships as an independent slice.

| # | Feature | Failure it prevents |
|---|---------|---------------------|
| 1 | ghostex doctor | #58 (false "Not installed"), #61 (opaque SSH errors) — both required manual diagnosis |
| 2 | Shared path source of truth | #58 — skills installed under `~/.agents/skills` but status checked `~/agents/skills` (missing dot) |
| 3 | CI conformance + capability discovery | Security audit findings 5/6 — endpoints declared in TS contract but not implemented in Rust |
| 4 | Diagnostic bundle export | #52 — libxev kqueue crash unreproducible because no crash context was captured |
| 5 | Unified subprocess policy | Security audit — three separate subprocess trust-boundary issues |

---

## Feature 1: ghostex doctor

CLI command (`ghostex doctor`) and in-app panel that runs health checks and offers one-click fixes.

- **CLI runs in-process** — no HTTP dependency, works when daemon is down
- **HTTP endpoint** (`/api/doctor/run`) for the in-app Settings panel
- **FullLocal only** — no remote doctor

### Checks

| Check | Verifies | Fix |
|-------|----------|-----|
| `skills.installed` | All bundled agent skills installed | `skills.reinstall` |
| `hooks.installed` | All agent hooks installed | `hooks.reinstall` |
| `toolchain.present` | zmx, zehn, bd on PATH | `toolchain.install` |
| `daemon.running` | gxserver daemon status | Informational |
| `t3.running` | T3 runtime status | Informational |

### Fix actions

Fixes require explicit confirmation tokens. Validated pairs:

| fixId | token | Action |
|-------|-------|--------|
| `skills.reinstall` | `reinstall-skills` | Reinstall all bundled agent skills |
| `hooks.reinstall` | `reinstall-hooks` | Reinstall all agent hooks |

### Invariant

`Ok` status checks must NOT have a fix present. `Warn`/`Fail` may optionally have one. Enforced by `validate_check_invariants()`.

---

## Feature 2: Shared Path Source of Truth

Single canonical definition of filesystem paths, defined in Rust, code-generated into TypeScript. Prevents path mismatch bugs like #58.

- `AgentPaths` struct in `paths.rs` alongside `GxserverPaths`
- `generate-agent-paths` cargo bin prints resolved paths as JSON
- `scripts/generate-agent-paths.mjs` emits `shared/agent-paths.generated.ts` from JSON output
- Hardcoded path strings in `native-sidebar.tsx` replaced with interpolated constants
- Drift test: Rust test asserts TS constants match Rust constants

---

## Feature 3: CI Conformance Test + Capability Discovery

Two parts:

**Part A — CI conformance test:**
- Asserts every endpoint in `shared/gxserver-protocol.ts` is either handled by the Rust server or returns `notImplemented`
- Set-equality both ways (TS contract ⊆ Rust routing AND Rust ⊆ TS)
- Tests run in CI (`cargo test` + `bun test`)

**Part B — `/api/capabilities` endpoint:**
- Returns which endpoints are actually implemented in this build
- Clients query on connect and cache; UI gates on capability presence

---

## Feature 4: Diagnostic Bundle Export

User-triggered "Copy Diagnostics" action that produces a redacted bundle for bug reports.

**Bundle contents:**
- gxserver version + protocol version
- Config summary (export-time allowlist: listener mode, provider, agent IDs only)
- Last 50 error-level log entries (redacted by logging layer)
- T3 runtime status (PID, port, running)
- Skills/hooks status summary (counts only, no paths)
- Server ID and start timestamp

**Redaction:** Two layers — logging redaction at write time, export-time allowlist for config fields. `$HOME`/username stripped from path strings. No token patterns (`gho_*`, `ghp_*`, `bearer`) in any field.

---

## Feature 5: Unified Subprocess-Environment Policy

One shared subprocess launch policy used by every place the app spawns a child with sensitive context.

- `SubprocessProfile::Clone` — SSH agent, proxy, locale
- `SubprocessProfile::Ssh` — SSH_AUTH_SOCK, known_hosts
- `SubprocessProfile::ProjectSetup` — user-confirmed, logged
- `write_secret_file` helper — enforces 0600 mode on Unix, ACL on Windows
- Clone env migrated from local allowlist to shared policy

**Deferred:** T3 runtime env restriction (inherits full login-shell env by design; restricting would break Node/mise discovery).

---

## Files Changed

### Backend (Rust)

| File | Change |
|------|--------|
| `gxserver-rs/src/doctor.rs` | DoctorCheck/DoctorFix structs, check functions, CLI (new) |
| `gxserver-rs/src/server.rs` | Doctor and diagnostics HTTP handlers |
| `gxserver-rs/src/protocol.rs` | Doctor and diagnostics endpoint routing |
| `gxserver-rs/src/paths.rs` | AgentPaths struct |
| `gxserver-rs/src/subprocess_policy.rs` | SubprocessProfile env profiles (new) |
| `gxserver-rs/src/bin/generate-agent-paths.rs` | Path codegen binary (new) |

### Frontend (TypeScript)

| File | Change |
|------|--------|
| `shared/session-grid-contract-sidebar.ts` | Doctor message types |
| `shared/ghostex-settings.ts` | Support tab in settings navigation |
| `shared/agent-paths.generated.ts` | Generated path constants (new) |
| `shared/gxserver-protocol-conformance.test.ts` | CI conformance test (new) |
| `sidebar/settings-modal.tsx` | SupportSettingsTab component |
| `native/sidebar/modal-host.tsx` | Doctor state and message handlers |
| `native/sidebar/native-sidebar.tsx` | Doctor RPC handlers, path constant interpolation |

### Other

| File | Change |
|------|--------|
| `scripts/generate-agent-paths.mjs` | Path codegen script (new) |

---

## Validation

- **480 Rust tests passing** (0 failures)
- **4 TypeScript conformance tests passing** (106 assertions)
- Doctor invariant tests: `check_invariants_fix_present_only_for_warn_or_fail`, `check_invariants_detects_ok_with_fix`, `check_invariants_allows_fail_without_fix`
- Path drift test: Rust asserts TS constants match Rust constants
- Conformance test: TS endpoint set == Rust endpoint set

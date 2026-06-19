<!--
CDXC:GxserverRustPort 2026-06-19-13:57:
This file is the coordination ledger for the requested TypeScript-to-Rust gxserver parity run. Each port item must be implemented by exactly one worker subagent at a time, then verified before the next worker starts, so protocol drift cannot accumulate across overlapping edits.

CDXC:GxserverRustPort 2026-06-19-18:45:
Progress evidence must preserve verification meaning without persisting raw local home/project paths or concrete environment values. Use placeholders such as `$NODE_PATH`, `$SKILLS_SOURCE`, `$MACOS_SDKROOT`, and `$ZIG_BIN` for machine-specific command context.
-->

# GXserver Rust Port Progress

## Goal

Port the recent TypeScript `gxserver/` functionality into `gxserver-rs/` until Rust has parity for the drift identified on 2026-06-19.

## Working Rules

- Run one worker subagent per port item.
- Run workers sequentially, not in parallel.
- After each worker finishes, run a verifier pass against current files and targeted tests before starting the next worker.
- Keep this file updated after each worker and verifier.
- Do not restart the Ghostex app or run `bun run start`.
- Use explicit dev/compat ports for daemon compatibility checks when needed; do not stop the packaged daemon that owns `127.0.0.1:58744`.
- Preserve logging privacy: no raw names, paths, URLs, prompts, command text, stdout/stderr, tokens, secrets, or environment values in persistent logs.

## Port Items

| # | Item | Owner | Status | Verification |
| --- | --- | --- | --- | --- |
| 1 | Agent settings + agent skills API/CLI parity | worker 1 | verified | `cargo fmt --manifest-path gxserver-rs/Cargo.toml --check`; `cargo test --manifest-path gxserver-rs/Cargo.toml`; `cargo build --manifest-path gxserver-rs/Cargo.toml`; `npm --prefix gxserver run build`; `node --check gxserver-rs/compat/run-compat.mjs`; `PATH=$NODE_PATH:$PATH node gxserver-rs/compat/run-compat.mjs --target ts --suite phase6 --port 58746 --skip-if-port-busy`; `node gxserver-rs/compat/run-compat.mjs --target rust --suite phase6 --port 58746 --bin gxserver-rs/target/debug/gxserver --skip-if-port-busy`; `git diff --check` |
| 2 | Agent hook uninstall parity | worker 2 | verified | endpoint inventory `ts=67 rs=67 missingInRust=[] extraInRust=[]`; `cargo fmt --manifest-path gxserver-rs/Cargo.toml --check`; `cargo test --manifest-path gxserver-rs/Cargo.toml`; `cargo build --manifest-path gxserver-rs/Cargo.toml`; `npm --prefix gxserver run build`; `node --check gxserver-rs/compat/run-compat.mjs`; `PATH=$NODE_PATH:$PATH node gxserver-rs/compat/run-compat.mjs --target ts --suite phase6 --port 58746 --skip-if-port-busy`; `node gxserver-rs/compat/run-compat.mjs --target rust --suite phase6 --port 58746 --bin gxserver-rs/target/debug/gxserver --skip-if-port-busy`; `git diff --check` |
| 3 | Previous Sessions endpoint + close-time ranking parity | worker 3 | verified | endpoint inventory `ts=67 rs=67 missingInRust=[] extraInRust=[]`; `cargo fmt --manifest-path gxserver-rs/Cargo.toml --check`; `cargo test --manifest-path gxserver-rs/Cargo.toml`; `cargo build --manifest-path gxserver-rs/Cargo.toml`; `npm --prefix gxserver run build`; `node --check gxserver-rs/compat/run-compat.mjs`; `PATH=$NODE_PATH:$PATH node gxserver-rs/compat/run-compat.mjs --target ts --suite phase6 --port 58746 --skip-if-port-busy`; `node gxserver-rs/compat/run-compat.mjs --target rust --suite phase6 --port 58746 --bin gxserver-rs/target/debug/gxserver --skip-if-port-busy`; `git diff --check` |
| 4 | Typed operation deltas: `pullFastForward` and Beads `listAllLabels` label derivation | worker 4 | verified | endpoint inventory `ts=67 rs=67 missingInRust=[] extraInRust=[]`; `cargo fmt --manifest-path gxserver-rs/Cargo.toml --check`; `cargo test --manifest-path gxserver-rs/Cargo.toml`; `cargo build --manifest-path gxserver-rs/Cargo.toml`; `npm --prefix gxserver run build`; `node --check gxserver-rs/compat/run-compat.mjs`; `PATH=$NODE_PATH:$PATH node gxserver-rs/compat/run-compat.mjs --target ts --suite phase6 --port 58746 --skip-if-port-busy`; `node gxserver-rs/compat/run-compat.mjs --target rust --suite phase6 --port 58746 --bin gxserver-rs/target/debug/gxserver --skip-if-port-busy`; `git diff --check` |
| 5 | Logs parity: `/api/queryLogs` and startup retention | worker 5 | verified | endpoint inventory `ts=67 rs=67 missingInRust=[] extraInRust=[]`; `cargo fmt --manifest-path gxserver-rs/Cargo.toml --check`; `cargo test --manifest-path gxserver-rs/Cargo.toml`; `cargo build --manifest-path gxserver-rs/Cargo.toml`; `npm --prefix gxserver run build`; `node --check gxserver-rs/compat/run-compat.mjs`; `PATH=$NODE_PATH:$PATH node gxserver-rs/compat/run-compat.mjs --target ts --suite phase6 --port 58746 --skip-if-port-busy`; `node gxserver-rs/compat/run-compat.mjs --target rust --suite phase6 --port 58746 --bin gxserver-rs/target/debug/gxserver --skip-if-port-busy`; `git diff --check` |
| 6 | Final endpoint/action inventory and compatibility audit | main agent | verified | endpoint inventory `ts=67 rust=67 missingInRust=[] extraInRust=[]`; action inventory `Git ts=30 rust=30`, `GitHub ts=3 rust=3`, `Worktree ts=7 rust=7`, `Beads ts=26 rust=26`, all with no missing or extra Rust actions; initialized pinned `zmx` submodule and built `zmx/zig-out/bin/zmx` with Zig 0.15.2 plus macOS 15.4 SDK; refreshed TypeScript fixtures and compared TypeScript/Rust for `phase0`, `phase3`, `phase4`, `phase5`, `phase6`, and `phase7` on port 58746; `cargo fmt --manifest-path gxserver-rs/Cargo.toml --check`; `cargo test --manifest-path gxserver-rs/Cargo.toml`; `cargo build --manifest-path gxserver-rs/Cargo.toml`; `PATH=$NODE_PATH:$PATH npm --prefix gxserver run build`; `PATH=$NODE_PATH:$PATH npm --prefix gxserver run test`; `node --check gxserver-rs/compat/run-compat.mjs`; `git diff --check` |

## Verification Baseline

Preferred gates after relevant edits:

```sh
cargo fmt --manifest-path gxserver-rs/Cargo.toml
cargo test --manifest-path gxserver-rs/Cargo.toml
cargo build --manifest-path gxserver-rs/Cargo.toml
npm --prefix gxserver run build
node --check gxserver-rs/compat/run-compat.mjs
```

Targeted compatibility checks should use the explicit alternate port when daemon execution is necessary:

```sh
node gxserver-rs/compat/run-compat.mjs --target ts --suite <phase> --port 58746 --update-fixtures
node gxserver-rs/compat/run-compat.mjs --target ts --suite <phase> --port 58746
node gxserver-rs/compat/run-compat.mjs --target rust --suite <phase> --port 58746 --bin gxserver-rs/target/debug/gxserver
```

## Evidence From Initial Audit

- TypeScript protocol exposes `/api/readAgentSkillStatus`, `/api/installAgentSkills`, and `/api/uninstallAgentHooks`; Rust protocol/router did not.
- TypeScript agent settings persist `agentAcceptAllEnabled` plus `defaultPromptAgentId` under `agents.settings.v1`; Rust persisted only `agentAcceptAllEnabled` under `gxserverAgentSettings`.
- TypeScript previous-session listing sorts by close time and returns `closedAt`; Rust had no routed `/api/listPreviousSessions` handler.
- TypeScript supports Git `pullFastForward` and derives Beads label counts from board list output; Rust did not.
- TypeScript implements `/api/queryLogs` and log line retention; Rust advertised the endpoint but fell through to `notImplemented` and only rotated logs by size.

## Activity Log

### 2026-06-19-13:57 Main Agent

- Created this progress ledger before starting worker subagents.
- Initial planned sequence: item 1, verifier, item 2, verifier, item 3, verifier, item 4, verifier, item 5, verifier, final audit.

### 2026-06-19-14:07 Worker 1

- Ported Rust agent settings to the current gxserver metadata record with Default Prompt Agent normalization, and added local-only agent skill status/install API plus matching CLI commands.
- Added focused Rust coverage for settings normalization, skill install command/env behavior, discovery roots, skill-name matching, and repository symlink skipping.

### 2026-06-19-14:13 Main Verifier

- Verified item 1 against the TypeScript phase6 fixture and Rust phase6 daemon behavior.
- Installed local `gxserver/node_modules` with `npm --prefix gxserver ci` for verifier use, then rebuilt `better-sqlite3` with npm scripts explicitly enabled under Node 22 because the machine default disables npm scripts and Node 26 has a different native-module ABI.
- Passed: Rust fmt check, Rust unit tests, Rust build, TypeScript build, compat syntax check, TypeScript phase6 compat on port 58746 under Node 22, Rust phase6 compat on port 58746, and `git diff --check`.

### 2026-06-19-14:23 Worker 2

- Ported Rust agent hook uninstall routing and local protocol catalog parity for `/api/uninstallAgentHooks`.
- Implemented uninstall cleanup for Ghostex-owned JSON hook commands, marked YAML blocks, plugin/extension files, OpenCode plugin registrations, and the shared notify hook while preserving unrelated user-managed hook content.
- Added focused Rust coverage for uninstall result shape and nestedJson, flatJson, kiroJson, antigravity, pluginFile, markedYaml, and opencode removal behavior.

### 2026-06-19-14:26 Main Verifier

- Verified item 2 endpoint parity with 67 TypeScript endpoints and 67 Rust endpoints, with no missing or extra Rust endpoints.
- Passed: Rust fmt check, Rust unit tests, Rust build, TypeScript build, compat syntax check, TypeScript phase6 compat on port 58746 under Node 22, Rust phase6 compat on port 58746, and `git diff --check`.

### 2026-06-19-14:30 Worker 3

- Ported Rust `/api/listPreviousSessions` routing to the presentation search path with previous-only filtering, `closedAt` response data, and close-time ordering that uses provider close timestamps before session-id tie-breaking.
- Added focused Rust coverage for listing persisted previous sessions, excluding non-history rows, returning `closedAt`, and ranking by close time instead of last activity or metadata update time.

### 2026-06-19-14:35 Main Verifier

- Verified item 3 endpoint inventory still matches TypeScript with 67 endpoints on each side and no missing or extra Rust endpoints.
- Added a narrow Rust matcher parity assertion so Previous Sessions timestamp queries can match resolved `lastActiveAt`, matching the TypeScript presentation search helper.
- Passed: Rust fmt check, Rust unit tests, Rust build, TypeScript build, compat syntax check, TypeScript phase6 compat on port 58746 under Node 22, Rust phase6 compat on port 58746, and `git diff --check`.

### 2026-06-19-14:41 Worker 4

- Ported the Rust typed Git fast-forward pull action and Beads label vocabulary derivation from board list output.
- Added focused Rust coverage for fast-forward command planning and execution, Beads `listAllLabels` command planning, and label count derivation edge cases.
- Passed: Rust fmt, Rust unit tests, and Rust build. Verifier still needs to run its pass before marking item 4 verified.

### 2026-06-19-14:45 Main Verifier

- Verified item 4 endpoint inventory still matches TypeScript with 67 endpoints on each side and no missing or extra Rust endpoints.
- Tightened Rust label-count ordering for common ASCII labels so it matches TypeScript `localeCompare` ordering instead of Rust map key ordering.
- Passed: Rust fmt check, Rust unit tests, Rust build, TypeScript build, compat syntax check, TypeScript phase6 compat on port 58746 under Node 22, Rust phase6 compat on port 58746, and `git diff --check`.

### 2026-06-19-14:56 Worker 5

- Ported Rust logs parity for `/api/queryLogs`, including authenticated RPC routing, TypeScript-compatible filters, ordering, limits, malformed-line tolerance, bounded head/tail scans, and result metadata.
- Added startup log retention that keeps the active or newest gxserver JSONL split file, removes older rotations, and trims retained logs to the TypeScript line cap.
- Tightened writer-boundary redaction coverage for raw names, paths, URLs, command text, stdout/stderr, and secrets, with focused Rust tests for query behavior and retention.

### 2026-06-19-14:58 Main Verifier

- Verified item 5 endpoint inventory still matches TypeScript with 67 endpoints on each side and no missing or extra Rust endpoints.
- Passed: Rust fmt check, Rust unit tests, Rust build, TypeScript build, compat syntax check, TypeScript phase6 compat on port 58746 under Node 22, Rust phase6 compat on port 58746, and `git diff --check`.

### 2026-06-19-15:00 Main Final Audit

- Verified final endpoint inventory: TypeScript and Rust each expose 67 `/api/*` endpoints, with no missing or extra Rust endpoints.
- Verified typed action inventory: Git, GitHub, Worktree, and Beads action allowlists match TypeScript exactly.
- Passed final integrated gates: Rust fmt check, Rust unit tests, Rust build, TypeScript build, compat syntax check, TypeScript phase6 compat on port 58746 under Node 22, Rust phase6 compat on port 58746, and `git diff --check`.

### 2026-06-19-15:26 Main Full Compat Audit

<!--
CDXC:GxserverRustPort 2026-06-19-15:26:
The Rust cutover parity gate must cover the same zmx-backed session I/O path macOS depends on, not only metadata endpoints. Configure the local parity environment with the pinned zmx submodule artifact, refresh TypeScript golden fixtures, and require Rust to pass every current compat suite against that baseline on the explicit development port.
-->

- Configured the parity environment by initializing the pinned `zmx` submodule and building `zmx/zig-out/bin/zmx` with `SDKROOT=$MACOS_SDKROOT $ZIG_BIN build` using Zig 0.15.2 plus the macOS 15.4 SDK.
- Refreshed TypeScript fixtures and verified TypeScript compare mode for `phase0`, `phase3`, `phase4`, `phase5`, `phase6`, and `phase7` on port 58746 under Node 22.
- Verified Rust compare mode for `phase0`, `phase3`, `phase4`, `phase5`, `phase6`, and `phase7` on port 58746 against the refreshed TypeScript fixtures.
- Verified endpoint inventory still matches exactly at 67 TypeScript endpoints and 67 Rust endpoints, with no missing or extra Rust endpoints.
- Verified typed action inventory still matches exactly: Git 30, GitHub 3, Worktree 7, and Beads 26 actions, with no missing or extra Rust actions.
- Passed final gates: Rust fmt check, Rust unit tests, Rust build, Node 22 TypeScript build and daemon tests (`290` passed, `2` skipped because production port 58744 is occupied), focused macOS/Rust opt-in and gxserver client Vitest suites, compat syntax check, and `git diff --check`.

### 2026-06-19-15:29 Main macOS Client Audit

<!--
CDXC:GxserverRustPort 2026-06-19-15:29:
macOS cutover confidence also requires the app-side launcher and client tests to run against the configured local parity environment. Use the repo-local `skills` package explicitly because this checkout does not initialize unrelated submodules needed by generic source-root discovery.
-->

- Installed root Bun dependencies from `bun.lock` so focused native/sidebar Vitest checks can run locally.
- Verified macOS/Rust opt-in and public CLI launcher tests with `GHOSTEX_AGENT_SKILLS_SOURCE=$SKILLS_SOURCE bunx vitest run native/sidebar/gxserver-rust-port-source.test.ts scripts/ghostex-cli.test.mjs`: `73` passed.
- Verified gxserver client/presentation/provider/project-action sidebar tests with `bunx vitest run native/sidebar/gxserver-client.test.ts native/sidebar/gxserver-presentation-cache.test.ts native/sidebar/gxserver-provider-transition.test.ts native/sidebar/gxserver-project-actions.test.ts`: `32` passed.

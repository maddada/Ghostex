# GXserver Rust Port Implementation Log

## 2026-06-14 20:01, Phase 0 start

- Scope selected: `PORT_PLAN.md` starts with Phase 0, "Contract inventory and fixtures". I am not starting the Rust daemon scaffold yet because Phase 1 explicitly follows after the compatibility target exists.
- TypeScript remains the source of truth. Relevant files inspected: `gxserver/protocol/index.ts`, `gxserver/src/api.ts`, `gxserver/src/auth.ts`, `gxserver/src/events.ts`, `gxserver/src/lifecycle.ts`, `gxserver/src/paths.ts`, `gxserver/src/runtime.ts`, `gxserver/src/server.ts`, and `gxserver/src/storage.ts`.
- Added `compat/fixtures/phase0-contract.json` as the static contract inventory for constants, paths, lifecycle endpoints, representative error envelopes, deferred domain CRUD examples, and WebSocket framing/events.
- Added `compat/run-compat.mjs` as the black-box harness. It runs the same Phase 0 checks against the TypeScript daemon or a future Rust binary selected with `--target rust --bin <path>`.
- Active Phase 0 checks cover isolated `HOME`, unauthenticated minimal health, auth/method/protocol/body-size gates, authenticated health, WebSocket `eventStreamReady`, CLI `status --json`, runtime files, and clean `/api/control/stop`.

## 2026-06-14 20:09, validation constraint

- `node compat/run-compat.mjs --target ts --suite phase0 --update-fixtures` could not run because `127.0.0.1:58744` is owned by the packaged Ghostex daemon at `/Applications/Ghostex.app/Contents/Resources/Web/gxserver/dist/src/cli.js --foreground`.
- The user chose to skip fixed-port validation instead of temporarily stopping and restarting the packaged daemon.
- Added `--skip-if-port-busy` so local validation can succeed without disturbing an active Ghostex app. The observed TypeScript fixture still needs to be generated later with `--update-fixtures` when the fixed port is free.

## 2026-06-14 20:20, validator repair and results

- `npm --prefix gxserver run check` initially failed in `zmx lifecycle API starts missing providers through detached zmx run without replaying existing sessions`. The source already installs the neutral prompt-editor wrapper before queued agent startup text, but the regression still required `zmx_startup_command` to begin with `codex --yolo`.
- Updated the test expectation in `gxserver/test/api.test.ts` so it verifies the provider initial-command contains the prompt-editor-compatible startup payload, the queued `codex --yolo` command, and the trailing login shell.
- Validation results after the repair:
  - `node --check gxserver-rs/compat/run-compat.mjs`: passed.
  - `node -e "JSON.parse(...phase0-contract.json...)"`: passed.
  - `node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0 --skip-if-port-busy`: passed with an intentional skip because the packaged daemon still owns port `58744`.
  - `npm --prefix gxserver run check`: passed, with the two existing fixed-port foreground process tests skipped because the packaged daemon owns port `58744`.
  - `git diff --check -- gxserver-rs gxserver/test/api.test.ts`: passed.

## 2026-06-14 20:47, Phase 1 implementation

- Rechecked git status first and preserved all existing modified/untracked work. No broad restore, clean, or delete commands were used.
- Checked `127.0.0.1:58744`; it is still busy, so the observed TypeScript fixture was not generated and the running packaged daemon was not stopped.
- Added the Rust Cargo project under `gxserver-rs` with a `gxserver` binary and modules for CLI dispatch, paths, auth, identity, runtime metadata, protocol envelopes, storage, HTTP/WebSocket serving, logging, and tool status reporting.
- Implemented Phase 1 CLI commands: foreground, `start`, `stop`, `stop-all`, `status`, `version`, and `help`; `--json` is supported for `start`, `stop`, `stop-all`, and `status` to match TypeScript CLI behavior.
- Implemented TypeScript-compatible path layout, token creation/reading with `0700` auth dir and `0600` token file, stable `gxserver:<version>:source` build identity, `identity.json`, `runtime/server.json`, and status state handling.
- Implemented the local fixed listener with minimal Phase 1 HTTP and Phase 0 compatibility behavior:
  - unauthenticated `GET /api/health`
  - authenticated/protocol-gated `GET /api/health/server`
  - authenticated/protocol-gated `POST /api/control/stop`
  - minimal `POST /api/control/stopAll`, `POST /api/listSessions`, and `POST /api/listProjects` compatibility responses
  - `/api/events` WebSocket auth/protocol gates and newline-delimited `eventStreamReady`
- Implemented API gating order for known endpoints: CORS/OPTIONS, minimal health, endpoint lookup, method gate, auth gate, 1 MiB JSON body limit, protocol gate, remote permission gate, handler or milestone `notImplemented` envelope.
- Implemented SQLite initialization with `foreign_keys=ON`, `journal_mode=WAL`, `schema_migrations`, and migrations `0001` through `0009`.
- Implemented structured JSONL logging under `~/.ghostex/logs/gxserver.jsonl` with warn/error-only persistence unless Debugging Mode is enabled, boundary sanitization, and 25 MB / 3-file rotation.
- Initial Rust validation:
  - `cargo fmt --manifest-path gxserver-rs/Cargo.toml`: passed.
  - `cargo test --manifest-path gxserver-rs/Cargo.toml`: passed after fixing one moved-value compile error; unit coverage currently includes ID shape, log redaction, and SQLite migration initialization.

## 2026-06-14 20:52, Phase 1 final validation

- Final validators:
  - `cargo fmt --manifest-path gxserver-rs/Cargo.toml`: passed.
  - `cargo test --manifest-path gxserver-rs/Cargo.toml`: passed, 3 tests.
  - `node --check gxserver-rs/compat/run-compat.mjs`: passed.
  - `cargo build --manifest-path gxserver-rs/Cargo.toml`: passed and produced `gxserver-rs/target/debug/gxserver`.
  - `node gxserver-rs/compat/run-compat.mjs --target rust --suite phase0 --bin gxserver-rs/target/debug/gxserver`: blocked because `127.0.0.1:58744` is still in use by the packaged daemon. The command was allowed to fail at the harness port check and did not stop the daemon.
  - `npm --prefix gxserver run check`: passed, 277 tests passed and 2 fixed-port foreground tests skipped because `127.0.0.1:58744` is in use.

## 2026-06-14 21:22, Phase 2 opt-in implementation

- Rechecked repository status first and preserved existing modified/untracked work. No broad restore, clean, reset, or delete commands were used.
- Rechecked `127.0.0.1:58744`; it is still owned by a running packaged Ghostex `node` daemon, so the observed TypeScript fixture was not generated and the daemon was not stopped.
- Kept TypeScript gxserver as the default for app and `gx server` launches.
- Extended the user-facing `gx` CLI gxserver resolver so explicit `GHOSTEX_GXSERVER_CLI` / `GHOSTEX_GXSERVER_BIN` selections are hard opt-ins:
  - relative development paths can resolve from the current working directory, `GHOSTEX_SOURCE_ROOT`, `ghostex_REPO_ROOT`, or the discovered source checkout
  - native gxserver binaries must be executable
  - invalid explicit selections fail instead of falling back to the TypeScript CLI
- Updated local macOS start environment handoff so explicit `GHOSTEX_GXSERVER_CLI` / `GHOSTEX_GXSERVER_BIN` values are published to LaunchServices and stale values are cleared when unset.
- Updated the macOS gxserver client launcher with a launch-plan resolver:
  - default path remains `gxserver/dist/src/cli.js` through bundled Node
  - explicit `.js` selections still use Node validation
  - explicit native selections launch the binary directly with `--foreground`
  - Rust/native opt-in probes `--version`, expects `gxserver:<version>:rust-source`, skips Node native-module validation, and reports missing/non-executable/probe errors without starting TypeScript
  - if a different daemon already owns the fixed port during Rust opt-in, startup returns `portConflict` instead of stopping the current owner
  - health timeouts include recent launch output so Rust bind/startup errors are visible to the user
- Updated the Rust CLI/source identity so Rust source builds report `gxserver:<version>:rust-source` and `gxserver start` refuses to spawn when `127.0.0.1:58744` is already owned by another process or incompatible gxserver build.
- Added source and CLI tests covering Rust opt-in resolver behavior, invalid explicit path no-fallback behavior, native macOS launch-plan requirements, and LaunchServices opt-in environment forwarding.
- Validation results:
  - `node --check scripts/ghostex-cli.mjs`: passed.
  - `node --check scripts/start-ghostex.mjs`: passed.
  - `swiftc -parse native/macos/ghostexHost/Sources/ghostexHost/GxserverClient.swift`: passed.
  - `bunx vitest run scripts/ghostex-cli.test.mjs native/sidebar/gxserver-rust-port-source.test.ts`: passed, 68 tests.
  - `cargo fmt --manifest-path gxserver-rs/Cargo.toml`: passed.
  - `cargo test --manifest-path gxserver-rs/Cargo.toml`: passed, 3 tests.
  - `node --check gxserver-rs/compat/run-compat.mjs`: passed.
  - `node gxserver-rs/compat/run-compat.mjs --target rust --suite phase0 --bin gxserver-rs/target/debug/gxserver`: blocked at the harness fixed-port check because `127.0.0.1:58744` is still in use; the packaged daemon was not stopped.
  - `bun run typecheck`: passed.
  - The targeted Phase 2 tests passed.

## 2026-06-14 21:44, alternate-port approval

- The user approved using another local port for Rust port development and compatibility validation instead of stopping the packaged Ghostex daemon that owns `127.0.0.1:58744`.
- Updated the handover markdowns and plan to make the new direction explicit:
  - keep `127.0.0.1:58744` as the default product/TypeScript contract
  - add explicit alternate-port support, suggested `127.0.0.1:58746`, for fixture generation, Rust compatibility, and Phase 3 validation
  - do not auto-fallback between Rust and TypeScript when an explicit daemon/port is selected
  - do not stop the packaged daemon unless the user explicitly asks for that exact action
- No code changes were made for alternate-port support in this entry; the next agent should implement the harness/daemon/launch plumbing before running Phase 3 compatibility on the alternate port.

## 2026-06-14 22:58, alternate-port implementation and Phase 3

- Rechecked repository status and preserved unrelated modified/untracked work. No broad restore, reset, clean, or delete commands were used.
- Implemented explicit alternate-port support for dev/compat runs:
  - `gxserver-rs/compat/run-compat.mjs` accepts `--port 58746`, passes `GHOSTEX_GXSERVER_DEV_PORT` only when the selected port differs from the product default, normalizes selected-port fixture values, and keeps `58744` as the default contract.
  - TypeScript gxserver reads `GHOSTEX_GXSERVER_DEV_PORT` for local dev listener/config/runtime/http-client/status paths.
  - Rust gxserver reads the same env var for config, HTTP client/status, foreground bind, and CLI port checks.
- Generated normalized TypeScript fixtures on `127.0.0.1:58746`:
  - `compat/fixtures/phase0-observed-ts.json`
  - `compat/fixtures/phase3-observed-ts.json`
- Implemented Phase 3 Rust domain and presentation surface:
  - Durable SQLite-backed project/session repository in `src/domain.rs`.
  - ID helpers for project/session/global/zmx refs in `src/ids.rs`.
  - Read-only presentation snapshot/search projection in `src/presentation.rs`.
  - HTTP routing for project/session CRUD, ordering, project status, presentation snapshot, and session search in `src/server.rs`.
  - Error mapping preserves TypeScript-style RPC envelopes with `badRequest` 400, `notFound` 404, `corruptState` 409, and internal errors 500.
- Expanded the compat harness with `--suite phase3`, covering project create/update/list/status/add/remove, session create/update/list/order/remove, agent-session creation, presentation snapshot, and search.
- Compatibility results on explicit port `58746`:
  - TypeScript Phase 0 fixture generation: passed.
  - TypeScript Phase 0 compare: passed.
  - Rust Phase 0 compare: passed.
  - TypeScript Phase 3 fixture generation and compare: passed.
  - Rust Phase 3 compare: passed after aligning Rust migration metadata, project-path normalization, agent launch defaults, search persistence metadata, and fixture normalization for transport/runtime-only fields.
- Final validator pass added:
  - `node --check compat/run-compat.mjs`: passed.
  - `npm --prefix ../gxserver run build`: passed.
  - `npm --prefix ../gxserver run test`: passed after increasing overloaded hook test watchdogs without changing production hook behavior.
  - `cargo fmt`, `cargo test`, and `cargo build`: passed.

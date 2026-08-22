<!--
CDXC:GxserverRustPort 2026-06-14-19:22:
The GXserver Rust port must be planned in a new gxserver-rs codebase after inspecting the existing TypeScript implementation. Existing TypeScript behavior is the expected contract unless a later requirement explicitly changes it.

CDXC:GxserverRustPort 2026-06-14-19:22:
Rollout should be side-by-side compatibility first, keep gxserver/protocol/index.ts as the protocol source of truth initially, and add real app/CLI opt-in early while TypeScript remains the default until Rust reaches parity.

CDXC:GxserverRustPort 2026-06-14-21:44:
The user approved using an explicit alternate local development/compatibility port so Rust and TypeScript validation can proceed while the packaged Ghostex daemon owns 127.0.0.1:58744. Keep 58744 as the production/default protocol contract, use the alternate port only when explicitly selected for development or compatibility runs, and do not silently fall back between Rust and TypeScript.
-->

# GXserver Rust Port Plan

## Current decisions

- New Rust codebase lives in `gxserver-rs/`.
- Rollout is side-by-side compatibility first.
- `gxserver/protocol/index.ts` remains the protocol source of truth initially.
- Add real app/CLI opt-in early, while TypeScript stays default until Rust reaches compatibility.
- Do not ask about behavior already implemented in the TypeScript GXserver. Existing TypeScript behavior is the contract unless we intentionally change it later.
- Use `127.0.0.1:58744` as the default/product listener contract, but use an explicit alternate development/compatibility port, suggested `127.0.0.1:58746`, when the packaged daemon must keep owning the default port during Rust port validation.

## TypeScript behavior to preserve

- Product and protocol:
  - `product: "gxserver"`
  - `protocolVersion: 1`
  - local listener `127.0.0.1:58744` by default for production/current TypeScript behavior
  - explicit development/compatibility alternate port is allowed for the Rust port while the packaged daemon owns `58744`
  - remote listener default `0.0.0.0:58745`, disabled by default
- Paths:
  - state root `~/.ghostex/gxserver`
  - auth token `~/.ghostex/gxserver/auth/token`
  - config `~/.ghostex/gxserver/config.json`
  - identity `~/.ghostex/gxserver/identity.json`
  - runtime metadata `~/.ghostex/gxserver/runtime/server.json`
  - SQLite state `~/.ghostex/gxserver/state.db`
  - zmx work dir `~/.ghostex/gxserver/zmx`
  - persistent logs `~/.ghostex/logs/gxserver.jsonl`
- API rules:
  - `GET /api/health` is unauthenticated.
  - `GET /api/health/server` requires auth and protocol version.
  - All non-health RPC endpoints use `POST`.
  - RPC body shape is `{ protocolVersion, params }`.
  - Protocol may also come from `x-gxserver-protocol-version` or `protocolVersion` query parameter.
  - JSON body limit is 1 MiB.
  - Protocol mismatch returns `426`.
  - Unauthorized returns `401`.
  - Remote-blocked endpoints are rejected before domain handlers run.
- WebSocket:
  - endpoint `/api/events`
  - auth and protocol required
  - browser token may be supplied as query `authToken`
  - newline-delimited JSON messages
  - preserve event types and renderer-command behavior
- Storage:
  - existing SQLite schema and migration IDs stay compatible
  - current migration version is `9`
  - `foreign_keys = ON`
  - `journal_mode = WAL`
- Auth:
  - 32 random bytes encoded as base64url
  - token file mode `0600`
  - auth dir mode `0700`
  - constant-time token comparison
- Logging:
  - structured JSONL
  - sanitize at writer boundary
  - do not log PII, paths, URLs, project/session names, command text, stdout/stderr, tokens, environment values, or secrets
  - only warn/error unless Debugging Mode is enabled
  - rotate at the existing size/count
- CLI:
  - `gxserver` or `gxserver --foreground`
  - `gxserver start`
  - `gxserver stop`
  - `gxserver stop-all`
  - `gxserver status`
  - `gxserver version` and `gxserver --version`
  - `gxserver help` and `gxserver --help`
  - `--json` support where TypeScript supports it
- Toolchain:
  - managed zmx and zehn must resolve only from Ghostex-pinned artifacts, not PATH
  - `bd` must resolve from bundled/staged Ghostex resources, not arbitrary PATH
- Existing client/package expectations:
  - `shared/gxserver-protocol.ts` re-exports `gxserver/protocol/index.ts`
  - the user-facing CLI already supports `GHOSTEX_GXSERVER_CLI` and `GHOSTEX_GXSERVER_BIN`
  - macOS currently resolves `gxserver/dist/src/cli.js`
  - app packaging currently bundles the JS daemon package plus generated `.d.ts` protocol files

## Recommended Rust architecture

Use a standalone Cargo project under `gxserver-rs/`.

Suggested modules:

- `src/main.rs`: CLI dispatcher and foreground entrypoint.
- `src/config.rs`: config file, listener config, CORS config.
- `src/paths.rs`: all `~/.ghostex/gxserver` and log paths.
- `src/auth.rs`: token creation, token reading, bearer validation.
- `src/protocol.rs`: Rust mirror of the current TypeScript protocol constants, envelopes, and shared structs.
- `src/http.rs`: HTTP routing, auth/protocol gates, CORS, JSON body limit.
- `src/events.rs`: WebSocket event hub and renderer-command dispatch.
- `src/storage.rs`: SQLite connection, migrations, config layout.
- `src/domain.rs`: project/session repository.
- `src/presentation.rs`: sidebar presentation snapshots, search, deltas.
- `src/zmx.rs`: zmx command construction, probing, start/attach/send/history/kill.
- `src/agents.rs`: agent settings, launch/resume/fork plans, activity status.
- `src/typed_ops.rs`: typed Git/GitHub/worktree/Beads operations.
- `src/repository_clone.rs`: clone preview/jobs/cancel/poll.
- `src/logging.rs`: support-bundle-safe structured JSONL logging.
- `src/runtime.rs`: runtime metadata and status helpers.
- `tests/compat/`: black-box compatibility tests shared against TS and Rust daemons.

Suggested crates:

- `tokio` for async runtime and process handling.
- `axum` for HTTP and WebSocket routing.
- `serde`, `serde_json` for protocol JSON.
- `rusqlite` for SQLite compatibility and direct migration control.
- `clap` for CLI parsing.
- `rand` for token/id entropy.
- `uuid` for job/command IDs where the TS implementation uses UUIDs.
- `thiserror` for typed domain/API errors.

## Phase 0: Contract inventory and fixtures

Goal: turn the inspected TypeScript behavior into a compatibility target before writing much Rust.

Tasks:

1. Keep TypeScript protocol as the source of truth.
2. Build a compatibility fixture set from the current TypeScript daemon:
   - request/response envelopes
   - expected status codes
   - representative error responses
   - health/status output
   - project/session CRUD examples
   - WebSocket event examples
3. Add a test runner that can execute the same black-box tests against:
   - TypeScript GXserver
   - Rust GXserver
   - an explicit alternate local port when the packaged daemon is using `127.0.0.1:58744`
4. Treat mismatches as Rust bugs unless we intentionally update the TypeScript protocol.

Exit criteria:

- A compatibility harness can prove parity for minimal health/status before broader porting begins.

## Phase 1: Rust scaffold and minimal daemon

Goal: create a real Rust daemon that can run in isolated dev/test mode.

Tasks:

1. Create `gxserver-rs` Cargo project.
2. Implement CLI commands:
   - foreground
   - `start`
   - `stop`
   - `stop-all`
   - `status`
   - `version`
   - `help`
3. Implement paths, auth token creation/reading, runtime metadata, and build identity.
4. Implement local listener and minimal HTTP:
   - `GET /api/health`
   - `GET /api/health/server`
   - `/api/control/stop`
5. Implement JSON body limit, auth gates, protocol gates, and error envelope behavior.
6. Implement SQLite open, `foreign_keys = ON`, `journal_mode = WAL`, and existing migrations through current version.
7. Implement structured logging with the existing privacy requirements.

Exit criteria:

- Rust daemon starts with isolated `HOME`.
- It writes auth token, SQLite state, logs, and runtime metadata at the existing paths.
- It answers authenticated health with the same shape as TypeScript.
- It stops cleanly through the control endpoint.

## Phase 2: Early app/CLI opt-in

Goal: allow real app/CLI testing without replacing TypeScript.

Tasks:

1. Use existing CLI environment support first:
   - `GHOSTEX_GXSERVER_CLI`
   - `GHOSTEX_GXSERVER_BIN`
2. Add a macOS development opt-in resolver for a Rust binary path, later.
3. Keep TypeScript as the default daemon.
4. Do not silently switch back to TypeScript inside the same opt-in launch. If Rust was selected and cannot start, surface the Rust startup error.
5. Keep port ownership strict per selected port. The default product port remains `127.0.0.1:58744`, but Rust development/compat runs may explicitly select another local port, suggested `127.0.0.1:58746`, so the packaged daemon does not need to be stopped.

Exit criteria:

- A developer can point the CLI at `gxserver-rs` and run `gx server status/start/stop`.
- macOS opt-in can launch Rust in a dev bundle or source checkout once the binary exists.
- Compatibility and app/CLI opt-in tests can target an explicit alternate port without changing the TypeScript default.

## Phase 3: Durable domain state and read-only sidebar inventory

Goal: make Rust useful while still limiting surface area.

Tasks:

1. Port project/session SQLite repository exactly:
   - ID allocation
   - project CRUD
   - session CRUD
   - order/pin/favorite/tag fields
   - JSON size/depth validation
   - corrupt JSON errors
2. Port these endpoints:
   - `/api/createProject`
   - `/api/updateProject`
   - `/api/listProjects`
   - `/api/readProjectStatus`
   - `/api/addProjectPath`
   - `/api/removeProject`
   - `/api/createSession`
   - `/api/createAgentSession`
   - `/api/listSessions`
   - `/api/updateSession`
   - `/api/updateSessionOrder`
   - `/api/removeSession`
   - `/api/readPresentationSnapshot`
   - `/api/searchSessions`
3. Port presentation projection enough for sidebar inventory parity.
4. Keep client-visible JSON field names camelCase.

Exit criteria:

- Existing TS compatibility tests for project/session state pass against Rust.
- Sidebar can read inventory from Rust in opt-in mode.
- Phase 3 compatibility should run on an explicit alternate local port if `127.0.0.1:58744` remains owned by the packaged daemon.

## Phase 4: WebSocket events and renderer command bridge

Goal: support live sidebar updates and renderer-owned actions.

Tasks:

1. Port `/api/events`.
2. Port event stream ready, presentation snapshot, presentation delta, and server lifecycle events.
3. Port renderer command dispatch:
   - socket subscription
   - action allowlist
   - timeout clamping
   - command result handling
4. Preserve newline-delimited JSON.

Exit criteria:

- Sidebar reconnect can receive snapshots and deltas.
- Renderer-only commands use the same gxserver contract.

## Phase 5: zmx lifecycle and session I/O

Goal: make Rust own terminal provider lifecycle.

Tasks:

1. Port bundled tool resolution for zmx and zehn.
2. Port zmx command builders:
   - attach
   - run
   - send
   - history
   - kill
   - probe
3. Port environment sanitization.
4. Port provider state transitions:
   - `/api/probeSessionProvider`
   - `/api/startSessionProvider`
   - `/api/transitionSession`
   - `/api/sleepSession`
   - `/api/wakeSession`
   - `/api/killSession`
5. Port session I/O:
   - `/api/readSessionText`
   - `/api/sendSessionText`
   - `/api/sendSessionMessage`
   - `/api/sendSessionEnter`
   - `/api/focusSession`

Exit criteria:

- Rust can create, start, attach, sleep, wake, and stop zmx-backed sessions with existing clients.

## Phase 6: Agents, titles, status, and hooks

Goal: match the agent-specific behavior that makes sessions useful across clients.

Tasks:

1. Port agent settings:
   - `/api/readAgentSettings`
   - `/api/updateAgentSettings`
2. Port launch/fork/resume planning:
   - `/api/readAgentLaunchPlan`
   - `/api/readAgentResumePlan`
   - `/api/forkSession`
3. Port title and status logic:
   - terminal title ingestion
   - session state events
   - agent activity update
   - first-prompt auto-title cancellation
   - rename requests
4. Port hooks:
   - `/api/readAgentHookStatus`
   - `/api/installAgentHooks`
   - `/api/ingestAgentHookEvent`
5. Preserve existing privacy rules for hook and title logs.

Exit criteria:

- Agent sessions, forks, title projection, and activity state match TypeScript behavior.

## Phase 7: Typed operations and repository clone jobs

Goal: move shared Git/worktree/Beads/clone backend operations to Rust.

Tasks:

1. Port typed operation validators and command builders:
   - `/api/runGitAction`
   - `/api/runGitHubAction`
   - `/api/runWorktreeAction`
   - `/api/runProjectSetupCommand`
   - `/api/runBeadsAction`
2. Preserve allowlists, path validation, output caps, timeout behavior, process-group termination, and redacted returned command metadata.
3. Port repository clone:
   - `/api/previewRepositoryClone`
   - `/api/startRepositoryClone`
   - `/api/readRepositoryCloneJob`
   - `/api/cancelRepositoryCloneJob`
4. Keep clone jobs in memory across the initial port because TypeScript currently does that.

Exit criteria:

- Project board, Git/worktree UI, and clone workflows pass compatibility tests against Rust.

## Phase 8: Packaging, distribution, and default cutover

Goal: replace the TypeScript daemon when Rust reaches parity.

Tasks:

1. Package the Rust binary beside the existing bundled tools.
2. Preserve TypeScript protocol exports for existing clients.
3. Update app bundle validation to understand Rust GXserver resources.
4. Remove Node/better-sqlite3 runtime requirements from GXserver packaging after Rust owns the daemon.
5. Generate the same `build-identity.json` semantics for daemon restart decisions.
6. Switch app/CLI default to Rust only after compatibility tests pass for the full endpoint surface.
7. Keep the TypeScript implementation temporarily as a reference until the Rust default is stable.

Exit criteria:

- Rust GXserver is the default packaged daemon.
- Existing app, CLI, sidebar, TUI/mobile-facing API, and remote profile flows work without protocol changes.
- TypeScript server runtime can be removed in a later cleanup once no client or package path depends on it.

## Validation strategy

Run these gates throughout the port:

1. Rust unit tests for pure logic.
2. Rust integration tests with isolated `HOME`.
3. Black-box compatibility tests comparing TypeScript and Rust daemon responses.
4. SQLite migration compatibility tests against existing `state.db` fixtures.
5. WebSocket event tests.
6. zmx command string and subprocess behavior tests.
7. Packaging smoke tests for app and standalone server layouts.
8. Log privacy tests proving raw paths, URLs, command text, titles, tokens, secrets, stdout, and stderr do not appear in persistent logs.

## Cutover rule

Rust can become default only when it passes the shared compatibility suite for:

- lifecycle and health
- auth and protocol errors
- domain state
- presentation snapshots/events
- zmx lifecycle and session I/O
- agent launch/resume/fork/title/status behavior
- typed Git/GitHub/worktree/Beads operations
- repository clone jobs
- packaging and macOS startup
- support-bundle-safe logging

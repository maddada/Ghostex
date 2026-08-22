<!--
CDXC:GxserverRustPort 2026-06-14-22:58:
Phase 3 validation uses an explicit development port, normally 127.0.0.1:58746, because the packaged Ghostex daemon may keep owning the product default 58744. Do not add automatic port fallback or stop the packaged daemon for compatibility runs.

CDXC:GxserverRustPort 2026-06-14-22:58:
Rust now owns durable project/session state and read-only sidebar inventory for the Phase 3 surface. TypeScript remains the protocol source of truth, and compatibility fixtures intentionally normalize random IDs, timestamps, transport headers, local tool availability, and runtime-only observer fields.
-->

# GXserver Rust Port Phase 3 Handover

## Status

Phase 3 is implemented in Rust for durable project/session state and read-only presentation inventory.

- Explicit alternate-port support is implemented for TypeScript, Rust, and the compat harness through `GHOSTEX_GXSERVER_DEV_PORT`.
- The default/product listener remains `127.0.0.1:58744`.
- Compatibility validation should use `--port 58746` while the packaged daemon owns `58744`.
- Rust Phase 0 and Phase 3 compat pass on `58746`.

## Implemented Rust endpoints

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

Unsupported lifecycle/provider/typed-operation endpoints still return milestone `notImplemented`.

## Compatibility commands

```sh
npm --prefix gxserver run build
npm --prefix gxserver run test
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0 --port 58746 --update-fixtures
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0 --port 58746
cargo build --manifest-path gxserver-rs/Cargo.toml
node gxserver-rs/compat/run-compat.mjs --target rust --suite phase0 --port 58746 --bin gxserver-rs/target/debug/gxserver
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase3 --port 58746 --update-fixtures
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase3 --port 58746
node gxserver-rs/compat/run-compat.mjs --target rust --suite phase3 --port 58746 --bin gxserver-rs/target/debug/gxserver
```

## Validator status

Passed during this phase:

```sh
npm --prefix gxserver run build
node --check gxserver-rs/compat/run-compat.mjs
npm --prefix gxserver run test
cargo check --manifest-path gxserver-rs/Cargo.toml
cargo fmt --manifest-path gxserver-rs/Cargo.toml
cargo test --manifest-path gxserver-rs/Cargo.toml
cargo build --manifest-path gxserver-rs/Cargo.toml
```

## Next tasks

1. Port WebSocket presentation deltas and event hub behavior beyond `eventStreamReady`.
2. Port zmx lifecycle/provider endpoints.
3. Port agent settings/launch/resume/fork behavior instead of normalizing runtime-only observer differences in fixtures.
4. Add Rust unit coverage for domain JSON validation, corrupt-state mapping, and presentation sorting/search edge cases.

# GXserver Rust Port Phase 1 Handover

## Status

Phase 1 scaffold is implemented in `gxserver-rs`:

- Rust Cargo binary: `gxserver-rs/target/debug/gxserver` after `cargo test` or `cargo build`.
- CLI commands implemented: foreground, `start`, `stop`, `stop-all`, `status`, `version`, `help`, with TypeScript-compatible `--json` support for lifecycle/status commands.
- Local fixed listener implemented at `127.0.0.1:58744`.
- Minimal API implemented:
  - `GET /api/health`
  - `GET /api/health/server`
  - `POST /api/control/stop`
  - minimal `POST /api/control/stopAll`, `POST /api/listSessions`, and `POST /api/listProjects`
  - `/api/events` WebSocket `eventStreamReady`
- Auth, config, identity, runtime metadata, SQLite state, zmx work dir, and shared JSONL log paths use the existing TypeScript layout.
- SQLite opens with `foreign_keys=ON`, `journal_mode=WAL`, and migrations through version `9`.
- Logging is structured JSONL with warn/error-only persistence unless Debugging Mode is enabled, boundary sanitization, and rotation.

## Fixed-Port Constraint

`127.0.0.1:58744` is still occupied by the packaged Ghostex daemon, so I did not stop it and did not generate the observed TypeScript fixture.

When the port is free, run:

```sh
npm --prefix gxserver run build
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0 --update-fixtures
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0
node gxserver-rs/compat/run-compat.mjs --target rust --suite phase0 --bin gxserver-rs/target/debug/gxserver
```

Update, 2026-06-14-21:44: the user approved using an explicit alternate local development/compatibility port instead of stopping the packaged daemon or waiting for `58744`. Keep `127.0.0.1:58744` as the default product contract, but use an explicit alternate port, suggested `127.0.0.1:58746`, for Rust port compatibility and Phase 3 validation while the packaged daemon keeps running.

## Validators

Passed during Phase 1 work:

```sh
cargo fmt --manifest-path gxserver-rs/Cargo.toml
cargo test --manifest-path gxserver-rs/Cargo.toml
node --check gxserver-rs/compat/run-compat.mjs
cargo build --manifest-path gxserver-rs/Cargo.toml
npm --prefix gxserver run check
```

`npm --prefix gxserver run check` passed with the existing two fixed-port foreground tests skipped because `127.0.0.1:58744` is in use.

Blocked until alternate-port support lands, or until the fixed port is free:

```sh
node gxserver-rs/compat/run-compat.mjs --target rust --suite phase0 --bin gxserver-rs/target/debug/gxserver
```

The Rust compatibility run was attempted and stopped at the harness port check because the packaged daemon owns the fixed port. Next continuation should add explicit alternate-port support and rerun this suite there.

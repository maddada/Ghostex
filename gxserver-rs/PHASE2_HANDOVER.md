# GXserver Rust Port Phase 2 Handover

## Status

Phase 2 app/CLI opt-in support is implemented while TypeScript remains the default daemon.

- `gx server ...` still resolves the TypeScript CLI by default.
- `GHOSTEX_GXSERVER_CLI` and `GHOSTEX_GXSERVER_BIN` are hard opt-ins. If either points at an invalid path, the launcher fails and does not fall back to TypeScript.
- Relative opt-in paths such as `gxserver-rs/target/debug/gxserver` resolve from development roots.
- Local macOS starts publish explicit gxserver opt-in env vars to LaunchServices and clear stale values when unset.
- The macOS launcher can run either:
  - JavaScript gxserver through the bundled Node runtime, default path.
  - Native/Rust gxserver directly with `--foreground`, explicit opt-in only.
- Rust source builds now report `gxserver:<version>:rust-source` so opt-in clients do not confuse TypeScript and Rust source daemons.
- Rust `gxserver start` refuses to spawn when `127.0.0.1:58744` is already owned by another process or incompatible gxserver build.

## Alternate-Port Approval

`127.0.0.1:58744` is still occupied by the packaged Ghostex daemon, and the user approved using another local port instead of stopping that daemon.

Keep `127.0.0.1:58744` as the default product contract. For Rust port development and compatibility validation, add and use an explicit alternate local port, suggested `127.0.0.1:58746`, so TypeScript and Rust fixtures can be generated while the packaged daemon keeps running. The alternate port must be explicit, not an automatic fallback, and selected Rust launches still must surface Rust startup errors.

After alternate-port support is implemented, run:

```sh
npm --prefix gxserver run build
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0 --port 58746 --update-fixtures
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0 --port 58746
node gxserver-rs/compat/run-compat.mjs --target rust --suite phase0 --port 58746 --bin gxserver-rs/target/debug/gxserver
```

## Opt-In Examples

```sh
GHOSTEX_GXSERVER_BIN=gxserver-rs/target/debug/gxserver gx server status --json
GHOSTEX_GXSERVER_BIN=/absolute/path/to/gxserver-rs/target/debug/gxserver bun run start
```

If Rust is selected on an occupied port, the expected result is a `portConflict` or compatibility-harness port blocker, not a TypeScript fallback.

## Validators

Passed:

```sh
node --check scripts/ghostex-cli.mjs
node --check scripts/start-ghostex.mjs
swiftc -parse native/macos/ghostexHost/Sources/ghostexHost/GxserverClient.swift
bunx vitest run scripts/ghostex-cli.test.mjs native/sidebar/gxserver-rust-port-source.test.ts
cargo fmt --manifest-path gxserver-rs/Cargo.toml
cargo test --manifest-path gxserver-rs/Cargo.toml
node --check gxserver-rs/compat/run-compat.mjs
bun run typecheck
```

Blocked until alternate-port support lands:

```sh
node gxserver-rs/compat/run-compat.mjs --target rust --suite phase0 --bin gxserver-rs/target/debug/gxserver
```

Additional note:

```sh
bun run test
```

The targeted Phase 2 Vitest command passed.

## Next Tasks

1. Implement explicit alternate-port support, suggested `127.0.0.1:58746`, across the compatibility harness, Rust daemon config/CLI, and any TypeScript launch path needed for fixture generation.
2. Generate `phase0-observed-ts.json` on the alternate port.
3. Re-run TypeScript and Rust Phase 0 compatibility suites on the alternate port.
4. Continue Phase 3 using the alternate port while keeping `127.0.0.1:58744` as the default product contract.

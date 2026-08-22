# GXserver Rust Port Phase 0 Handover

## Status

Phase 0 implementation is in place for the Rust port compatibility target:

- `compat/fixtures/phase0-contract.json` inventories the TypeScript contract for lifecycle/health, protocol gates, paths, migrations, representative RPC envelopes, deferred domain CRUD examples, and WebSocket framing.
- `compat/run-compat.mjs` is a black-box runner that can execute the Phase 0 suite against:
  - TypeScript: `node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0`
  - Future Rust binary: `node gxserver-rs/compat/run-compat.mjs --target rust --suite phase0 --bin <path>`
- `IMPLEMENTATION_LOG.md` records the work and validation details for continuation.

## Important Constraint

The fixed local port `127.0.0.1:58744` was occupied by the packaged Ghostex daemon:

`/Applications/Ghostex.app/Contents/Resources/Web/gxserver/dist/src/cli.js --foreground`

The user chose not to stop it, so the observed TypeScript fixture was not generated. When the port is free, run:

```sh
npm --prefix gxserver run build
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0 --update-fixtures
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0
```

That should create `gxserver-rs/compat/fixtures/phase0-observed-ts.json`.

Update, 2026-06-14-21:44: the user approved using an explicit alternate local development/compatibility port instead of waiting for `127.0.0.1:58744` to become free. Keep `58744` as the default product contract, but update the harness and daemon launch paths to support an explicit alternate port, suggested `127.0.0.1:58746`, before regenerating fixtures while the packaged daemon remains running.

## Validation Run

Passed:

```sh
node --check gxserver-rs/compat/run-compat.mjs
node -e "JSON.parse(require('node:fs').readFileSync('gxserver-rs/compat/fixtures/phase0-contract.json','utf8'))"
node gxserver-rs/compat/run-compat.mjs --target ts --suite phase0 --skip-if-port-busy
npm --prefix gxserver run check
git diff --check -- gxserver-rs gxserver/test/api.test.ts
```

Notes:

- The compat run intentionally skipped because port `58744` was busy.
- `npm --prefix gxserver run check` passed after updating one stale zmx lifecycle test expectation in `gxserver/test/api.test.ts`.

## Next Agent Tasks

1. Add explicit alternate-port support to the compatibility harness and daemon launch paths, suggested `127.0.0.1:58746`; do not stop the packaged daemon.
2. Generate `phase0-observed-ts.json` with `--update-fixtures` on the selected alternate port.
3. Re-run the TypeScript compatibility suite without `--update-fixtures` on the selected alternate port.
4. Continue later phases only after the observed TypeScript fixture exists.

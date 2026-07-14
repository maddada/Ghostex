# Ghostex Audit — Verification & Remediation

**Scope reviewed:** `gxserver-rs` backend trust boundaries
**Method:** Read-only verification of every finding against the current cloned source (`5c73715b`), followed by targeted code fixes for the actionable items.
**Toolchain note:** No Rust toolchain is available in this environment, so the two code changes were made by inspection and *have not been compiled here*. Run `cargo build && cargo test -p gxserver` on your side to confirm.

---

## 1. Verification results

Every finding was checked against the actual code. All six findings and the "additional context" note are **accurate** — line references match (the report's `/workspace/Ghostex/...` paths map to the same files in this checkout).

| # | Finding | Severity | Verified | Disposition |
|---|---------|----------|----------|-------------|
| 1 | Clone jobs inherit ambient env despite `env_clear()` | High | ✅ Confirmed | **Fixed** (allowlist) |
| 2 | Managed T3 runtime binds `0.0.0.0:3774` | Medium | ✅ Confirmed | **Fixed** (loopback bind) |
| 3 | `/api/events` accepts `authToken` query param | Medium | ✅ Confirmed | Intentional; mitigations verified in place |
| 4 | Project setup command run via shell | Medium | ✅ Confirmed | Intentional privileged surface; guidance below |
| 5 | Remote listener is declarative, not wired | Informational | ✅ Confirmed | No code defect; documentation item |
| 6 | Admin endpoints declared but `notImplemented` | Informational | ✅ Confirmed | Correct milestone behavior |
| + | Permission checks use `ListenerKind::Local` | Context | ✅ Confirmed | Consistent with local-only runtime |

### Verification detail

- **F1** — `run_clone_process` (`repository_clone.rs:569-578`) calls `.env_clear().envs(repository_clone_environment())`. The helper (`:723-731`) collected **all** `env::vars()` and removed only 3 color variables, so `env_clear()` was effectively a no-op. Confirmed exactly as reported.
- **F2** — `t3_runtime.rs:19-21`: `T3_RUNTIME_HOST = "127.0.0.1"` but `T3_RUNTIME_LISTEN_HOST = "0.0.0.0"`; the launch script (`:1085`) passes `--host 0.0.0.0` while the probe (`t3_http_request`, `:1175-1183`) connects over `127.0.0.1`. Confirmed — the runtime is only ever reached over loopback, so the broad bind was pure surplus exposure.
- **F3** — `server.rs:7260-7266`: authorization accepts either `Authorization` header or `authToken` query value. Confirmed. Log redaction of `authToken` is real (`logging.rs`), and only one **local** (`127.0.0.1:58744`) listener is actually bound.
- **F4** — `typed_operations.rs:450-494`: `worktreeCommand` from `gitConfig` is passed as shell command text to `command_shell()`. Confirmed. Note the result summary redacts the text as `<worktree setup command>`.
- **F5/F6** — Only one listener is bound/served (`server.rs:319-365`). `remote_default()` has `enabled: false` (`protocol.rs:73-84`); config merge discards caller listener overrides (`config.rs:139-147`); admin endpoints map to `remote_blocked` (`protocol.rs:466-475`) and unrouted paths return `notImplemented` (`server.rs:1347-1358`). Confirmed as a declarative/parity gap, not an exploit.
- **Additional context** — `is_remote_endpoint_allowed(ListenerKind::Local, …)` is hardcoded at `server.rs:542` and `:672`, consistent with the single local listener.

---

## 2. Changes applied

### Fix — Finding 1 (High): clone subprocess environment allowlist
**File:** `gxserver-rs/src/repository_clone.rs`

Replaced the "inherit everything minus 3 color vars" logic with an explicit allowlist. `env_clear()` is now meaningful: the clone child receives only variables Git legitimately needs (executable/config discovery, SSH agent, locale, temp dirs, proxy, Windows essentials) plus `LC_*`. Arbitrary ambient state — cloud/CI tokens, `GIT_*`/`GIT_SSH_*` behavior overrides, `LD_PRELOAD`, etc. — is dropped.

Added three unit tests:
- `clone_environment_allowlist_keeps_required_vars`
- `clone_environment_allowlist_drops_sensitive_and_behavior_altering_vars`
- `clone_environment_does_not_leak_ambient_secret` (asserts an injected process-env secret is absent from the clone environment)

> Note on auth-bearing vars: `HOME`, `SSH_AUTH_SOCK`, and proxy vars are deliberately **kept** because SSH/HTTPS clones need them to authenticate at all. The fix removes *unexpected* inheritance and behavior-altering overrides; it does not sever the credentials a clone requires. If you want stricter isolation (e.g. pass credentials only explicitly), that is a larger design change worth a follow-up.

### Fix — Finding 2 (Medium): T3 runtime binds loopback by default
**File:** `gxserver-rs/src/t3_runtime.rs`

Changed `T3_RUNTIME_LISTEN_HOST` from `"0.0.0.0"` to `"127.0.0.1"` with a comment explaining the invariant. Since the only consumer probes over loopback, this is behavior-preserving for legitimate use while removing reachability from other hosts/containers/bridged interfaces. Widening it again should require an explicit, documented remote opt-in with verified per-endpoint auth.

---

## 3. Findings addressed without code change (with rationale)

- **Finding 3 — `/api/events` query `authToken`.** This is a documented browser-WebSocket compatibility requirement (browsers cannot set `Authorization` on WS handshakes). Removing it would break browser clients. The report's own mitigations already hold: the token is redacted in logs, and the daemon binds only a local listener. **Recommendation:** keep the endpoint local-only; if a real remote listener is ever wired, switch browsers to a short-lived one-time WebSocket ticket before exposing it.
- **Finding 4 — shell-executed setup command.** Intentional product feature (`worktreeSetupCommand`). It is dispatch-gated and the command text is redacted in results. **Recommendation (policy, not code):** treat `gitConfig.worktreeCommand` as privileged local-trust input, constrain who can write it, and label it clearly in UX as "runs shell code."
- **Findings 5 & 6 — declarative remote/admin surface.** Not defects: the unimplemented admin endpoints correctly return `notImplemented`/`remote_blocked`, and no second listener is bound. **Recommendation:** document the Rust daemon as local-only and have clients use capability discovery rather than assuming parity from the shared protocol contract.

---

## 4. Suggested next steps

1. `cargo build && cargo test -p gxserver` to confirm the two edits compile and the new tests pass.
2. Commit the two fixes promptly (concurrent agents share this checkout).
3. Decide policy on Finding 4 (config-write restrictions) and Finding 3 (future remote transport) before any release that implies remote/admin parity.

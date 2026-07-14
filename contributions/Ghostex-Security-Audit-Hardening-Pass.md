# Ghostex Security Audit — Hardening Pass (pt-act/Ghostex)

## Scope & method
- **Target:** `pt-act/Ghostex` @ `024b6575` — the five implemented hardening features (doctor, shared paths, capability discovery/conformance, diagnostic export, subprocess policy) plus the CodeQL-alert fix commit.
- **Method:** read-only diff review of the new surface (`3ebde44c..024b6575`): `doctor.rs`, `subprocess_policy.rs`, `paths.rs` (`AgentPaths`), `protocol.rs`, `server.rs` handlers, generator, native sidebar/settings UI, conformance test. No runtime execution (no Rust toolchain in the environment); the highest-severity finding is a logical certainty verified against the code.
- **Baseline:** prior audit findings 1 (clone env) and 2 (T3 bind) are re-checked.

## Executive summary
The implementation is solid and the new endpoints are well-gated. Most of my earlier spec-review concerns were addressed (diagnostics uses a conservative allowlist; doctor fixes are minimal and shell-free; conformance is set-equality both ways). **No new remotely-exploitable vulnerability was found.** However, the subprocess-policy refactor introduced **one real regression that silently breaks SSH-agent git clones**, and Feature 2's anti-drift goal is **incompletely applied to hooks** — ironically re-creating a #58-class path mismatch for the hooks browse catalog.

| # | Finding | Severity |
|---|---------|----------|
| 1 | `SSH_AUTH_SOCK` stripped from clone env by over-broad `"AUTH"` sensitive pattern | **Medium-High (functional regression)** |
| 2 | Native hooks catalog path (`.agents/hooks`) disagrees with canonical `HOOKS_ROOT` (`.ghostex/hooks`) | Low-Medium |
| 3 | Doctor "confirmation token" is a hardcoded constant (security theater) | Low |
| 4 | `/api/capabilities` truthfulness derives from a hand-maintained list, not real dispatch | Low |
| 5 | `write_secret_file` creates-then-chmods (non-atomic) and follows symlinks | Low |
| 6 | `Ssh`/`ProjectSetup` profiles + `log_project_setup_command` are dead code (latent leak) | Info |
| 7 | Doctor offers a `toolchain.install` fix the server never implements | Info |
| 8 | Diagnostic error-log tail may embed local paths (username disclosure) | Info |

---

## Finding 1 — `SSH_AUTH_SOCK` stripped from clone subprocess environment
**Severity: Medium-High (functional regression, security-adjacent)**

`subprocess_policy.rs` filters the environment in two stages: (a) keep keys in the profile allowlist or `LC_*`; (b) drop any key whose **uppercased name contains** a `SENSITIVE_KEY_PATTERNS` entry. That list contains `"AUTH"`:

```rust
const SENSITIVE_KEY_PATTERNS: &[&str] = &[ "TOKEN","PASSWORD","SECRET","BEARER","CREDENTIAL","AUTH","API_KEY","APIKEY" ];
// ...
.filter(|(key, _)| !SENSITIVE_KEY_PATTERNS.iter().any(|p| key.to_ascii_uppercase().contains(p)))
```

`"SSH_AUTH_SOCK".to_ascii_uppercase()` contains `"AUTH"`, so it is dropped **even though `CLONE_ENV_ALLOWLIST` explicitly lists it** — stage (b) overrides stage (a). `repository_clone.rs:574` runs real clones through `subprocess_environment(SubprocessProfile::Clone)`, so:

- `git@…` (SSH) clones that rely on `ssh-agent` keys — the common case for private repos — lose `SSH_AUTH_SOCK` and fail to authenticate.
- This is a **regression** from commit `f253650f`, whose allowlist correctly preserved `SSH_AUTH_SOCK`.

The tests miss it: `clone_profile_includes_ssh_auth_sock` asserts the **constant array** contains the key, and `clone_environment_does_not_leak_ambient_secret` asserts absence of a secret — neither asserts that `subprocess_environment(Clone)` actually **yields** `SSH_AUTH_SOCK`.

**Remediation**
- Make the allowlist authoritative: apply the sensitive-key filter only to keys that are *not* explicitly allowlisted (allowlist wins), or
- Narrow `"AUTH"` to avoid collision (e.g., match token-bearing forms like `"AUTH_TOKEN"`, `"_AUTH"` boundaries) so it never shadows `SSH_AUTH_SOCK` / `XAUTHORITY`.
- Add a regression test: with `SSH_AUTH_SOCK` set in the parent, `subprocess_environment(Clone)` must contain it. The same bug is latent in the (currently unused) `Ssh` profile — fix once, centrally.

---

## Finding 2 — Hooks catalog path not migrated; #58-class drift persists for hooks
**Severity: Low-Medium (reliability)**

Feature 2 made `.ghostex/hooks` canonical: `AgentPaths::hooks_root = ~/.ghostex/hooks`, the generated `HOOKS_ROOT = ".ghostex/hooks"`, and the hook **installer** uses it (`agent_hooks.rs` `HookPaths::new` → `agent_paths.hooks_root`). Skills were migrated consistently in `native-sidebar.tsx` (all sites use `SKILLS_ROOT`). **But the agents-hub hooks catalog was not migrated** — it still hardcodes the old path:

```
native/sidebar/native-sidebar.tsx:39907  const hooksRoot = p(".agents", "hooks");
native/sidebar/native-sidebar.tsx:39596  if (isRelativeTo(candidatePath, p(".agents", "hooks"))) {
```

`~/.agents/hooks` ≠ `~/.ghostex/hooks`, so the **Hooks browse tab looks in the wrong directory** and installed hooks won't appear there — exactly the #58 class of "installed but not shown" drift the feature set out to eliminate. (The server-side hook *status* and the doctor `hooks` check both use `.ghostex/hooks`, so the status **badge** is correct; only the catalog browse root drifts.)

Compounding it, the drift test (`generated_ts_matches_rust_agent_paths`) only asserts the generated TS constants exist — it does **not** verify `native-sidebar.tsx` actually imports/uses them, so it gives false confidence and would not catch this.

**Remediation**
- Replace both `p(".agents","hooks")` occurrences with the interpolated/imported `HOOKS_ROOT`.
- Strengthen the guard: add a lint/test that fails if `native-sidebar.tsx` contains a hardcoded agent-path string literal (`"agents"`, `"hooks"`, `.agents/…`) outside the generated import.

---

## Finding 3 — Doctor "confirmation token" is a hardcoded constant
**Severity: Low**

`DoctorFix.confirmation_token` is a fixed string (`"reinstall-skills"`, `"reinstall-hooks"`), and `handle_doctor_fix_http` matches `(fixId, confirmationToken)` against those constants. This provides **no** anti-CSRF/anti-replay/consent value beyond the auth gate; any authenticated caller can pass the known constant. The genuine protection — `FullLocal` + `requires_auth=true` — is present and sound, and the only actions are idempotent skills/hooks reinstall, so impact is low.

**Remediation:** either drop the "token" framing (it's really a fix identifier), or, if real confirmation semantics are wanted, issue a per-run server-generated nonce returned by `/api/doctor/run` and require it on `/api/doctor/fix`.

---

## Finding 4 — `/api/capabilities` truthfulness is list-driven, not dispatch-driven
**Severity: Low**

The handler computes `implemented` vs `notImplemented` from the static `known_not_implemented_endpoints()` list against `all_ts_endpoint_paths()`. It does **not** consult actual route dispatch. An endpoint that has a routing descriptor but no arm in the big `server.rs` match (falling through to `_ => NOT_IMPLEMENTED`) and is absent from the static list would be advertised as `implemented` while returning 501 at runtime — the exact "contract lies about runtime" gap the feature was meant to close. The conformance test validates the two lists against `endpoint_for()`, not against handler presence in dispatch.

**Remediation:** derive capabilities from (or add a test that exercises) the real dispatch outcome, so the static list cannot silently drift from handler reality.

---

## Finding 5 — `write_secret_file` is non-atomic and symlink-following
**Severity: Low (hygiene)**

```rust
let mut file = fs::File::create(path)?;              // default umask perms, empty
file.set_permissions(Permissions::from_mode(0o600))?; // tighten
file.write_all(contents.as_bytes())?;                 // then write secret
```

Contents are written only after the chmod, so no secret is exposed at broad perms — but the file briefly exists empty at default perms, and `File::create` follows symlinks (a pre-planted symlink at `path` could redirect the write; low risk since the parent is app-owned under `~/.ghostex`). Prefer atomic creation:

```rust
OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
```

The Windows branch is a documented TODO (no ACL restriction yet) — acceptable if tracked.

---

## Finding 6 — Dead code: `Ssh` / `ProjectSetup` profiles and setup-command logging
**Severity: Info (latent)**

Only `SubprocessProfile::Clone` is wired (`repository_clone.rs`). `SubprocessProfile::Ssh`, `SubprocessProfile::ProjectSetup`, and `log_project_setup_command` are unreferenced. Two latent concerns for when they are wired:
- `log_project_setup_command` prints the **full command text to stderr, unredacted**, and explicitly defers structured-logging integration — this would put setup-command contents (potentially secrets a user embedded) into stderr/support bundles.
- The `Ssh` profile inherits Finding 1's `SSH_AUTH_SOCK` bug.

**Remediation:** either wire and harden them or mark clearly as unimplemented; fix Finding 1 centrally before enabling `Ssh`.

---

## Finding 7 — Doctor offers a fix the server can't apply
**Severity: Info (UX/correctness)**

`check_toolchain()` returns a fix with `confirmation_token: "install-tools"`, but `handle_doctor_fix_http` has no matching arm, so triggering it returns `badRequest`. Remove the offer or implement the handler.

---

## Finding 8 — Diagnostic bundle error-log tail may disclose local paths
**Severity: Info**

`exportDiagnostics` is otherwise conservative (config summary limited to `listeners`+`product`; skills/T3 as counts/booleans; server-generated `serverId`; no tokens). The `recentErrors` tail is reused from the redacted log query, but error messages can still embed `~/…` filesystem paths that reveal the OS username. Acceptable for a user-initiated self-service bundle; consider a home-path scrub if bundles are meant to be posted publicly.

---

## Positive confirmations (what's done well)
- **Endpoint gating is correct.** `full_local` and `remote_allowed` both set `requires_auth=true`; `/api/doctor/run|fix|exportDiagnostics` are `FullLocal` (remote-blocked); `/api/capabilities` is `RemoteAllowed` and returns only a `path → implemented|notImplemented` map (no permission levels, no internals). No unauthenticated exposure.
- **Diagnostic export redaction** uses an export-time allowlist and reuses the redacted log query — directly addresses the earlier spec-review concern that config/T3/agent-IDs would bypass log redaction.
- **Doctor fix surface is minimal and shell-free** — only idempotent skills/hooks reinstall; the spec's riskier "open System Settings"/"show conflicting PID"/toolchain-install fixes were not wired (good attack-surface reduction).
- **No XSS in the doctor/diagnostics UI** — `check.detail` renders as escaped React text; diagnostics use `navigator.clipboard.writeText`.
- **CodeQL commit `024b6575` fixes 3 real DOM-injection sinks correctly** — backslash-escaped-before-quote ordering in `formatMarkdownQuote`; protocol allowlist (`http:`/`https:`/`data:image/`) before `img.src`; `data:image/` prefix check on favicon. Skips (vendored/test/dev) documented in `codeql.txt`.
- **Prior findings intact:** T3 loopback bind (`127.0.0.1`) preserved; T3 env migration correctly deferred (avoids breaking the `zsh -lic` login-shell env); clone-env allowlist approach retained (modulo Finding 1).

## Recommended priority
1. **Fix Finding 1** (SSH_AUTH_SOCK) — it silently breaks private SSH clones; add the runtime regression test. Highest priority.
2. **Fix Finding 2** (hooks catalog path) — finish the Feature 2 migration and add the hardcoded-path lint.
3. Findings 3–5 as hardening; 6–8 as cleanup.

*No Rust toolchain was available; findings are by inspection. Finding 1 is a logical certainty; recommend confirming with a `cargo test` that asserts `SSH_AUTH_SOCK` survives `subprocess_environment(Clone)`.*

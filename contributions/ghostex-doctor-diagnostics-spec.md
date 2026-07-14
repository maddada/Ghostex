# Ghostex Doctor & Diagnostics — Implementation Spec

**Date:** 2026-07-14
**Status:** Implemented and validated
**Scope:** `gxserver-rs` backend doctor endpoints, native sidebar RPC bridge, Settings modal Support tab

---

## 1. Overview

Adds a system health checker ("Doctor") and diagnostic bundle exporter to Ghostex, accessible from the Settings modal's new Support tab. The doctor runs five invariant checks, surfaces results with optional one-click fixes, and exports a structured diagnostics bundle for troubleshooting.

---

## 2. Architecture

```
Settings Modal (Support tab)
  │
  ├── Run Doctor ──→ vscode.postMessage({ type: "runDoctor" })
  │                    → native-sidebar.tsx → gxserverClient.rpc("/api/doctor/run")
  │                    ← { checks: DoctorCheck[] }
  │
  ├── Apply Fix ──→ vscode.postMessage({ type: "applyDoctorFix", fixId, confirmationToken })
  │                   → native-sidebar.tsx → gxserverClient.rpc("/api/doctor/fix", { fixId, confirmationToken })
  │                   ← { applied: true, fixId }
  │
  └── Copy Diagnostics ──→ vscode.postMessage({ type: "exportDiagnostics" })
                            → native-sidebar.tsx → gxserverClient.rpc("/api/doctor/exportDiagnostics")
                            ← { version, protocolVersion, configSummary, recentErrors, t3Status, skillsSummary, serverId, startedAt }
```

All communication flows through the webview ↔ native bridge (`vscode.postMessage` → `native-sidebar.tsx` handler → `gxserverClient.rpc()` → response → `postAppModalHost()` → `modal-host.tsx` state → `SettingsModal` props).

---

## 3. Features

### F1 — Doctor Checks (`/api/doctor/run`)

**Endpoint:** `POST /api/doctor/run` (FullLocal only)
**Response:** `{ checks: DoctorCheck[] }`

Five checks run on every invocation:

| Check ID | What it verifies | Fix available |
|----------|-----------------|---------------|
| `skills.installed` | All bundled agent skills are installed | `skills.reinstall` |
| `hooks.installed` | All agent hooks are installed | `hooks.reinstall` |
| `toolchain.present` | zmx, zehn, bd are on PATH | `toolchain.install` |
| `daemon.running` | gxserver daemon status | No (informational) |
| `t3.running` | T3 runtime status | No (informational) |

**DoctorCheck type (Rust):**
```rust
#[serde(rename_all = "camelCase")]
pub struct DoctorCheck {
    pub id: String,
    pub status: CheckStatus,  // Ok | Warn | Fail
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<DoctorFix>,
}
```

**Invariant:** `Ok` status checks must NOT have a `fix` present. `Warn`/`Fail` checks may optionally have a fix. Enforced by `validate_check_invariants()`.

### F2 — Doctor Fix (`/api/doctor/fix`)

**Endpoint:** `POST /api/doctor/fix` (FullLocal only)
**Request:** `{ fixId: string, confirmationToken: string }`
**Response:** `{ applied: true, fixId: string }` on success; `badRequest` RPC error on failure

Validated fix ID + confirmation token pairs:

| fixId | confirmationToken | Action |
|-------|-------------------|--------|
| `skills.reinstall` | `reinstall-skills` | Calls `install_agent_skills()` |
| `hooks.reinstall` | `reinstall-hooks` | Calls `install_agent_hooks()` |

Any other pair returns a `badRequest` error. The confirmation token prevents accidental or unauthorized fix application.

### F3 — Diagnostics Export (`/api/doctor/exportDiagnostics`)

**Endpoint:** `POST /api/doctor/exportDiagnostics` (FullLocal only)
**Response:** Bundle object directly (wrapped in RPC envelope)

Bundle fields:
- `version` — gxserver version
- `protocolVersion` — protocol version constant
- `configSummary` — sanitized config (listeners, product only)
- `recentErrors` — last 50 error-level log entries
- `t3Status` — T3 runtime status (running, pid, port)
- `skillsSummary` — skills count (total, installed)
- `serverId` — server identifier
- `startedAt` — server start timestamp

### F4 — Support Tab UI

**Location:** Settings modal → Support sidebar page (tab icon: `IconStethoscope`)

Components:
- **Run Doctor button** — triggers check execution, shows "Running..." while loading
- **Copy Diagnostics button** — copies bundle JSON to clipboard via `navigator.clipboard.writeText()`, falls back to native export if no cached data
- **Check Results** — card list with status icons (green ✓ / yellow ⚠ / red ✗), check ID, detail text
- **Fix buttons** — two-step confirmation: click fix description → Confirm/Cancel appears → confirm triggers `applyDoctorFix`

---

## 4. Types (TypeScript)

### Shared contract (`session-grid-contract-sidebar.ts`)

```typescript
export type SidebarDoctorCheck = {
  id: string;
  status: "ok" | "warn" | "fail";
  detail: string;
  fix?: {
    id: string;
    description: string;
    confirmationToken: string;
  };
};

export type SidebarDoctorChecksResultMessage = {
  checks: SidebarDoctorCheck[];
  type: "doctorChecksResult";
};

export type SidebarDoctorFixResultMessage = {
  ok: boolean;
  error?: string;
  type: "doctorFixResult";
};

export type SidebarDiagnosticsExportResultMessage = {
  ok: boolean;
  json?: string;
  error?: string;
  type: "diagnosticsExportResult";
};
```

### Webview → Native requests (`SidebarToExtensionMessage`)

```typescript
| { type: "runDoctor" }
| { type: "applyDoctorFix"; fixId: string; confirmationToken: string }
| { type: "exportDiagnostics" }
```

### Settings modal props (`SettingsModalProps`)

```typescript
doctorChecks?: SidebarDoctorCheck[];
doctorLoading?: boolean;
diagnosticsJson?: string;
diagnosticsLoading?: boolean;
onRunDoctor?: () => void;
onApplyDoctorFix?: (fixId: string, confirmationToken: string) => void;
onExportDiagnostics?: () => void;
```

---

## 5. Files changed

### Backend (Rust)

| File | Change |
|------|--------|
| `gxserver-rs/src/doctor.rs` | DoctorCheck/DoctorFix structs, 5 check functions, validate_check_invariants, run_doctor_cli (366 lines) |
| `gxserver-rs/src/server.rs` | handle_doctor_run_http, handle_doctor_fix_http, handle_export_diagnostics_http handlers |
| `gxserver-rs/src/protocol.rs` | `/api/doctor/run`, `/api/doctor/fix`, `/api/doctor/exportDiagnostics` as FullLocal endpoints |
| `gxserver-rs/src/paths.rs` | AgentPaths with `agents_root` field |
| `gxserver-rs/src/lib.rs` | `pub mod doctor;` declaration |
| `gxserver-rs/src/cli.rs` | `doctor` CLI subcommand |

### Frontend (TypeScript)

| File | Change |
|------|--------|
| `shared/session-grid-contract-sidebar.ts` | SidebarDoctorCheck type, 3 result message types, 3 request types |
| `shared/ghostex-settings.ts` | Added `"support"` to SETTINGS_MODAL_NAVIGATION_TABS |
| `sidebar/settings-modal.tsx` | SupportSettingsTab component, doctor props, IconStethoscope import |
| `native/sidebar/modal-host.tsx` | Doctor state, type guards, message handlers, SettingsModal props |
| `native/sidebar/native-sidebar.tsx` | runDoctorChecks, applyDoctorFix, exportDiagnosticsJson handlers + switch cases |

### Other

| File | Change |
|------|--------|
| `.gitignore` | Added `.validator-memory/` |

---

## 6. Validation

### Rust tests
- `cargo test` — **480 passed, 0 failed**
- Doctor-specific tests: `check_invariants_fix_present_only_for_warn_or_fail`, `check_invariants_detects_ok_with_fix`, `check_invariants_allows_fail_without_fix`, `toolchain_check_returns_fail_when_tools_missing`, `daemon_check_returns_valid_check_even_when_down`

### TypeScript tests
- `bun test shared/gxserver-protocol-conformance.test.ts` — **4 passed, 106 assertions**

### Validator-fixer MCP
- `session-grid-contract-sidebar.ts` — APPROVE (0 errors)
- `settings-modal.tsx` — APPROVE (0 errors)
- `native-sidebar.tsx` — APPROVE (0 errors)
- `modal-host.tsx` — 1 false positive (useState misidentified as function declaration)
- Rust files — false positives (validator doesn't support Rust)

---

## 7. Design decisions

1. **Confirmation tokens for fixes.** Each fix requires a matching `fixId` + `confirmationToken` pair. This prevents accidental fix application and ensures the UI has explicitly presented the fix to the user before execution.

2. **Re-run doctor after fix.** When a fix succeeds, the modal-host automatically re-triggers `/api/doctor/run` to refresh the check results, giving immediate feedback.

3. **Clipboard-first diagnostics.** The Copy Diagnostics button tries `navigator.clipboard.writeText()` first (instant, no native bridge needed). Only falls back to the native export path if no cached diagnostics JSON exists.

4. **Informational checks without fixes.** Daemon and T3 status checks are informational — they report status but don't offer automated fixes, since the appropriate action varies by user setup.

5. **Invariant: Ok checks have no fix.** `validate_check_invariants()` enforces that `CheckStatus::Ok` checks must not carry a `fix` field. This prevents the UI from showing misleading fix buttons for healthy checks.

# Issue #38 — Move macOS menu bar into a gxserver-owned agent: design decisions

> Decision draft for the proposed `GxserverBar.app` menu-bar agent. This resolves the 7 open design questions so the feature is ready to build. **No implementation is included** — this is a specification/decision record. The full implementation plan lives on branch `gxserver-menubar` at `docs/plans/2026-06-12-001-feat-gxserver-menubar-agent-plan.md`.

## Goal recap

Move the macOS menu-bar status item out of `Ghostex.app` into a standalone `LSUIElement` agent, `GxserverBar.app`, that:
- controls the daemon (Start / Stop / Restart / Open Logs),
- shows session-status indicators (attention / working / available) driven by gxserver presentation state over `127.0.0.1:58744`,
- survives the main app closing (the daemon already does — see `AppDelegate.swift:769`).

Planned units: **U1** status API + broadcast, **U2** the agent app, **U3** startup bootstrap, **U4** build/staging (macOS-only), **U5** cross-process focus via `ghostex://`, **U6** remove `SessionStatusIndicatorController` (keep floating indicators).

## Decisions on the 7 open questions

### 1. Reboot persistence
**Decision: register a `LaunchAgent` plist in `~/Library/LaunchAgents`, owned and written by gxserver, with `RunAtLoad` + `KeepAlive` scoped to the daemon's lifetime.**
- `SMAppService.loginItem` can't register an arbitrary staged/relocated app bundle, so it's out.
- gxserver already owns runtime metadata; it writes the plist pointing at the staged `GxserverBar.app` launcher and `launchctl bootstrap`s it. On uninstall, gxserver removes the plist (ties into decision 7's cleanup).
- Do **not** relocate the bundle into `/Applications` automatically; keep it staged alongside the daemon and reference it by absolute path in the plist.

### 2. Status-item click vs. attached menu
**Decision: attached menu is primary; no separate left-click action.**
- AppKit can't cleanly offer both a direct click action and an attached `NSMenu` on one `NSStatusItem`.
- The control surface (Start/Stop/Restart/Open Logs + status list) is inherently a menu, so bind the menu directly. If a "focus most-urgent session" quick action is wanted later, add it as the **first, bold menu item** rather than a click handler.

### 3. Dev/prod flavor isolation
**Decision: no hardcoded bundle id / port / token. Derive all three from the daemon flavor at launch.**
- The agent must accept its gxserver base URL, port, token path, and bundle id **suffix** from the environment/launch arguments provided by the bootstrapping daemon (e.g. `ghostex` vs `ghostex-dev`).
- Bundle id: `com.madda.ghostex.gxserverbar` and `com.madda.ghostex-dev.gxserverbar`. Never assume `58744`; read the flavor's configured local port.
- This prevents a dev agent from controlling the prod daemon and vice-versa.

### 4. Preference source for `size` / `hideMenuBarIndicators`
**Decision: gxserver is the single source of truth; the agent reads them from the status API (U1), not from app preferences.**
- The agent has no access to the main app's `UserDefaults` domain and must not duplicate preference storage.
- Fold `size` and `hideMenuBarIndicators` into the `/api/ui/statusIndicators` payload (or a sibling `/api/ui/menuBarConfig`) and push changes over the existing `/api/events` broadcast so the agent updates live.

### 5. `ghostex://` focus-session route
**Decision: add a `focus-session` route to the existing scheme before U5 depends on it.**
- The scheme currently handles terminal/open/edit only. Add `ghostex://focus-session?sessionId=…` handled by `Ghostex.app` (launching it if needed), reusing the existing session-focus path.
- The agent opens this URL to bring a session forward; keeps cross-process focus in one well-defined channel.

### 6. Floating-indicator protocol split (U6)
**Decision: split `SetSessionStatusIndicators` and the shared handler at `AppDelegate.swift:2161` so floating/in-window indicators stay in `Ghostex.app` while the menu-bar item moves to the agent.**
- Extract a shared status model; route menu-bar rendering to the agent (via U1 data) and keep floating-window rendering in the app.
- Removing `SessionStatusIndicatorController` (U6) must not remove the floating indicators — verify they still render after the split.

### 7. Security hardening
**Decision: enforce all of the following before shipping.**
- Token + runtime metadata files created mode **`0600`**, owner-only.
- The agent asserts its gxserver base URL is **loopback** (`127.0.0.1`) and refuses any non-loopback host — consistent with the T3/loopback posture already adopted elsewhere in gxserver.
- Validate/normalize any path the agent opens (logs, launcher) and any `ghostex://` URL it constructs (allowlist scheme + known routes + numeric `sessionId`).
- On uninstall/flavor removal, the daemon removes the LaunchAgent plist and `launchctl bootout`s the agent (login-item cleanup).
- The agent holds only the bearer token needed for the local status/control endpoints; it never persists it outside the `0600` metadata file.

## Scope confirmations (unchanged from the proposal)

- **Headless gxserver** (Homebrew/tarball, no `.app`): bootstrap must **no-op** when `GxserverBar.app` is absent. macOS-only; other platforms skip U1–U6 entirely except the platform-neutral U1 API, which is harmless.
- **Remote / connection-profile instances: out of scope.** The agent controls only the **local** daemon.

## Suggested build order

U1 (status API + broadcast) → U4 (build/stage agent, macOS-only) → U2 (agent app) → U3 (daemon bootstrap + LaunchAgent) → U5 (`ghostex://focus-session`) → U6 (remove controller, keep floating). Land U1 first so the app and agent share one status contract.

## Open confirmation for the maintainer

- OK to write a user-scoped `LaunchAgent` plist from the daemon (decision 1)? If login-item UX is strongly preferred, the alternative is shipping `GxserverBar.app` as a real installed bundle so `SMAppService` can manage it — larger packaging change.

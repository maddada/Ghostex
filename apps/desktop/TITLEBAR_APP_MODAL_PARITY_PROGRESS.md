# GPUI Titlebar App Modal Parity Progress

<!--
CDXC:GPUITitlebarAppModalParity 2026-06-24-10:41:
GPUI titlebar modal parity must reuse the macOS React modal components and route gxserver-backed behavior through production-style bridges instead of placeholder UI. Keep this file as the orchestration ledger while one worker sub-agent at a time ports production behavior into GPUI, with no app restart, no per-slice verification commands, no temporary/stub implementations, and no broad fallback paths.
-->

## Goal

Bring the GPUI app titlebar and related modal UI/UX to production parity with the macOS app for this area. The GPUI titlebar should open the same app modals and gxserver-backed flows that the macOS titlebar opens, using shared/cross-platform code where it makes sense and the same React modal components that the macOS app uses.

## Working Rules

- The main agent is the orchestrator and does not implement product code directly.
- Run exactly one worker sub-agent at a time.
- Assign L-sized, bounded implementation slices with clear file ownership.
- Workers must not revert or overwrite existing uncommitted work from the user or other agents.
- Workers should append an entry here before finishing.
- Do not run verification commands after each slice; defer checks until the user asks.
- Do not run `bun run start` or restart the app.
- Use real production wiring, not temporary stubs or fallback behavior.
- Use CEF/GPUI-owned normal-layout windows or child surfaces for GPUI; do not use WKWebView/WebKit, transparent overlays, hidden hit-test regions, or synthetic mouse routing.
- Persistent logs and progress notes must not include private user content such as project paths, terminal text, URLs with queries, tokens, credentials, command text, or raw payloads.

## Implementation Slices

1. GPUI app-modal host foundation
   - Add a GPUI-owned app-modal host/window controller using CEF.
   - Load the shared React modal host bundle used by the macOS app.
   - Add a CEF-compatible `ghostexAppModalHost` bridge with the macOS lifecycle messages.
   - Wire Settings, Hotkeys, and Command Palette from the GPUI titlebar through this host.

2. Shared modal host bundling
   - Make the modal-host HTML/entry available to the GPUI Vite/CEF build without duplicating React modal components.
   - Keep aliases and styles shared with the existing macOS/sidebar modal source.
   - Preserve cross-platform-friendly boundaries.

3. Sidebar state and gxserver command routing
   - Provide GPUI modal host state compatible with the production sidebar store.
   - Route gxserver-backed modal commands such as Previous Sessions through the existing gxserver client/protocol shape.
   - Dispatch modal result events back into the shared React modal host.

4. Titlebar menu/dropdown parity
   - Replace inert GPUI titlebar buttons with typed actions matching macOS behavior.
   - Add Settings menu actions for Settings, Commands, Hotkeys, Configure Actions, and Open Targets.
   - Add Previous Sessions and other titlebar modal entries where macOS exposes them.

5. Resources/daemon and remaining gxserver-backed parity
   - Hook resources/daemon session modals or panels to real GPUI/gxserver state.
   - Avoid unsupported kill/focus controls until GPUI owns equivalent runtime/session authority.
   - Keep unsupported actions honest and product-correct instead of hidden behind fallbacks.

## Progress Log

### 2026-06-24-10:41 Main orchestrator

- Created this dedicated progress ledger for GPUI titlebar app-modal parity.
- Confirmed the current GPUI titlebar buttons are still inert and that existing uncommitted GPUI work is focused on project workarea CEF surfaces, not app modal parity.
- Next worker slice: GPUI app-modal host foundation with Settings, Hotkeys, and Command Palette titlebar routes.

### 2026-06-24-11:09 Stabilization - GPUI App-Modal Host Foundation

- Completed titlebar/modal-host slice: the GPUI titlebar Settings glyph now opens an OS-owned NativeMenu with typed Settings, Hotkeys, and Command Palette actions, and all three actions route through the shared GPUI CEF app-modal window instead of inert controls or placeholder UI.
- Fixed foundation coherence: the bundled `modal-host.html` entry stays a thin wrapper around `native/sidebar/modal-host.tsx`, the CEF `ghostexAppModalHost` shim is documented as limited to bundled `modal-host.html` and sidebar `index.html`, and only the modal-host entry receives native-window identity fields.
- Exact files touched: `gpui/src/main.rs`, `gpui/src/cef/macos.rs`, `gpui/modal-host.html`, `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`, and `gpui/SETTINGS_PARITY_PROGRESS.md`.
- Remaining gaps: `updateSettings` persistence/fan-out is still not wired; Settings status/action bridges remain minimal; Previous Sessions, daemon/resources, gxserver commands, and full settings fan-out remain out of scope for this slice.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-11:53 Worker 6A - Previous Sessions App-Modal and Local Gxserver Query

- Added the GPUI Previous Sessions app-modal kind, titlebar NativeMenu action, and action listener so GPUI opens the existing shared React Previous Sessions modal in the owned CEF app-modal window.
- Routed app-modal `requestPreviousSessions` sidebar commands to local gxserver `/api/listPreviousSessions` using the existing typed-operation helper with previous-only, bounded query params, then dispatched `previousSessionsResult` back as a transient sidebarState message.
- Projected gxserver search rows into the existing `SidebarPreviousSessionItem` shape with metadata-only fields, and made gxserver/token/network/parse failures return an empty result for the matching request so the modal clears loading without logging private daemon details.
- Handled Previous Sessions restore/delete commands through real local gxserver `/api/createSession` and `/api/removeSession` calls for canonical `gxserver:<projectId>:<sessionId>` rows; the legacy Search by Text command remains a harmless no-op because the modal no longer renders that action and GPUI lacks current-project launch authority for it here.
- Exact files touched: `gpui/src/main.rs` and `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-12:00 Worker 7A - Running Sessions App-Modal and DaemonSessions State

- Added the GPUI Running Sessions app-modal kind, titlebar NativeMenu action, and action listener so GPUI opens the existing shared React `daemonSessions` modal in the owned CEF app-modal window.
- Routed app-modal `refreshDaemonSessions` to a transient `daemonSessionsState` sidebarState payload built from real local gxserver `/api/health/server` and `/api/readPresentationSnapshot` data when available, without replacing the stored Settings hydrate snapshot.
- Projected gxserver workspace terminal/agent presentation rows into `SidebarDaemonSessionItem` metadata with `ownership: "gxserver"`, live restore state, stable ids, public cwd/project path fields when present, and explicit zero dimensions until gxserver exposes real terminal geometry.
- Handled `killDaemonSession` through gxserver `/api/transitionSession` with `action: "close"` for rows carrying project/session ids, then refreshed state. `killTerminalDaemon` refreshes with an explicit unsupported error message instead of fake success.
- Exact files touched: `gpui/src/main.rs` and `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-12:11 Worker 8A - Pinned Prompts and Scratch Pad App-Modal Product State

- Added GPUI `pinnedPrompts` and `scratchPad` app-modal kinds, titlebar NativeMenu actions, titlebar action listeners, and command/hotkey bridge routes so the existing shared React modals open from GPUI titlebar, command palette, and app-modal open messages.
- Added a centralized GPUI app-modal product-state path under the shared Ghostex home `state/` area, separate from Settings JSON and logs, for Scratch Pad content and Pinned Prompts data.
- Hydrated `pinnedPrompts` and `scratchPadContent` from real persisted GPUI product state in the shared app-modal sidebar hydrate while preserving Settings, Previous Sessions, Running Sessions, Portless, and project-settings hydrate behavior.
- Routed `saveScratchPad` and `savePinnedPrompt` sidebar commands to product-state persistence, including prompt create/update, ISO timestamps, title normalization from content, newest-updated-first ordering, malformed non-string payload rejection, and post-save modal hydrate refresh without logging user content.
- Exact files touched: `gpui/src/main.rs`, `gpui/src/shared_settings.rs`, and `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-12:22 Worker 9A - Settings Entry App-Modal Parity

- Added GPUI app-modal kinds for `configureAgents`, `configureActions`, and `openTargets` so bridge-open messages from the shared command palette and modal host no longer get ignored.
- Routed those ids through the same shared Settings-sized CEF app-modal host as Settings and Hotkeys, including latest sidebar hydrate attachment and Settings-agent gxserver reconciliation before open.
- Added typed GPUI NativeMenu actions/listeners for Configure Agents, Configure Actions, and Open Targets, and mapped `runGhostexHotkeyAction` ids `configureAgents`, `configureActions`, `actions`, and `openTargets` to the same production modal route.
- Exact files touched: `gpui/src/main.rs`, `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`, and `gpui/SETTINGS_PARITY_PROGRESS.md`.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-12:26 Worker 10A - Agents Hub App-Modal Route and Bridge

- Added the GPUI `agentsHub` app-modal kind so command-palette/app-modal bridge opens, hotkey action ids, and the Settings utility NativeMenu can open the existing shared React Agents Hub modal in the owned CEF app-modal host.
- Routed Agents Hub sidebar commands to real GPUI Rust handlers: metadata-only catalog generation, selected-file content reads, validated file saves, and bounded OS opener calls for catalog-approved files/profile/group paths.
- Kept file bodies out of the open/catalog hydrate; content is read only after the shared modal requests the selected file, and save/open boundaries revalidate against the current catalog-derived allowlist before touching disk or invoking the OS opener.
- Exact files touched: `gpui/src/main.rs`, `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`, and `gpui/SETTINGS_PARITY_PROGRESS.md`.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-12:37 Worker 10B - Agents Hub External Editor Command Parity

- Updated the GPUI Agents Hub external edit bridge to validate the selected catalog file first, read the saved Settings editor command, and build the same VS Code-compatible, Zed-compatible, and generic command shapes used by macOS.
- Kept OS default opener behavior on the path-in-Finder/folder-open path only; the external editor button now invokes the configured command through `/bin/zsh -lc` with safely quoted validated folder/file arguments and suppressed stdio.
- Files touched: `gpui/src/main.rs`, `gpui/src/shared_settings.rs`, `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`, and `gpui/SETTINGS_PARITY_PROGRESS.md`.
- Remaining gaps: this slice did not add verification coverage or run the app; behavior still depends on the user's configured editor command being available in the shell environment.
- Verification: no verification commands were run; no cargo check/test/fmt, bun/npm build/typecheck/test, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-12:50 Worker 11 - Titlebar Open In Runtime Parity

- User-facing behavior delivered: the visible GPUI titlebar folder/Open In button is no longer inert. Left click launches the active/default visible Open In target for the active project, and right click opens an OS-owned NativeMenu with visible Open In targets plus Configure.
- High-level technical approach: ported the shared Open In built-in catalog and settings semantics narrowly into Rust, including hidden ids, availability resolved ids/commands/app names, always-available Open Folder, normalized custom targets after built-ins, process-local active target selection, typed NativeMenu target actions, and bounded native process launching with suppressed stdio and private-data-free notifications.
- Exact files touched: `gpui/src/main.rs`, `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`, and `gpui/SETTINGS_PARITY_PROGRESS.md`.
- Remaining gaps: no broader titlebar controls were changed; terminal Actions, Resources, Keep Awake, Tips, Git, and settings fan-out remain outside this slice. Runtime validation of installed editor/app availability was deferred.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-14:24 Worker 12 - Titlebar Actions Runtime Parity

- User-facing behavior delivered: the visible GPUI titlebar Actions play button is no longer inert. Left click runs the selected/last configured action when available, otherwise the first configured action, and opens Settings directly to the Actions tab when no configured action exists. Right click opens an OS-owned NativeMenu of configured actions plus Configure.
- High-level technical approach: reused the gxserver/sidebar-projected command button contract for action definitions; browser actions switch/wake the GPUI Browser and load the saved URL through the existing Browser CEF path; terminal actions create a command-pane tab and attach the saved command through the explicit command-terminal launch-payload source for that exact command slot.
- Privacy/layout behavior: no persistent logs, OS browser opens, shell-outs from the titlebar, React overlays, hidden hit regions, synthetic routing, fake run-state success, command-text notifications, or durable command/URL persistence were added.
- Exact files touched: `gpui/src/main.rs` and `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`.
<!--
CDXC:GPUITitlebarActions 2026-06-27-09:10:
Workspace parity slices 489, 507, and 510 superseded Worker 12's terminal Action limitation by adding Action run id/status-file lifecycle, mounted idle Action reuse/write into the exact command surface, and stale command-pane HUD filtering.
-->
- Superseded gap: the command-pane Action reuse/run-state/write-into-existing-terminal limitation recorded in Worker 12 is no longer a remaining titlebar gap because later workspace parity slices 489, 507, and 510 delivered the needed Action lifecycle, mounted idle-surface reuse/write, and stale HUD filtering behavior.
- Runtime validation caveat: end-to-end GPUI titlebar Actions menu/run/HUD behavior still has not been validated in this ledger because this slice did not run build, test, app, or browser verification.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-27-09:26 Task Slice - Titlebar Action Debug Rerun Parity

<!--
CDXC:GPUITitlebarActions 2026-06-27-09:26:
GPUI's Rust-owned titlebar Actions button now mirrors the shared command-palette click rule for close-on-exit terminal Actions. Use only process-local sanitized run feedback to decide Debug reruns, and keep sidebar bridge `runMode` payloads authoritative so renderer selectors cannot be reinterpreted by stale titlebar state.
-->

- User-facing behavior delivered: after a close-on-exit terminal Action fails and has no active run, rerunning it from the GPUI titlebar Actions button, menu row, or positional Action hotkey opens the Debug Action path instead of starting another hidden command-pane run. Active/running, successful, browser, and normal terminal Actions still run in Default mode.
- High-level technical approach: added a process-local sanitized Action run-feedback mirror in Rust, updated it from the existing sidebar run-state dispatch/clear boundary, and routed only titlebar/menu/index click sources through a titlebar-specific run-mode resolver before the shared action runner.
- Privacy/layout behavior: stored only command ids, active run ids, and coarse run state; no command text, URLs, cwd/env, paths, status-file paths, terminal output, logs, shell-state data, overlays, hit-test routing, or app restarts were added.
- Verification: `RUSTUP_TOOLCHAIN=1.95.0 cargo fmt --manifest-path gpui/Cargo.toml` passed; `RUSTUP_TOOLCHAIN=1.95.0 cargo fmt --manifest-path gpui/Cargo.toml --check` passed; focused Rust tests for `titlebar_action_click_run_mode_tracks_sanitized_feedback_without_payload_override`, `gpui_sidebar_command_action_parser_accepts_only_matching_action_target`, and the broader `command_` filter passed. No app launch/restart, browser automation, or `bun run start` was run.

### 2026-06-24-14:30 Worker 13 - Visible Resources Titlebar Route

- User-facing behavior delivered: the visible GPUI titlebar Resources button is no longer inert. Left click and right click both open the existing shared React Running Sessions/`daemonSessions` app modal through the GPUI CEF app-modal host.
- High-level technical approach: reused the already-implemented `GpuiAppModalKind::DaemonSessions` route, which refreshes daemon session state from gxserver on open, instead of adding a separate Resources overlay, hidden hit region, transparent view, synthetic coordinate route, placeholder panel, or duplicate React UI.
- Exact files touched: `gpui/src/main.rs` and `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`.
- Remaining Resources dropdown gaps: this is not full macOS Resources dropdown/process monitor parity. GPUI still does not implement process CPU/RAM sampling, dev-server bundles, Portless resources rows, browser/Code resource bundles, gxserver restart controls, or bulk quit/sleep from the Resources titlebar control.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-13:16 Worker 14 - Titlebar Keep Awake Runtime Slice

- User-facing behavior delivered: the visible GPUI Keep Awake titlebar button is no longer inert. When Show Beta Features is enabled and the Keep Awake titlebar control is not hidden, left click and right click open an OS-owned NativeMenu with the shared duration choices, a running-only Don't keep awake row, and Power Settings.
- High-level technical approach: added process-local GPUI Keep Awake runtime ownership that starts `/usr/bin/caffeinate` directly on macOS with fixed argv, suppressed stdio, `-dis` or `-is` according to Settings, and bounded `-t` seconds for the 2-hour and 5-hour durations. Stop and teardown kill only the child process this GPUI instance spawned; no pids or raw settings are persisted.
- App-modal/settings behavior: Power Settings opens the shared Settings modal with `{ modal: "settings", initialSection: "power" }`, and Settings save/poll refresh stops the GPUI-owned runtime if beta is disabled or the titlebar control is hidden.
- Exact files touched: `gpui/src/main.rs`, `gpui/src/shared_settings.rs`, `gpui/TITLEBAR_APP_MODAL_PARITY_PROGRESS.md`, and `gpui/SETTINGS_PARITY_PROGRESS.md`.
- Remaining Keep Awake gaps: GPUI still does not implement the lid-close helper, external-display auto-start, battery-threshold/low-power/user-switch deactivation, delayed-send holds, working-session automatic holds, or runtime validation of the menu/process behavior.
- Verification: no verification commands were run; no build, test, format, typecheck, app launch/restart, browser automation, or `bun run start` command was run.

### 2026-06-24-23:17 GPUI Info Dropdown React Panel

- User-facing behavior delivered: the visible GPUI info glyph now opens a dropdown panel instead of doing nothing, and the panel renders the shared React Tips content from `native/sidebar/titlebar-host.tsx`.
- High-level technical approach: added a controlled `gpui_component::popover::Popover` trigger, a GPUI-owned CEF panel sized like the macOS Tips panel, a thin `titlebar-host.html` GPUI CEF entry, and first-party bridge access for the titlebar host so Tips header commands can open Browser/docs/changelog or shared app-modal setup/video routes.
- Layout/privacy behavior: no AppKit/Swift dropdown window, transparent overlay, hidden hit region, duplicated GPUI tips data, arbitrary browser-page bridge, raw URL bridge, or titlebar command logging was added.
- Verification: `cargo +1.95.0 check --manifest-path gpui/Cargo.toml --bin ghostex-gpui` passed with existing dead-code warnings; `bunx vite build --config gpui/vite.config.ts` passed and emitted `titlebar-host.html`; `git diff --check` passed for the touched dropdown files. No app launch/restart, browser automation, or `bun run start` was run.

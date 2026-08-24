# Onboarding Sandbox — Architecture Spec

Epic: bead `ghostex-s5gr` (label `onboarding-sandbox`). This document is the contract
between the build agents. Read it fully before writing code.

## Goal

A standalone Vite React app (dev-server only, never shipped) that simulates the gpui
macOS app's onboarding experience end-to-end so a human can finally _see and iterate on_
the flow without restarting the real app:

- A fake macOS desktop: wallpaper, menu bar, dock. Clicking the Ghostex dock icon
  "launches" the app and runs the real startup sequencing.
- The real React onboarding modals (`apps/desktop/views/modal-host.tsx` and everything it
  mounts) rendered **unchanged** inside fake NSPanel windows (iframes).
- A simulation engine that is a faithful TS port of the gpui Rust startup/onboarding
  logic — including its bugs/races, because the whole point is to observe today's
  behavior (flags burned before modals show, popups dropped, restart-required paths).
- A control panel with every environment variable (codex CLI installed, hooks installed,
  gxserver health, …), scenario presets, a live editor for the fake persisted state
  file, an annotated event log, and a force-open gallery of every modal.
- Quit/relaunch buttons that reproduce real restart semantics (persisted state survives,
  in-memory suppressions reset).

Run: `bun run sandbox:onboarding` → vite dev server (default port 5199).

## Non-goals

- No production build target, no tests (repo rule: never write tests unless asked).
- No real gxserver, no network. Everything mocked in-page.
- Do not modify any production file. The ONLY exceptions: the root `package.json`
  script addition (foundation agent, targeted Edit) — nothing else outside
  `apps/desktop/test/onboarding-sandbox/`.

## Directory layout + ownership

Files marked (done) already exist — do not rewrite them, only the owner may extend them.
Stubs marked (stub) are placeholders the named owner REPLACES wholesale.

```
apps/desktop/test/onboarding-sandbox/
  SPEC.md                      (done)
  vite.config.ts               (done — foundation may adjust if genuinely needed)
  tsconfig.json                (done)
  index.html                   (done)
  modal-window.html            (done)
  README.md                    foundation creates, integration finalizes
  src/
    main.tsx                   (done) composition shell
    sandbox.css                (done, base) — shared layout vars only
    state/
      types.ts                 (done) ALL shared types — extend only additively
      store.ts                 (done) zustand store — wiring only; DO NOT fork state shape
    engine/                    OWNER: engine agent (bead ghostex-s5gr.8)
      sim-controller.ts        (stub) engine replaces — implements store actions
      … any other engine modules (env-defaults, presets, startup-sequence,
        status-messages, modal-chrome, tips, persistence)
    bridge/                    OWNER: foundation agent (bead ghostex-s5gr.7)
      modal-connections.ts     (stub) foundation replaces
      modal-window-frame.tsx   (stub) foundation replaces
    modal-window/              OWNER: foundation agent
      modal-window-main.ts     iframe entry: shim FIRST, then real modal-host import
    desktop/                   OWNER: desktop agent (bead ghostex-s5gr.9)
      desktop.tsx              (stub) desktop agent replaces; add desktop.css etc.
    controls/                  OWNER: controls agent (bead ghostex-s5gr.10)
      control-panel.tsx        (stub) controls agent replaces; add controls.css etc.
```

Integration agent (bead ghostex-s5gr.11) may touch everything to fix drift.

## The modal pipeline (foundation)

Verified facts about the real modal host (from code research — trust these):

- `apps/desktop/views/modal-host.tsx` self-mounts `<AppModalHost/>` into `#root` at module
  scope (line ~3505). It imports only `packages/core-ui/*`, `packages/shared/*`, react, react-dom, sonner —
  zero native/gxserver transport imports. It imports `packages/core-ui/styles.css` itself.
- Outbound (React→host): `window.webkit.messageHandlers.ghostexAppModalHost.postMessage(msg)`
  — `packages/core-ui/app-modal-host-bridge.ts:304` THROWS if the handler is missing, and the host
  posts `{type:"ready"}` on mount ⇒ the shim MUST be installed before the module is
  imported (mirror `apps/web/src/main.tsx:8` + `apps/web/src/app/app-modal-host-shim.ts`).
- Inbound (host→React): the host listens for
  `window.dispatchEvent(new CustomEvent("ghostex-app-modal-host-message", { detail }))`.
- Globals to set in the iframe before import:
  `window.__ghostex_APP_MODAL_HOST_ID__ = "gpui"`,
  `window.__ghostex_APP_MODAL_HOST_SURFACE__ = "nativeWindow"`.
- Handshake: on `{type:"open", modal}` the host renders only when the modal is
  "renderable"; it then posts `{type:"presented", modal, requestId?}`. For
  `nativeWindow` surface, fit-height modals post one-shot
  `{type:"contentHeightMeasured", height, modal}` → resize the fake panel.
  The real app keeps the NSPanel HIDDEN until `presented` — the sandbox must do the
  same (a spinner-chrome panel until presented is good UX and faithful).
- Renderability gates: `settings`-family and `firstLaunchSetup`/`tipsAndTricks` require
  the sidebar store to be hydrated (`revision > 0`). Send
  `{type:"sidebarState", message: <hydrate>}` before `open`, AND pass the same hydrate
  as `latestSidebarStateMessage` on the open payload for those kinds (the host applies
  it synchronously before opening). Use `createSidebarStoryMessage()` from
  `packages/core-ui/sidebar-story-fixtures.ts` (it has `revision: 1`) as the base hydrate.
- Other outbound messages to route to the engine: `{type:"close"}`,
  `{type:"sidebarCommand", message}` (THE channel for all modal→app commands),
  `{type:"addProjectDialogRequest"|...}` (add-project ops carry `requestId` and are
  answered with matching `*Result` inbound messages), `{type:"debugLog"}`,
  `{type:"downloadGhostexUpdate"}`, `{type:"restartAndUpdateGhostex"}`.

Parent⇄iframe transport: same-origin `window.postMessage` with an envelope marker —
parent→iframe `{__onboardingSandbox:"deliver", windowId, detail}` (shim re-dispatches
`detail` as the CustomEvent), iframe→parent `{__onboardingSandbox:"outbound", windowId,
message}`. `windowId` arrives via the iframe URL query (`modal-window.html?windowId=…`).
The marker matters: the modal host itself re-emits some transient results via
`window.postMessage` internally; ignore anything without the marker.

`bridge/modal-connections.ts` public API (already stubbed — keep signatures):

- `sendToModalWindow(windowId, detail)` — deliver an inbound host message
- `setModalOutboundHandler(handler)` — engine registers one global handler
  `(windowId, message) => void`
- `registerModalIframe(windowId, el)` / `unregisterModalIframe(windowId)` — used by
  `ModalWindowFrame`

`ModalWindowFrame` renders one `SimModalWindow` from the store as a floating macOS-style
panel (title bar + close button + iframe), hidden/loading until `presented`, resized by
`contentHeightMeasured` when `height === "fit"`. Draggable by title bar is a
nice-to-have. Window chrome data (title/size per modal kind) comes from the engine's
modal-chrome table via the store record.

## The simulation engine (faithful port — bugs included)

Port the real sequencing from `apps/desktop/src/app/**` (the former monolithic
`gpui/src/main.rs` has since been split into that module tree). Every step must emit
a `SimEvent` with `codeRef` pointing at the real symbol (e.g.
`apps/desktop/src/app/os_integration.rs start_gpui_first_run_onboarding`) so the log
doubles as documentation of today's flow.

Launch sequence (macOS):

1. `launchApp()` → phase `launching`; two detached tracks start in parallel, mirroring
   `cx.open_window`: **Track A** gxserver bootstrap (`start_gpui_local_gxserver_bootstrap`),
   **Track B** CEF init (`initialize_cef`). Delays from `env.timing`
   (`gxserverProbeMs`, `cefInitMs`) make the race observable and adjustable.
2. Track A after probe, by `env.gxserver.scenario`:
   - `healthyToolsAvailable` → run portless prompt check (no-op today:
     `GPUI_PORTLESS_APP_INTEGRATION_ENABLED = false` — emit an event saying the prompt
     is compile-time disabled and suppressed-until-restart) then call
     `firstRunOnboarding()` (attempt #1).
   - `healthyToolsUnavailable` | `buildMismatch` | `protocolMismatch` → gxserver toast
     ("Updating gxserver…" / "Restarting gxserver" per real copy), simulated daemon
     respawn poll; on success re-run ONLY the portless check — **not** onboarding.
     Emit a warning event: "onboarding skipped this run — real bug, requires app
     restart" (`apps/desktop/src/app/os_integration.rs` area). If `respawnFixesHealth`, the scenario
     heals to healthy for subsequent launches.
   - `spawnFailure` → "gxserver failed" toast, nothing else.
3. Track B after `cefInitMs`: sidebar surface ready → call `firstRunOnboarding()`
   (attempt #2). Phase → `running`.
4. `firstRunOnboarding()` mirrors `start_gpui_first_run_onboarding`:
   - Early-return (with event) if sidebar/CEF not ready — attempt dropped. NOTE the
     real race: whichever attempt runs first consumes the flags; if Track A wins
     before CEF is ready the call is dropped BUT only after… actually in the real code
     the sidebar guard is checked first, so a pre-CEF Track A attempt is a pure no-op.
     Reproduce exactly that.
   - Read the fake state file; in ONE pass compute and persist:
     `tipsAndTricksSeen → true` (silently), `highlightedFeaturesSeenRevision →
"2026-06-16-highlighted-features-launch"` (silently — DiscoverGhostex tour is
     never auto-shown anymore), `openTutorialVideo = firstLaunchSetupSeenRevision !==
"2026-06-18-short-first-launch"` then mark seen, `showOsIntegrationToast =
!osIntegrationOnboardingSeen` then mark seen. Emit events for each burned flag —
     including the "flag persisted BEFORE the modal opens" hazard.
   - Then foreground: OS-integration toast (auto-dismiss) if flagged, and at most
     ONE modal window (fixed 2026-08-19; was a Windows-only video->setup chain and
     nothing but the video everywhere else): `firstLaunchSetup`, whose first page
     is the tutorial video. Gate = `firstLaunchSetupSeenRevision !== <revision>`
     (this revision was presented) OR `!firstLaunchSetupComplete` (the user never
     closed it). The revision marker is burned when the window exists, the
     complete marker when the modal closes.
   - The video iframe points at a player page the host serves from a real origin
     (`tutorialVideoEmbedUrl` in the open payload; sandbox:
     `/tutorial-video-player.html`). A file:// document cannot embed YouTube —
     the player answers "Error 153 - Video player configuration error".
5. Single modal slot: only one `SimModalWindow` may exist from AUTO-opens; an auto-open
   while one is up is DROPPED with a warning event (mirrors `app_modal_window` slot).
   Force-open from the gallery bypasses the slot check (but emit an event noting the
   real app cannot do this).
6. Tips: badge count = unread tips (12 tip ids) + notices derived ONLY from settings
   (debuggingMode, persistence off) — cli/hook notices are NOT probed at startup.
   Opening the tips panel triggers the runtime status probe
   (`request_gpui_titlebar_tips_runtime_status`) → compute cli + missing-hooks notices
   from `env` (rules below) and emit events.

### 2026-08-18 fixes (engine parity update)

The real app's onboarding was fixed on 2026-08-18 and the engine mirrors it. The
original behavior above is kept in this document as the historical description;
where the two differ, this section wins:

- **Markers after display.** The background pass still burns the two silent
  legacy markers (`tipsAndTricksSeen`, `highlightedFeaturesSeenRevision`), but
  `firstLaunchSetupSeenRevision` is persisted only after the tutorial-video
  window was actually created and `osIntegrationOnboardingSeen` only after the
  toast was shown. A dropped auto-open therefore re-offers the video next launch.
- **Once-per-launch guard.** An in-memory guard (checked by the Track A, Track B
  and post-respawn attempts) replaces the old flag-based dedup, which stopped
  working once the markers moved behind the display.
- **Respawn re-runs onboarding.** The daemon-respawn success path calls
  first-run onboarding again (idempotent via guard + markers), so the
  "onboarding skipped this run — restart required" hazard is gone; the event log
  now shows the re-run instead of the warning.
- **Portless deferral.** An occupied app-modal slot defers the portless prompt
  check and re-runs it when the modal closes, instead of dropping it for the
  whole run. Portless itself stays compile-time disabled, and that event stays.
- **Tutorial video is a non-React-host window.** `watchGhostexVideo` is the one
  `GpuiAppModalKind` with `uses_react_modal_host() == false`: its child window
  loads `GHOSTEX_TUTORIAL_VIDEO_URL` (the YouTube _watch page_, not the embed)
  as the top-level document, `is_ready` starts true, and no hydrate/open/
  presented handshake happens. The engine marks such windows `presented`
  immediately and skips message delivery; `modal-chrome.ts` carries their
  `nonReactHostUrl`, and `ModalWindowFrame` points the iframe there. In the
  sandbox that URL is `/yt/watch?v=…`, served by the dev server's reverse proxy
  (`yt-proxy.ts`) so the same page can be framed and stay same-origin. The real
  app injects a trusted `f` key ~1.5s after load to go fullscreen; the sandbox
  dispatches the same key event and additionally enforces its outcome with an
  injected stylesheet, because browsers refuse fullscreen from untrusted events.
  See README.md ("Tutorial video window") for the measured limitations.

`sidebarCommand` handling (from any modal window):

- `requestAgentHookStatus` → progressive per-agent emission mirroring
  `run_gpui_progressive_agent_hook_status_task`: priority order
  `codex, claude, opencode, pi`, then the rest; one merged `agentHookStatus`
  sidebarState message per step, `env.timing.hookStatusPerAgentMs` apart; single-flight.
- `requestGhostexCliStatus` → `ghostexCliStatus` message built from `env`.
- `installAgentHooks {agentIds}` / `installGhostexCli` / `installBrowserControl` /
  `install*Skill` / `uninstallBundledAgentSkill` → after `installActionMs`, mutate
  `env` accordingly, then push refreshed status messages. Install for an agent with
  `cliInstalled:false` yields `cliMissing` (no-op on env).
- `updateSettings` → merge into the fake settings + re-send hydrate.
- Unknown commands → log event (kind `message`) so gaps are visible, never throw.

Status derivation rules (mirror `server/src/agent_hooks/api.rs read_hook_status` +
`gpui_ghostex_cli_status_message` in `apps/desktop/src/app/helpers/os_cli/cli_status.rs`):

- per-agent status: `cliMissing` if `!cliInstalled`; else `installed` if
  `hookState === "installed"`; `updateRequired` if `"outdated"`; else `missing`.
- firstLaunchSetup "hooks ready" gate (real fn `areFirstLaunchAgentHooksReady`): ANY of
  codex/claude/opencode/pi is `installed`/`notRequired`.
- skills gate (`areFirstLaunchBundledSkillsInstalled`): ALL 8 skills installed:
  browser, embeddedBrowser, computerUse, agentOrchestration, fable56Orchestration,
  findPrevSession, generateTitle, manageBeads, moveCodexSession.
- missing-hooks tips notice: agents with `cliInstalled && status ∉ {installed,
notRequired, cliMissing}`; `updateRequired` labeled "outdated", else "missing".
- cli notice: `!ghostexCli.installed || !ghostexCli.gxUsable`.

Real message types: import from the shared contract (e.g.
`SidebarAgentHookStatusMessage`, `SidebarGhostexCliStatusMessage` — see
`packages/shared/session-grid-contract-sidebar.ts` and how
`packages/core-ui/first-launch-setup-modal.stories.tsx:13-35` builds fixtures from
`DEFAULT_SIDEBAR_AGENTS`). Never hand-roll shapes the real components validate.

Add-project ops: answer `addProjectDialogRequest` (and the repository
clone/browse/worktree request messages if they arrive) using
`packages/core-ui/add-project-modal/add-project-modal-mocks.ts`
(`createAddProjectStoryMocks`) semantics; reply envelope
`{type:"addProjectDialogResult", requestId, ok, result|error}` etc. On success,
increment `env.projectCount`.

Modal chrome table: title + size per `SandboxModalKind` mirroring
`GpuiAppModalKind::window_title/window_size` (`apps/desktop/src/app/model/app_modal_kind.rs`):
`firstLaunchSetup`/`discoverGhostex` = 1120×850, title "Ghostex Tips" for
firstLaunchSetup; fit-height modals (see one-shot table in modal-host.tsx:152) get
`height:"fit"`. Reasonable defaults for the rest; don't over-research.

Persistence: fake state file + env + launchCount in `localStorage`
(`ghostex.onboardingSandbox.*`). `quitApp()` closes everything, keeps persisted state.
`relaunchApp()` = quit + launch (new launch increments `launchCount`, resets
in-memory-only suppressions — e.g. portless suppressed-until-restart). "Wipe state
file" = brand-new user.

## Desktop UI (bead .9)

- Fake macOS desktop filling the stage area: wallpaper (CSS gradient fine), menu bar
  (Apple logo, "Ghostex" when running, clock), dock with a few decoy app icons + the
  real Ghostex icon (look under `apps/desktop/assets/` / `media/` for an icon asset; a styled
  fallback glyph is acceptable). Click Ghostex → `launchApp()`. Running indicator dot.
  While `launching`, show subtle bounce.
- Fake Ghostex main window (only when running): titlebar with traffic lights (red =
  `quitApp()`), title, and the ⓘ Tips button with unread badge (`tipsBadgeCount`).
  Body: left sidebar pane showing the real empty-state copy ("No Projects Added…" when
  `projectCount === 0`, "Unable to load sessions / Load Sessions" when gxserver
  unhealthy — real copy in `packages/core-ui/sidebar-app.tsx:3559-3588`), main area placeholder
  terminal. Visual re-mock only — do NOT mount the real SidebarApp.
- Toast stack (top-right of the fake window) rendering `store.toasts` mirroring GPUI
  app toasts (title, message, auto-dismiss).
- Tips panel: anchored dropdown under the ⓘ button when `tipsPanelOpen`. Preferred:
  mount the REAL React `TitlebarTipsMenu` from `apps/desktop/views/titlebar-host.tsx`
  following the mocking pattern of `packages/core-ui/titlebar-reading-panels.stories.tsx`
  (it installs `__ghostex_TITLEBAR_PANEL_KIND__` + `__ghostex_NATIVE_HOST__`). If that
  drags in unmountable module-scope side effects, fall back to a visual re-mock fed by
  `store.tipsNotices` (12 tips + notices list). State whichever you chose in your
  report.
- Render `store.modalWindows` via `ModalWindowFrame` (from bridge), centered above the
  fake window.
- All styling: plain CSS in `src/desktop/*.css` (imported by your components). Do NOT
  introduce new Tailwind classes in sandbox files (the generated CSS doesn't scan this
  directory). Aim for a convincing, clean macOS look (SF-ish system font stack,
  translucency, shadows).

## Control panel (bead .10)

Right-side inspector drawer (collapsible), sections:

1. **Scenario presets** — one click applies env + state file + optional auto-relaunch:
   "Brand-new user", "New user, gxserver needs upgrade" (buildMismatch +
   respawnFixesHealth), "Returning user, nothing installed", "Power user (all
   installed)", "Hooks outdated", "CLI shadowed (gx blocked)". Applying a preset while
   the app runs offers/executes relaunch.
2. **Environment** — every `SimEnvState` field: per-agent CLI + hook state (priority
   agents codex/claude/opencode/pi prominent, the rest collapsible), ghostex CLI
   (installed/gxUsable/blocked), 8 skills (+ all on/off), cua-driver (app, cli, two
   permissions), gxserver scenario + respawn-heals toggle, platform (macos/windows),
   project count, update available, timing sliders.
3. **Persisted state file** — live JSON view of the fake
   `gpui-first-run-onboarding-state.json` with per-field toggles/inputs + "Wipe (fresh
   user)" button. Highlight fields that changed during the current run.
4. **App lifecycle** — Launch / Quit / Relaunch buttons + phase + launch counter.
5. **Modal gallery** — force-open button per `SandboxModalKind` (grouped: onboarding,
   project, settings, misc), calling `forceOpenModal(kind)`; include the payload
   presets needed for interesting ones (e.g. portlessSetup mode/protocol variants).
6. **Event log** — chronological `store.events`: time since launch, colored kind chip,
   label, expandable detail, `codeRef` rendered as a dim mono suffix. Filter by kind;
   auto-scroll; clear button; a "restart-required" warning row style for the dropped-
   onboarding events.
   Plain CSS (`src/controls/*.css`), compact developer-tool aesthetic.

## Shared repo rules for ALL agents

- Never run `bun run start` or anything launching the real app. Running
  `bunx vite --config apps/desktop/test/onboarding-sandbox/vite.config.ts` IS allowed.
- `bunx tsc -p apps/desktop/test/onboarding-sandbox/tsconfig.json --noEmit` must pass for your
  files (pre-existing errors from imported production code are not yours to fix —
  report them instead).
- No tests. No git branch switching, no commits, no destructive git commands.
- Other agents work in this checkout concurrently: touch ONLY your owned files (plus
  additive edits to `types.ts` if truly needed). Never revert or "clean up" foreign
  diffs. Use targeted Edits for any shared file.
- When done: `bd update <your-bead> --status review` and
  `bd comment <your-bead> "<user-facing summary, no file lists>"` (use `--force` if bd
  complains about prefix mismatch).
- Production components must run UNCHANGED. If a real component genuinely cannot work
  without a production-file change, stop and report it in your final message instead
  of patching production code.

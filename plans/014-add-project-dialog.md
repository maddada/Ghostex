# 014 — Add-Project Dialog (gpui + web + mobile, gxserver shared logic)

Date: 2026-07-30. Status: in progress (multi-agent implementation).

Companion doc: `plans/014-add-project-dialog.surfaces.md` — where everything plugs into Ghostex (modal systems, Storybook harness, web runtime, mobile structure, contract checklist, repo rules).

## Goal

Replace Ghostex's bad add-project UX (native OS folder picker locally; flaky `RemoteProjectPickerModal` remotely) with a consistent add-project command-palette flow, working for **local and remote machines**, on **gpui desktop**, **ghostex-web**, and **mobile (RN)**, with **gxserver owning all shared logic**.

Also fixes the reported remote bug (root-caused 2026-07-30):
1. Remote `/api/addProjectPath` right after reconnect can take ~19s vs the 20s timeout (`gpui/src/main.rs:30639-30647`, JS waiter `native/sidebar/modal-host.tsx:675-678`); failure feedback is a tiny transient line; **and on error Rust never refreshes remote presentation** (`main.rs:30650-30677`) so adds that land after the timeout never appear (the machine's presentation stream is often dead: `presentationStreamFailed`).
2. Enter-key trap: cmdk auto-highlights the first suggestion so plain Enter *navigates* instead of *adding* (`sidebar/remote-project-picker/remote-project-picker-modal.tsx:199-208`). Using `autoHighlight={false}` in path modes—Enter submits the typed path, mod+Enter overrides a highlight—is the fix.

## Architecture decisions (settled — do not relitigate)

1. **The dialog is one shared React component** at `sidebar/add-project-modal/` (new directory), styled as a centered top-anchored command dialog. Reuse existing remote helpers in `sidebar/remote-project-picker/` rather than duplicating.
2. **gpui hosts it in the app-modal native child-window system** (pattern A: `openAppModal` → `modal-host.html`) — a 576px dialog cannot fit inside the 220–420px sidebar document, and the user asked for "React in a gpui window". New `GpuiAppModalKind::AddProject` with payload `{ machineId?: string }` (preselects a machine; omitted → machine step decides).
3. **ghostex-web hosts the same component in-page** via the app-modal shim → CustomEvent → host component pattern (`ghostex-web/src/app/recent-projects-modal-host.tsx` is the model).
4. **The component is callback-driven** (no transport inside): props supply async ops `{ listMachineOptions, browse, addProject, discoverSourceControl, lookupRepository, startClone, readCloneJob, cancelCloneJob }` plus presentation props. gpui's modal host implements callbacks with requestId request/response bridge messages (Rust routes local vs remote by machineId — CEF never sees hosts/tokens); web implements them with `rpcForMachine`; Storybook mocks them. Adopt the worktree-popover idioms (mint requestId per call, nothing optimistic, dismiss abandons answers).
5. **Machines**: machine step shown only when >1 option (local + connected remotes); a remote entry point preselects its machine and skips the step.
6. **gxserver owns all logic**: browse, add (with create-if-missing), source-control discovery/lookup, clone. Mobile reaches the same logic through new `ghostex` CLI verbs (mobile speaks SSH+CLI, not the wire protocol).
7. **Scope of clone sources**: Local folder + Git URL fully working everywhere. Provider sources (GitHub via `gh`, GitLab via `glab`) implemented in discovery+lookup; Bitbucket / Azure DevOps rows render with "not ready / Setup Required" treatment (discovery reports them unavailable) — no server support for them in this pass.
8. Mobile is a **multi-screen native-stack flow**, not a port of the palette: Source → Repository → Destination / Local-browse screens, Ghostex mobile conventions (`plans/014-add-project-dialog.surfaces.md` §5).
9. Old surfaces: entry points are rewired to the new dialog; `RemoteProjectPickerModal` stays only where embedded for clone-destination in `add-repository-modal.tsx` (untouched).

## Part A — gxserver + contracts + CLI (foundation)

Files: `gxserver-rs/src/*`, `shared/gxserver-protocol.ts`, `native/sidebar/gxserver-client.ts`, `gxserver-rs/src/ghostex_cli/*`.

1. `/api/addProjectPath`: add optional `createIfMissing?: boolean` — expand `~`, resolve, `mkdir -p` when missing and requested; keep idempotency; use consistent error messages ("Workspace root is not a directory: …" etc.). Existing behavior unchanged when the flag is absent.
2. `/api/browseProjectDirectories`: verify parity with spec §6.1 (dirs only, prefix case-insensitive, hidden rules, `parentPath` = server-resolved absolute, EACCES→empty, `~` and bare `~` handling). Fix gaps only if found.
3. New `/api/discoverSourceControl`: probe `gh`/`glab` (availability, version, auth status via `gh auth status` / `glab auth status`, 5s timeout per probe); return readiness data including `installHint`; bitbucket/azure-devops reported unavailable with a hint. New `/api/lookupRepository`: `{provider, repository}` → `{provider, nameWithOwner, url, sshUrl}` (`gh repo view --json …`, glab equivalent).
4. Clone: reuse the existing job endpoints (`/api/previewRepositoryClone`, `/api/startRepositoryClone`, `/api/readRepositoryCloneJob`, `/api/cancelRepositoryCloneJob`, `gxserver-rs/src/server.rs:2464-2469`). Verify a plain `remoteUrl` + `destinationPath` clone-then-register works with the intended destination semantics (exists&non-empty → error; create parent dirs; register project at cwd on success). Extend only if a gap blocks the dialog.
5. `protocol.rs`: remote-allowlist all endpoints the dialog needs from remote machines.
6. CLI verbs (for mobile): `browse-directories <partialPath>`, `discover-source-control`, `lookup-repository <provider> <repo>`, `clone-repository <remoteUrl> <destinationPath>` (blocking wrapper over the clone job), and `add-project --create-if-missing`. JSON output. Register in `ghostex_cli/mod.rs`, route in `actions.rs`, document in `usage.rs`.
7. `shared/gxserver-protocol.ts`: endpoint paths + Params/Result types; `native/sidebar/gxserver-client.ts`: methods + dispatch entries.
8. Tests in gxserver-rs are allowed and expected for new server logic (NOT in gpui/).

## Part B — shared React dialog + Storybook (foundation, parallel with A)

Files: new `sidebar/add-project-modal/**`, story `sidebar/add-project-modal/add-project-modal.stories.tsx` (+ interactions).

- Implement the flow: machine step (>1 machines) → sources step → local browse / repo input → clone destination. Reproduce the keyboard model (no auto-highlight in path modes; Enter submits typed path; mod+Enter with highlight; Backspace-on-empty pops; clearing initialQuery pops), submit-path resolution, labels, empty-state hints, placeholders, and visual design adapted to Ghostex's existing dialog/command primitives.
- Callback props per architecture decision 4; every interactive element gets `data-add-project-field="…"` + aria-labels.
- Clone uses the job API: start → poll `readCloneJob` until done/failed with a simple "Cloning…" state → `addProject(cwd)`.
- Reliability UX from the root cause: pending states must not be lossy — errors render as a persistent inline error region (not a transient list line); add timeout handled by the HOST (60s), and the dialog shows "still working…" affordance rather than silently dying.
- Storybook: standalone stories with mocked callbacks (fixture directory tree, scripted latencies/failures) + play-function interaction stories covering: browse/descend/up, Enter-submits-typed-path, mod+Enter with highlight, Create & Add label flip, machine step with local+remote, source readiness (ready gh / not-ready bitbucket with Setup Required treatment), url→destination→clone→add happy path, lookup failure stays on step, add failure shows persistent error. Follow harness conventions in surfaces doc §3 (storyRoot = document.body, `findRequiredElement`, `step()`).
- Typecheck: `bun run typecheck` (or the sidebar tsconfig path used by CI) and `bun run storybook` must build.

## Part C — gpui integration (after A+B)

Files: `gpui/src/main.rs`, `gpui/src/cef/*` (only if bridge lists need the new kind), `native/sidebar/modal-host.tsx`, `sidebar/app-modal-host-bridge.ts`, `gpui/sidebar/gxserver-runtime.ts`, `sidebar/sidebar-app.tsx`, `shared/session-grid-contract-sidebar.ts` (new message variants).

1. New app-modal kind `addProject` — the 5 coordinated edits (surfaces doc §1.2): bridge kind + payload, modal-host state/open/render/fit-height, `GpuiAppModalKind` variant (`from_modal_id`/`modal_id`/`window_title` "Ghostex Add Project"/`window_size` ~640×520/`open_message`).
2. Callback plumbing: requestId request/response messages for listMachineOptions, browse, add(createIfMissing), discover, lookup, startClone/readCloneJob/cancelCloneJob, routed by machineId — local → local gxserver, remote → `gpui_remote_gxserver_request_target` tunnel (follow the existing `browseRemoteProjectDirectories`/`addRemoteProjectPath` round-trips, `main.rs:30515`/`:30591`). **Add timeout 60s** (Rust and JS waiter). Log breadcrumbs bounded-ids-only.
3. Root-cause fixes: refresh remote presentation after add/clone **regardless of Ok/Err** (`main.rs:30650-30677`); error strings surfaced verbatim to the dialog.
4. Entry points: V1 local Projects header button (`sidebar/sidebar-app.tsx:5294` area) and remote machine header (`:5556-5563`) → `openAppModal({modal:"addProject", machineId?})`; V2 create-button menu gains "Add project…"; command palette `addProject` command routed to the new modal.
5. Constraints: NO tests in gpui; `cargo check` in gpui/ and the vite CEF build must pass; do NOT run `bun run start`; do not touch the deprecated Swift app.

## Part D — ghostex-web (after A+B)

Files: `ghostex-web/src/app/app-modal-host-shim.ts`, `action-events.ts`, new `add-project-modal-host.tsx`, `__root.tsx` mount, `ghostex-web/src/sidebar-runtime/sidebar-runtime.ts` if sidebar messages are involved.

- Shim supports `addProject`; host component renders the shared dialog in-page with callbacks over `rpcForMachine(machineId, …)` (browse/add/discover/lookup/clone endpoints from Part A). Machine options from the connection registry.
- Web entry points: same sidebar buttons now work on web (they post openAppModal through the shim). Verify end-to-end against the local gxserver (web:dev proxy), including at least one real add + cleanup (`/api/removeProject` the scratch project afterwards; use a scratch dir under /tmp).
- `bun run web:typecheck` and `bun run web:build` must pass.

## Part E — mobile (after A)

Files: `mobile/src/**` (new `features`-style screens or SessionsScreen overlay replacement), `mobile/src/commands/ghostexCli.ts` (new verb builders), `mobile/src/navigation/types.ts`, `mobile/App.tsx`, `mobile/src/copy.ts`.

- Multi-screen flow per spec §5 with Ghostex conventions (surfaces doc §5): Source screen (machine already chosen by the entry-point context menu; sections: Local folder, Git URL, providers with readiness), Local-browse screen (path input + Add button + FolderBrowser list; tap replaces input; hidden dirs excluded), Repository screen (Continue/Lookup), Destination screen (repo card + browse + Clone project).
- Transport = new CLI verbs over the existing SSH exec (`runGhostexCli` / `runSessionCommand` patterns). Duplicate handling and inline ErrorBanner semantics per spec §5.8 adapted to Ghostex (no optimistic rows; refresh inventory after add).
- Entry: replace the current bare `PromptDialog` overlay (`SessionsScreen.tsx:2309-2327`) — the "Add Project" context-menu item navigates to the new flow.
- `bunx tsc --noEmit` in mobile/ must pass; verify in iOS simulator (`bunx expo prebuild && bunx expo run:ios`). Read https://docs.expo.dev/versions/v57.0.0/ docs before writing code (binding per mobile/AGENTS.md). Report honestly what simulator verification could and couldn't cover (SSH machine may be unavailable).

## Verification (final, fable agent)

Storybook play stories all pass (drive via browser if no test-runner); web end-to-end add (local + confirm UI parity); iOS simulator flow walkthrough; `cargo check` (gpui + gxserver-rs), gxserver-rs tests, all typechecks/builds above. NO verification in the real gpui app. Fix loop until clean.

## Handoffs

Each implementing agent appends a short handoff section to `plans/014-add-project-dialog.handoffs.md` (create if absent): what changed, new APIs/messages with exact names, anything the next part must know, verification results.

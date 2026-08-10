# Ghostex Surfaces Recon — for the new Add-Project dialog (2026-07-30)

Working repo: /Users/madda/dev/_active/Ghostex. All file:line refs are into this repo.
Companion doc: `plans/014-add-project-dialog.md` (implementation plan).

## 0. Architecture decision

Two dialog systems exist. The "new worktree dialog" (2026-07-29) is the **Sidebar-V2 in-document popover** (pattern B) — its own header comment (`sidebar/v2/sidebar-v2-worktree-popover.tsx:10-24`) explains it deliberately avoids the native child-window modal host so it can render in the sidebar document, be driven in Storybook, and work on web with zero plumbing. **The add-project dialog uses pattern B.** Do NOT add a `GpuiAppModalKind` variant or touch `gpui/src/main.rs` / `native/sidebar/modal-host.tsx`.

## 1. Pattern B — the worktree popover template

- Component: `sidebar/v2/sidebar-v2-worktree-popover.tsx:1-347`. Props at `:65-85` (`agents`, `defaultAgentId`, `errorMessage`, `isPending`, `messageSource`, `onDismiss`, `onRequestWorktrees`, `onSubmit`, `position`, `projectLabel`, `vscode`). Plain HTML controls, every field has `data-worktree-field="…"` for Storybook targeting.
- Mounted conditionally in `SidebarV2Root`: `sidebar/v2/sidebar-v2-root.tsx:1774-1807`.
- Wraps itself in `SidebarContextMenuPortal` (`sidebar/sidebar-context-menu-portal.tsx:243-357`): portal to document.body, dismiss backdrop, viewport clamping, Escape + window-blur dismissal, native menu-opened/closed notifications, `dismissAllSidebarContextMenus()` registry.
- Trigger: `sidebar/v2/sidebar-v2-create-button.tsx` split button; menu item calls `onOpenWorktreePopover(position)`; position = `{ clientX: rect.left, clientY: rect.bottom + 4 }` (`:94-97`).
- Root state: `SidebarV2WorktreePopoverState` `sidebar/v2/sidebar-v2-root.tsx:289-292`, hooks `:394-401`, `openWorktreePopover` `:736-745`; call sites toolbar `:1452-1454`, group header `:1548-1550`.

### Request/response idiom (copy exactly)
1. Sidebar→host commands ONLY via `sidebar/v2/sidebar-v2-messages.ts` (header `:3-12`: contract-first, never V2-only side channels). Examples: `postSidebarV2CreateWorktreeSession` `:163-187`, `postSidebarV2RequestProjectWorktrees` `:208-223`.
2. Every request mints a `requestId`: `createSidebarV2RequestId(prefix)` `sidebar/v2/sidebar-v2-root.tsx:306-308`.
3. Answers arrive as `MessageEvent`s on `messageSource` (defaults to `window`). Popover list listener `sidebar-v2-worktree-popover.tsx:180-208`; root submit listener (single listener + refs) `sidebar-v2-root.tsx:855-925`.
4. **Nothing is optimistic** — created entities arrive as ordinary presentation deltas; `requestId` only drives pending state + inline error (`sidebar-v2-root.tsx:391-400`, `shared/session-grid-contract-sidebar.ts:1170-1176`).
5. Dismiss abandons the answer (clears pending ref) `sidebar-v2-root.tsx:1785-1791`.

### Styling
- CSS in `sidebar/styles/sidebar-v2.css:1160-1340` (worktree block). Portal content mounts OUTSIDE `[data-sidebar-version="v2"]` → popover classes unscoped; in-sidebar trigger classes scoped (`sidebar/styles/sidebar-v2.css:1205-1209` comment).
- Tokens: `--sv2-density`, `--sv2-muted`, `--sv2-foreground`, `--sv2-border`, `--sv2-row-hover`, `--app-button-background/foreground`, `--app-error-foreground`. Worktree popover: 268px wide, 8px gaps, 10px padding, 6px radii, 12px body/10px uppercase labels. Menu chrome classes reused: `session-context-menu*`.
- Icons: `@tabler/icons-react`.

### Capability gating
`SidebarSessionLifecycleCapabilities` `shared/session-grid-contract-sidebar.ts:309-322` (+ per-machine `lifecycleCapabilitiesByMachineId` `:823-836`, threaded at `sidebar/sidebar-app.tsx:5150-5152`). Absent capability → affordance disappears entirely (`sidebar-v2-create-button.tsx:98-116`).

## 2. Existing add-project machinery (today)

- gpui local: sidebar posts `{ type: "pickWorkspaceFolder" }` (`sidebar/sidebar-app.tsx:4745-4748`, contract `shared/session-grid-contract-sidebar.ts:1776-1783`) → native OS folder picker in Rust (`gpui/src/main.rs:29430-29466`) → `workspaceFolderPicked` → `gpui/sidebar/gxserver-runtime.ts:13256-13285` calls `POST /api/addProjectPath`.
- `registerProjectPath()` helper: `gpui/sidebar/gxserver-runtime.ts:12899-12912`.
- A path browser already exists (pattern-A modal host, used for remote add + clone destination): `sidebar/remote-project-picker/remote-project-picker-modal.tsx` (props `:43-56`: `onAddProject(path)`, `onBrowse(input)`), helpers `sidebar/remote-project-picker/remote-project-paths.ts`, `remote-command-palette-logic.ts`, types `remote-filesystem.ts:1-13`. Wired in modal host at `native/sidebar/modal-host.tsx:1275-1304`; also embedded in `sidebar/add-repository-modal.tsx:521-544`.

## 3. Storybook

- Config `sidebar/.storybook/main.ts` (stories glob `../**/*.stories.@(ts|tsx)`, `@` → repo root, dev middlewares serving real local settings/projects read-only). Preview: dark `#050505`, fullscreen.
- Run: `bun run storybook` (port 6006; runs `build:sidebar-css` first — required). Storybook 10.3.5; test utils from `"storybook/test"`.
- Template to copy: `sidebar/v2/sidebar-v2-worktree.interactions.stories.tsx` — philosophy comment `:17-28` ("assert MESSAGES, not moved rows"), meta `:35-48` (`fixture: "sidebar-v2-inbox"`, `sidebarLifecycleCapabilities: "settleSnoozeGitAndWorktree"`, `sidebarV2Layout: "flat"`, `sidebarVersion: "v2"`), `findPostedMessage` `:56-68`, mock-answer via `window.postMessage({ …requestId, type: "…Result" }, "*")` `:70-86`, open helper `:88-98`.
- Harness: `sidebar/sidebar-story-harness.tsx` (`getSidebarStoryMessages()`/`resetSidebarStoryMessages()` `:23-31`; mock host = recording `WebviewApi` + reducer + re-hydrate `:33-101`). No mock gxserver client exists — everything through the message contract.
- Meta plumbing: `sidebar/sidebar-story-meta.tsx` (`DEFAULT_SIDEBAR_STORY_ARGS` `:11-25`, `SIDEBAR_STORY_ARG_TYPES` `:27-124`, decorators `:126-132`, `renderSidebarStory` `:134-140`). Fixtures: `sidebar/sidebar-story-fixtures.ts`, `sidebar/v2/sidebar-v2-story-fixtures.ts` (`sidebar-v2-inbox`, `sidebar-v2-monorepo`, `sidebar-v2-multi-machine`).
- Helpers: `sidebar/sidebar-app.interactions.helpers.ts` (`expectMessage` `:66-72`, `findRequiredElement` `:206-224`), `sidebar/v2/sidebar-v2.story-helpers.ts` (`waitForSidebarV2` `:9-31`).
- Play-function conventions: `const storyRoot = canvasElement.ownerDocument.body` (portals!), `await waitForSidebarV2(storyRoot)`, `resetSidebarStoryMessages()`, wrap phases in `step()`, target `data-*-field` attributes.

## 4. ghostex-web

- Mounts shared sidebar: `ghostex-web/src/sidebar-runtime/WebSidebar.tsx:6-18` (`SidebarApp` with `messageSource={runtime.messageSource}`); shell `ghostex-web/src/routes/__root.tsx:133-135`.
- gxserver client: `ghostex-web/src/connections/gxserver-client.ts` (`rpc<T>(path, params)` `:43-66`); registry `connection-registry.ts` (`rpcForMachine<T>(machineId, path, params)` `:35-45`).
- Sidebar runtime message switch: `ghostex-web/src/sidebar-runtime/sidebar-runtime.ts:377-620`; unsupported → `nativeOnlyNoOp`. **`pickWorkspaceFolder` is a no-op on web → web cannot add projects today.**
- Existing project handlers to imitate: `requestRecentProjects` `:520-558` (answers back through `messageSource.postMessage({ requestId, … })` `:554-558`), `restoreRecentProject` `:571-596`.
- Machine-id helpers: `ghostex-web/src/sidebar-runtime/sidebar-ids.ts` (`parseSidebarGroupId`, `parseSidebarProjectId`, `createSidebarProjectId`).
- Run: `bun run web:dev` (proxies /api → 127.0.0.1:58744), `bun run web:typecheck`, `bun run web:build`.
- Pattern-B popover needs ONLY new cases in sidebar-runtime.ts (browse + add), nothing else.

## 5. Mobile (mobile/, RN/Expo SDK 57)

- **Does not speak gxserver protocol.** Everything over SSH via CLI: `mobile/src/commands/ghostexCli.ts` (`addProjectCommand` `:196-199`, `removeProjectCommand` `:192-194`); transport `mobile/src/inventory/client.ts` (`GhostexNative.exec(machine.id, loginShellCommand(...), 20000)`). Mobile does NOT import shared/; its contract types live in `mobile/src/contract/mobileSummary.ts`.
- CLI is implemented in `gxserver-rs/src/ghostex_cli/mod.rs` (`add-project` registered `:173`, mapped `:318`), `actions.rs:183` (`addProject → POST /api/addProjectPath`), usage `usage.rs:143`. **No browse CLI verb exists — must be added** (register in mod.rs, route in actions.rs to `/api/browseProjectDirectories`, add builder in ghostexCli.ts).
- Existing Add Project UI: `Overlay` union variant `{ kind: 'addProject', machine, error }` `mobile/src/screens/SessionsScreen.tsx:185`; entry = Projects section-label context menu `:1492-1550` (item `:1515-1521`); render = bare `PromptDialog` `:2309-2327`; executor `runSessionCommand` `:546-590`.
- Navigation: `@react-navigation/native-stack`, single root stack `mobile/App.tsx:36`, routes `mobile/src/navigation/types.ts:2-11`. New multi-step flow can be a new stack screen or upgraded overlay.
- UI conventions: tokens `mobile/src/theme/palette.ts` (`GhostexPalette`, `GhostexRadii`); copy centralized in `mobile/src/copy.ts`; dialogs `mobile/src/components/common/PromptDialog.tsx` / `ActionSheet.tsx` / `ConfirmDialog.tsx` / `ProgressOverlay.tsx` / `StateCard.tsx`; anchored menus `mobile/src/components/sessions/ContextMenu.tsx`; svg glyphs `mobile/src/components/sessions/icons.tsx`; zustand stores.
- Run iOS: `cd mobile && bun install && bunx expo prebuild && bunx expo run:ios`; typecheck `bunx tsc --noEmit`. Simulator is arm64-only. `mobile/AGENTS.md` is binding: read https://docs.expo.dev/versions/v57.0.0/ docs before writing code.

## 6. Shared contracts + endpoint checklist

- Contract barrel `shared/session-grid-contract.ts`; sidebar contract `shared/session-grid-contract-sidebar.ts` — Host→sidebar union `ExtensionToSidebarMessage` `:1291-1323`; Sidebar→host union `SidebarToExtensionMessage` `:1325-…`. Exemplar result messages: `SidebarWorktreeSessionResultMessage` `:1178-1187`, `SidebarRecentProjectsResultMessage` `:1203-1207`. Exemplar requests: `requestProjectWorktrees` `:2586-2592`, `createWorktreeSession` `:2611-2620`.
- Protocol `shared/gxserver-protocol.ts`: endpoints incl. `/api/addProjectPath` `:148`, `/api/browseProjectDirectories` `:177`, `/api/browseFilesystem` `:184`, `/api/createProject` `:141`. **Browse contract types already exist**: `GxserverProjectDirectoryBrowseParams` `:646-650` (`cwd?`, `limit?`, `partialPath`), `…Entry` `:652-655` (`fullPath`, `name`), `…Result` `:657-660` (`entries`, `parentPath`).
- Typed client (ACTIVE shared module despite path): `native/sidebar/gxserver-client.ts` — `addProjectPath` `:634-640`, dispatch switches `:1167-1168`, `:1240`, `:1254`.
- Server: `gxserver-rs/src/protocol.rs` remote-allowlist (`/api/addProjectPath` `:412`, `/api/browseProjectDirectories` `:455` remote_allowed); handlers `gxserver-rs/src/server.rs` (`/api/addProjectPath` `:1858`, `/api/browseProjectDirectories` `:2470`); add-project semantics `gxserver-rs/src/domain.rs:621-624`, `:1156`, `:4405`.

New-endpoint checklist: shared/gxserver-protocol.ts (path + Params/Result) → gxserver-rs protocol.rs + server.rs → native/sidebar/gxserver-client.ts → shared/session-grid-contract-sidebar.ts (request + result messages with requestId) → capability flag if gated → poster in sidebar/v2/sidebar-v2-messages.ts → handlers in gpui/sidebar/gxserver-runtime.ts (~:5775-5830) AND ghostex-web/src/sidebar-runtime/sidebar-runtime.ts (+ CLI verb for mobile).

## 7. Repo rules that bite

- No tests in gpui/; no code/tests in deprecated native//src/ (except the gpui-owned files listed in AGENTS.md); NO fallbacks — fix behavior; never run `bun run start` unless the user asked; concurrent agents share this checkout (never clobber foreign uncommitted hunks; targeted edits only); logs = bounded ids only (no hosts/users/paths/tokens); CEF must never receive remote hosts/tokens.

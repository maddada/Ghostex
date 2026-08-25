# Ghostex Extensions — Design Decisions

Status: **implemented on 2026-08-25**, except remote command-server execution
for remote projects (R1), which remains explicitly deferred. This file is the
decisions record for the shipped architecture.

## Vision (from the initial brief)

Ghostex gets a Raycast-style extensions ecosystem:

- New root folder `extensions/` in the repo (worktree symlink), backed by a new
  **public repo `Ghostex-extensions`** (github.com/maddada/Ghostex-extensions).
  One folder per extension, each with its own README rendered like Raycast does.
- Publishing/updating an extension = PR against the extensions repo with labels
  (`new`, `update`, `urgent`).
- An **extensions browser modal** in the app to browse the GitHub catalog
  (rendered README per extension, install from there).
- Extension buttons live top-right under a **puzzle-piece dropdown**, pinnable
  to the bar.
- Authors provide an **SVG icon** (Tabler-style) per extension.
- Extensions are powerful: they can run system commands and control Ghostex via
  the `ghostex`/`gx` CLI.
- Extensions receive context: current active session, the session they were
  started in (same payload as "Copy details"), current project name + path, and
  whether we're inside a worktree.

### Placement types (the 5 surfaces)

| Type | Where it appears |
|---|---|
| **View extension** | A full mode view, exactly like the Code mode hosts code-server today |
| **Chat-bar extension** | An iframe below the chat composer, show/minimize, switchable between multiple installed chat-bar extensions |
| **Terminal-pane extension** | Runs commands in the terminal surface — either as a split to the right of the current terminal or as a new terminal tab (per-extension user setting) |
| **Popup extension** | CEF pane dropdown at top-right, like Chrome browser-action popups |
| **Modal extension** | Large centered modal, extension-defined size up to a cap |

First deliverable after design: **one example extension per type**, built
together.

## Infrastructure done so far (2026-08-25)

- All 12 stale worktrees deleted (8 clean + 4 with unattributable uncommitted
  changes, deletion explicitly approved).
- `tooling/create-extensions-worktree.sh` — creates the light worktree at
  `~/dev/_worktrees/ghostex-extensions` (branch `feat/extensions`, 157 MB vs
  ~150 GB full): normal checkout of app source + vendored ghostty; symlinks
  into the main checkout for the big `.dependencies` submodules (with
  `--skip-worktree` on the gitlinks), `node_modules`, all Cargo `target/`
  dirs, `apps/desktop/{runtime,build,dist}`, `docs/`, and ghostty's zig
  caches; symlink paths ignored via shared `.git/info/exclude` (dir-only
  gitignore patterns don't match symlinks). `extensions/` in the worktree is a
  symlink to the sibling checkout `~/dev/_active/Ghostex-extensions`
  (initialized locally; GitHub repo not created yet).

## Codebase research

Full surface map in [CODEBASE-MAP.md](./CODEBASE-MAP.md) — covers mode
tabs/views, quick actions, CEF surface machinery, the modal system, terminal
tab/split creation, the chat composer, existing plugin/skill/component-store
precedents, icon handling, and 16 friction points for a top-level
`extensions/` folder. Highlights:

- There is **existing prior art**: `artifacts/005-plugin-architecture-options/plugin-options.html`
  already frames Code/Kanban/Automate/Docs as "plugins"; the Settings →
  Plugins tab + `component_store.rs` (sealed manifest, sha256, install/
  uninstall/reinstall, per-plugin visibility switches) is the closest existing
  install pipeline; `skills/` is the closest structural precedent for a
  catalog folder.
- Every placement type has a near-exact existing mechanism to generalize:
  Code-mode CEF slot (view), Tips/Resources anchored CEF dropdown panels
  (popup), `GpuiAppModalKind` CEF child windows (modal),
  `AgentsWorkspaceNewTerminalPlacement { Tab, SplitRight, … }` +
  `open_gpui_command_action_terminal` (terminal-pane), and the session-chat
  composer grid in `session-chat-view.tsx` (chat-bar — currently has no
  below-composer region; the note panel is the pattern to copy).
- Main costs: several closed Rust enums/TS unions need id-carrying
  extension variants (~40 match sites), the CEF bridge allowlist is keyed by
  literal entry filename, the vite CEF entry map is hard-coded, and
  `extensions/**` must be added to tsconfig/vitest/format scopes + AGENTS.md.
- "Copy details" payload (context extensions receive) is
  `buildSidebarSessionDetailsClipboardText` in
  `packages/shared/session-details-copy.ts` — Title, Session ID, Kind,
  Status, Activity, Agent, Agent Session ID, Persistence, Remote Machine,
  Project, Project Path, Worktree, Worktree Branch, Parent Project, Last
  Active.

## Raycast reference — findings worth borrowing

- `package.json` **is** the manifest (`$schema` for validation); folder name =
  extension id; store metadata + commands + preferences + deps in one file.
- Commands have `mode: view | no-view | menu-bar` (+ `interval` for background
  runs) — maps well to our 5 placement types.
- Declarative **preferences schema** (`textfield | password | checkbox |
  dropdown | appPicker | file | directory`, `required` gates first run) instead
  of per-extension settings UI.
- Submission: fork + PR to the monorepo; CI validates manifest/assets/build;
  bot labels (`new extension`, `extension fix`, `OP is author`); **merge to
  main = publish**; `CHANGELOG.md` uses literal `{PR_MERGE_DATE}` substituted
  at merge.
- Store page renders: icon, description, README, command list, `metadata/`
  screenshots, changelog, author/contributors.
- Trust model: extensions are **not sandboxed** (full user permissions); the
  security story is mandatory open source + human PR review, plus per-extension
  isolation and encrypted per-extension storage.

## Decisions

_(one entry per settled question — updated as we go)_

### Implementation status

| Decision | Status |
|---|---|
| 1. Runtime model | Implemented: static and command servers share the local HTTP URL contract. |
| R1. Remote support | Deferred: the contracts carry remote context, but command servers do not yet execute on remote project machines. |
| R2. Audit and supply-chain integrity | Implemented in the extensions-repo validator, reproducible-dist checks, permission-bump check, and publishing workflow. |
| 2. Manifest and layout | Implemented with `ghostex-extension.json`, its JSON Schema, and the fixed extension folder layout. |
| 3. Install and update | Implemented with a generated catalog and SHA-256-verified extension archives. |
| 4. Context and capability API | Implemented with the typed page bridge plus command-server environment and scoped API token. |
| 5. Permissions | Implemented with manifest declarations, install consent, and host-side bridge enforcement. |
| 6. Settings | Implemented with the gxserver-owned extension store and generated preference forms. |
| 7. Extensions modal | Implemented with Store and Installed tabs. |
| 8. Toolbar UX | Implemented with the puzzle dropdown, pinned runtime SVG icons, and live badges. |
| 9. Example extensions | Implemented with Storybook, Session Scratchpad, Lazygit, btop, Claude Usage, and Git. |
| 10. Placement flexibility | Implemented for view, chat-bar, popup, modal, and terminal-pane placements. |

### 1. Runtime model — uniform URL contract, two server forms ✅ (2026-08-25)

**Every extension UI is "a thing served over HTTP at a local URL"** — one
launcher, one lifecycle, one CEF path, one bridge (injected by origin), one
manifest shape. No `file://` static kind. The manifest `server` block has two
forms, one line apart:

```json
"server": { "static": "dist/" }
"server": { "command": "node server.js", "readiness": { "httpGet": "/" } }
```

- **`static`**: Ghostex's own built-in local file server serves the folder —
  no extension process ever runs. For simple extensions (popup color picker,
  chat-bar scratchpad). Instant open, nothing to babysit, easiest to audit.
- **`command`**: Ghostex spawns the process, polls readiness, loads its URL,
  owns lifecycle (kill on quit, restart policy, orphan cleanup). Generalizes
  today's bespoke code-server launcher + component store; **code-server
  becomes the flagship view extension** using this form. Big binaries install
  on demand via a manifest `install` block (per-platform URL + sha256).

An author who outgrows `static` switches that one manifest line to `command`;
everything else (placement, bridge, install/update, audit rules) is
unchanged. Placement (view/chat-bar/popup/modal/terminal-pane) is orthogonal
to the server form; terminal-pane extensions remain manifest-only command
specs run in a real terminal (no web surface). Rejected: `file://` static
loading (breaks uniformity), literal server-only (process bloat + popup
latency + boilerplate servers to audit), React-compiled-by-Ghostex (bundler
in app, core-ui API coupling).

### R1. Remote support — DEFERRED (2026-08-25)

Server-type extensions must eventually run on the remote machine for remote
projects (code-server requires this). Tailscale provides reachability, and
extensions can be given SSH access to run commands on the remote machine.
Every runtime-model decision must stay compatible with this; the actual
implementation is deferred to the end of the project.

### R2. Audit & supply-chain integrity — REQUIRED ✅ (2026-08-25)

All extension code is audited by the user + AI reviews at PR time. The repo
must have rules preventing approved extensions from depending on external
code that can change behavior after approval. Draft rules (to be finalized in
the security question):

1. **Self-contained bundles only** — no CDN `<script src>`, no runtime remote
   `import()`, no `eval` of fetched content. Enforced mechanically: extension
   CEF surfaces' request handler blocks script/wasm loads from outside the
   extension's own folder. Fetching *data* from APIs is allowed; fetching
   *code* is not.
2. **Reproducible dist verified by CI** — PR contains `src/` + `dist/` +
   lockfile; CI rebuilds from pinned deps and fails unless the rebuilt output
   matches committed `dist/` byte-for-byte.
3. **Pinned deps, no install scripts** — lockfile required, CI installs with
   `--ignore-scripts`, registry packages at exact versions only (no git/URL
   deps).
4. **Server-type binaries pinned by hash** — on-demand downloads declared in
   the manifest with per-platform URLs + SHA-256 (component-store precedent);
   changing a binary means a new PR.
5. **No runtime code downloads, ever** — new behavior ships as a new version
   through a reviewed PR.

### 2. Manifest & layout — `ghostex-extension.json` + fixed folder ✅ (2026-08-25)

Dedicated manifest (no npm coupling) with a published **`$schema`**
(`ghostex-extension.schema.json`) for editor validation. **Folder name =
extension id.** Layout per extension in the Ghostex-extensions repo:

```
extensions/<id>/
├── ghostex-extension.json   # the manifest
├── icon.svg                 # Tabler-style SVG, author-provided
├── README.md                # rendered on the store page (browser modal)
├── CHANGELOG.md             # entries use {PR_MERGE_DATE}, substituted at merge
├── metadata/                # store screenshots
├── dist/                    # built, self-contained bundle (what runs)
└── src/                     # source, audited in PR review
```

Core manifest fields (details refined in later decisions): `$schema`, `name`
(= folder), `title`, `description`, `placements` + `defaultPlacement`
(subset of `view | chat-bar | popup | modal`, or the standalone
`terminal-pane` kind — see Decision 10), `author`, `categories`,
`icon`, `server` (`{static}` or `{command, readiness, install}`), placement
config (e.g. `modal: {width, height}` capped by host), `preferences`
(Raycast-style declarative schema), `permissions` (e.g. `exec`, `cli`,
`ssh`). Rejected: `package.json`-with-`ghostex`-field and full Raycast
`package.json` clone (npm semantics don't fit non-JS / terminal-pane
extensions, and Raycast's commands+mode model doesn't map to our 5 placement
types).

### 3. Install/update — CI-published catalog + hash-verified zips ✅ (2026-08-25)

**Merge to main = publish** (Raycast model). On each merge, the
Ghostex-extensions CI: validates every manifest against the `$schema`,
substitutes `{PR_MERGE_DATE}` in changelogs, regenerates **`catalog.json`**
(all manifests + store metadata), builds per-extension **`<id>-<version>.zip`
+ sha256**, and publishes everything to a rolling "store" GitHub Release.

App side: the browser modal fetches `catalog.json` in one request; install
downloads the zip, verifies sha256, unpacks into the local extensions dir
(`~/.ghostex/extensions/<id>/` — exact path to be confirmed with the paths
crate); update re-fetches the catalog and offers per-extension updates. This
mirrors the existing `component_store.rs` pipeline (sealed manifest, sha256,
version pruning). Rejected: local git clone (clone grows forever with `dist/`
+ screenshots; all-or-nothing updates) and GitHub-API-on-demand browsing
(rate limits, N requests per modal open, no hash-verified artifact).

### 4. Context & capability API — JS bridge SDK for pages, env + token for processes ✅ (2026-08-25)

**Pages**: a `window.ghostex` bridge injected **by origin** into extension CEF
surfaces (same CEF message-channel mechanism `chat.html` uses today), wrapped
by a typed SDK published as one vendorable file. Surface (to be refined at
implementation time):

- `ghostex.context()` → `{ activeSession, startSession (the Copy-details
  payload from packages/shared/session-details-copy.ts), project: {name,
  path}, worktree: {isWorktree, branch} }` + `ghostex.onContextChange(cb)`
- `ghostex.cli(verb, args)` → run `ghostex`/`gx` CLI verbs
- `ghostex.exec(cmd, opts)` → system commands with streaming output
- `ghostex.settings.get()` → the extension's preference values (+ change events)
- `ghostex.ui.*` → minimize/close/resize-within-caps, toasts

**Command-server processes**: context via env vars (`GHOSTEX_SESSION_ID`,
`GHOSTEX_PROJECT_PATH`, `GHOSTEX_PROJECT_NAME`, `GHOSTEX_WORKTREE`,
`GHOSTEX_WORKTREE_BRANCH`, …) plus `GHOSTEX_API_URL` +
`GHOSTEX_API_TOKEN` (scoped per extension) for the gxserver HTTP API; the
`gx` CLI works from the process too. Any language gets full power without the
JS SDK. Manifest `permissions` are enforced host-side per call for both
consumers. Rejected: gx-CLI-only (no events/streaming, string parsing
everywhere) and HTTP-API-only (token/CORS/WS boilerplate in every simple
page).

### 5. Permissions — declared + install consent + host-enforced ✅ (2026-08-25)

Manifest `permissions` (e.g. `exec`, `cli`, `ssh`, `network`, `clipboard`,
plus the implicit "runs a background process" for `server.command`). The
install dialog in the browser modal lists them; the host **rejects and logs**
any bridge call not covered by a declared permission; CI fails an update PR
that adds permissions without a version bump, so updates re-surface consent.
Honest documented limit: a spawned server process is inherently unsandboxed —
enforcement is real for bridge calls, advisory for processes; PR review + AI
audit (R2) remains the primary gate. Submission workflow (from the vision):
fork + PR against Ghostex-extensions with labels **`new` / `update` /
`urgent`**; CI validation per Decision 3; merge = publish. Rejected:
review-only trust (no enforceable user consent) and browser-style runtime
prompts (prompt fatigue; automation extensions become nagware).

### 6. Settings — gxserver-owned store + auto-generated preferences UI ✅ (2026-08-25)

Per-extension state lives in a **gxserver-owned extensions store** keyed by
extension id (like project metadata / stashed prompts today): `enabled`,
`pinned`, placement options (e.g. terminal-pane `terminalPlacement:
"splitRight" | "tab"`), `preferences` values, installed `version`,
`grantedPermissions`. No per-extension keys enter the strict `ghostexSettings`
schema; values can sync to web/mobile surfaces later for free. The per-extension preferences form is
auto-generated from the manifest `preferences` schema (Raycast types:
`textfield`, `password`, `checkbox`, `dropdown`, `file`, `directory`;
`required` preferences gate first run) and lives in the **Installed tab of
the Extensions modal** (see Decision 7), not in the Settings modal. Extensions may keep private internal state in
their own origin's localStorage. Rejected: namespaced flat keys in
`native-sidebar-settings.json` (strict normalizer churn, desktop-only) and
extension-private storage only (no declarative form, no first-run gate, no
central reset).

### 7. Extensions modal — one modal, Store + Installed tabs ✅ (2026-08-25)

A new large app modal (new `GpuiAppModalKind`, sized like Discover Ghostex)
with two tabs:

- **Store**: browses the CI-published `catalog.json` — search, category and
  type filters; detail view per extension with rendered README, `metadata/`
  screenshots, permissions list, version + changelog, Install/Update button
  (install consent dialog per Decision 5).
- **Installed**: manage installed extensions in one place — enable/disable,
  pin/unpin, the auto-generated preferences form (Decision 6), version +
  update, uninstall.

**Installed-tab look (user-specified 2026-08-25): copy Chrome's
`chrome://extensions` list** — a 2-column grid of cards, each card: extension
icon top-left, title + short description beside it, a `Details` and a
`Remove` outline button bottom-left, and an enable/disable toggle
bottom-right. Style it to match the **Automate view's look and feel** (the
Project Board Codex redesign language: `#0e0e0e` surfaces, 32px controls,
shared `Switch`/`SegmentedControl` primitives — see
`apps/desktop/views/project-board/styles.ts` and the shared components rules
in AGENTS.md).

**Settings → Plugins stays as-is** for the legacy components (code-server /
kanban / CEF) until code-server migrates to being an extension. Rejected:
store modal + management in a Settings Extensions tab (splits extension UX
across two surfaces), and Raycast-style permanent list+detail two-pane
(user prefers the tabbed store).

### 8. Toolbar UX — one puzzle dropdown, per-type launch, pinnable icons ✅ (2026-08-25)

A **puzzle-piece button** joins the top-right titlebar cluster (new
`GpuiTitlebarPopupKind`, same native popup-window pattern as
Actions/Git/OpenTargets, own `…TitlebarButtonHidden` key). Its dropdown lists
**every installed + enabled extension** (badged/grouped by type) plus a
"Browse extensions…" row opening the modal's Store tab. Clicking an entry
performs the type's natural launch:

- **view** → switch to that extension's view (mode-tab behavior)
- **chat-bar** → open/focus its panel under the active session's chat composer
- **terminal-pane** → run its command per the user's split-right/new-tab setting
- **popup** → open its anchored CEF panel dropping down under the button
  (Chrome-style; Tips/Resources panel machinery)
- **modal** → open its centered modal

**Pinning** (toggle on the dropdown row or in the Installed tab) places the
extension's SVG icon directly on the titlebar next to the puzzle button; a
pinned extension launches from its own icon — a pinned popup opens under its
own icon exactly like a pinned Chrome extension. Rejected: popups-only puzzle
dropdown (no single launch point) and merging into the existing Actions
dropdown (mixes audited store extensions with personal commands; vision
explicitly asked for the puzzle button).

### 9. Example extensions (one+ per type) — ✅ Implemented (2026-08-25)

- **View → Storybook**: `server.command` extension running the project's
  Storybook (`bun storybook --port {port}`, cwd `{projectPath}`, httpGet
  readiness) shown as a new mode tab — deliberately the little sibling of the
  code-server pattern.
- **Chat-bar → Session Scratchpad**: static markdown scratchpad keyed to the
  active session (content swaps on `onContextChange`), persisted in the
  gxserver extension store. Freeform user notes — complements, not overlaps,
  the agent-notes surface.
- **Terminal-pane → Lazygit AND System Monitor (btop)**: two examples.
  Lazygit runs in `{projectPath}` (worktree-aware, `requires: ["lazygit"]`
  missing-dependency UX); btop needs no project context. Both honor the
  per-extension split-right/new-tab setting.
- **Popup → Claude Usage** (user-specified, modeled on
  github.com/robinebers/openusage): a `server.command` popup whose background
  process polls Claude usage and **live-updates its pinned titlebar icon**
  (Claude logo + stacked percentages, e.g. weekly 95% / model 86%). The
  dropdown panel replicates the OpenUsage layout: provider header ("Claude
  Max 20x"), Session bar (% used + resets-in), Weekly bar (+ limit warning),
  per-model bar (e.g. Fable, ~spare %), usage-trend sparkline, Extra Usage,
  Today / Yesterday / Last 30 Days cost + token totals, Status and Dashboard
  link buttons. **New platform requirement this surfaces: a dynamic-icon /
  badge API** (`ghostex.ui.setIcon`/`setBadge`-style, Chrome badge analog)
  callable from an extension's background process so pinned icons can render
  live data while the popup is closed.

  **OpenUsage port research (done 2026-08-25; repo is MIT, native Swift/SwiftUI
  menu-bar app; clone at `/tmp/openusage-research`):**
  - **Live limit bars come from Anthropic's OAuth API, not local math**:
    `GET https://api.anthropic.com/api/oauth/usage` with `Authorization:
    Bearer <token>`, headers `anthropic-beta: oauth-2025-04-20` + a
    `claude-code/x.y.z` User-Agent. Response supplies `five_hour.utilization`
    → Session, `seven_day.utilization` → Weekly, and per-model bars from the
    `limits[]` array (`kind: "weekly_scoped"`, match
    `scope.model.display_name`, `percent` 0–100), each with `resets_at`;
    `extra_usage.used_credits/monthly_limit` (cents) → Extra Usage; plan
    label from credentials' `subscriptionType` + `rateLimitTier`; 429s
    honored via `retry-after`.
  - **Credentials reuse Claude Code's login**, probed in order: macOS
    Keychain service `"Claude Code-credentials"` (+ legacy/`-<hash>`
    variants) → `~/.claude/.credentials.json` (`claudeAiOauth`) → Claude
    Desktop's `"Claude Safe Storage"` keychain → `CLAUDE_CODE_OAUTH_TOKEN`
    env (403s on usage). Token refresh: `POST
    https://platform.claude.com/v1/oauth/token` with Claude Code's client id.
  - **Cost tiles (Today/Yesterday/30 Days) are a local ccusage-style JSONL
    scan, no API**: scan `<claude config>/projects/**/*.jsonl`, keep lines
    with `"usage":{`, dedup by `(message.id, requestId)`, cost = line's
    `costUSD` else tokens × LiteLLM/models.dev rates; incremental cache by
    path+size+mtime.
  - Key files for the port: `Providers/Claude/ClaudeAuthStore.swift`
    (credential probing), `ClaudeUsageClient.swift` (endpoint/headers),
    `ClaudeUsageMapper.swift` (response schema incl. `weekly_scoped`),
    `ClaudeLogUsageScanner.swift` (dedup rules); provider pattern documented
    in `docs/adding-a-provider.md` (AuthStore / UsageClient / Mapper /
    Provider per folder — a shape our extension can mirror in JS, and a
    future path to more provider-usage extensions).
- **Modal → Git extension** (user-specified): rebuild Ghostex's Git surface
  as the first modal extension, **completely separate from the main
  codebase**, replacing the current hard-to-work-on Git UI over time
  (today: `GpuiAppModalKind::GitCommit` + `GitFileDiff` →
  `packages/core-ui/git-commit-modal.tsx` / `git-file-diff-modal.tsx`, the
  titlebar Git dropdown, and ~110 KB of runtime logic in
  `apps/desktop/sidebar/gxserver-runtime/git/`). It is the flagship
  **multi-placement** extension (Decision 10): user can show it as a modal
  **or a full view** (or popup) — so its UI must be fully responsive across
  those sizes. Status/commit/diff/branch operations run through
  `ghostex.exec` git calls (and gxserver git endpoints where useful) against
  `{projectPath}`, worktree-aware.

### 10. Placement flexibility — placements are a user setting, not a fixed kind ✅ (2026-08-25)

Since view, popup, modal, and chat-bar all host the same webview, **one web
extension can support multiple placements**, and the user picks the active
one per extension (in the Installed tab / dropdown row). Manifest change to
Decision 2: replace the single `type` with:

```json
"placements": ["modal", "view", "popup"],   // what the extension supports
"defaultPlacement": "modal"
```

- The extension declares which placements its UI supports and **must be
  responsive** across the sizes of every placement it lists.
- The user's chosen placement is stored in the gxserver extension store
  (Decision 6, alongside `terminalPlacement`) and drives the launch behavior
  in the puzzle dropdown / pinned icon (Decision 8): the same entry opens as
  a view tab, an anchored popup, or a centered modal depending on the
  setting.
- The bridge exposes the active placement (e.g. `ghostex.context().placement`
  + a change event) so the page can adapt its layout.
- `terminal-pane` remains its own kind (command spec, no webview);
  `chat-bar` may be listed as a placement by extensions whose UI makes sense
  docked under the composer.
- The Git extension (Decision 9) is the flagship multi-placement example
  (modal or view).

## Open questions queue

1. ~~Extension runtime model~~ — settled, see Decision 1.
2. ~~Manifest format & folder layout~~ — settled, see Decision 2 (+ Decision 10 `placements` amendment).
3. ~~Install/update mechanics~~ — settled, see Decision 3.
4. ~~Context & capability API~~ — settled, see Decision 4.
5. ~~Trust/security & permissions~~ — settled, see Decision 5.
6. ~~Per-extension settings & preferences~~ — settled, see Decision 6.
7. ~~Extensions browser modal~~ — settled, see Decision 7.
8. ~~Toolbar dropdown + pinning UX~~ — settled, see Decision 8.
9. ~~Example extensions~~ — settled, see Decision 9 (Storybook, Scratchpad, Lazygit + btop, Claude Usage, Git).
10. ~~Placement flexibility~~ — settled, see Decision 10.

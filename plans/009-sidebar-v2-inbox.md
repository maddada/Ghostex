# 009 — Sidebar V2 ("Inbox") for the gpui app

STATUS 2026-07-29: **COMPLETE.** All five phases implemented and independently verified
clean (final targeted confirmation passed: 565 cargo tests, 628 shared tests, 41/41 V2
stories, zero regressions beyond documented foreign breakage). Uncommitted — pending the
user's commit decision.

- 2026-07-29 (post-ship): V2 card polish round — (a) resting rows no longer reserve
  hover-action width (actions float over a theme-matched scrim; hover-reflow invariant
  still holds); (b) project icons: the favicon resolver in
  `gxserver-rs/src/project_icon.rs` checks well-known paths and link-rel
  scan; 64KiB cap, traversal-safe, family-root keyed, content-hash deltas) publishing
  `GxserverPresentationProject.discoveredIconDataUrl`. Icon precedence (deliberate,
  user-confirmed intent): user-set IMAGE → DISCOVERED repo icon → typed Tabler glyph →
  folder. Note: the user's Ghostex project carries a forgotten legacy tabler "archive"
  icon from the deprecated macOS app — discovered favicon must outrank typed glyphs or
  that resurfaces.

Canonical spec, agreed with the user on 2026-07-29. Implementation agents: read this whole file
before touching code. This is the single source of truth; if code reality conflicts with this
doc, flag it to the orchestrator instead of improvising.

## Vision

A flat, position-stable "Sidebar V2" inbox of sessions across all projects,
selectable as an opt-in alternative to the current sidebar. The current sidebar (V1) stays
EXACTLY as it is and remains the default. V2 is a presentation layer plus new lifecycle
state — it must never break existing gpui behaviors (session activation, browser tab
activation, pane focus, wake flows all keep using the same message paths).

## Decisions (agreed with user — do not relitigate)

1. **Shape**: flat session inbox (newest-first, position-stable — activity never reorders rows)
   PLUS a **"Group by Project"** sub-mode: collapsible project groups instead of the header
   scope dropdown. In grouped mode each project has its own collapsed **Settled** shelf at the
   bottom of its group. Worktrees roll up under their parent project (no sibling project rows).
2. **Lifecycle**: settle AND snooze. Server-owned state in gxserver-rs:
   `settledAt`, `settledOverride`, `snoozedUntil`, `snoozedAt` per session. Manual
   settle/un-settle (hover ✓ / ↩), snooze presets popover (In 1 hour / This evening /
   Tomorrow 9am / Next week), Snoozed shelf sorted soonest-wake-first, exact-boundary wake
   timers, raised-hand-while-snoozed surfacing, auto-settle (configurable N days inactive,
   default 3; or PR merged/closed), guards: never settle/snooze a working/attention session.
   Capability flags so older gxservers (remote machines) degrade gracefully (affordances hide,
   nothing auto-settles).
3. **Cross-machine logical projects**: auto-group by normalized git `origin` remote URL
   (gxserver-rs probes it, ships in presentation), per-project override (by repo / by
   repo+path / keep separate) stored in the shared settings file. Machine badge on sessions
   from non-local machines. Non-git projects never merge.
4. **Worktree model**: worktree = ATTRIBUTE of a session (cwd + branch), not a
   registered sibling project. Created lazily and atomically server-side; deleting the last
   session pointing at a worktree offers cleanup (with dirty-status awareness). Old
   worktree-as-project registrations keep working in V1 and display merged under the parent in V2.
5. **Creation UX**: plain + click = instant local session (unchanged). Split-button dropdown /
   context item "New worktree session…" opens a COMPACT popover: agent (default last used),
   base branch (default project default branch), "start from origin" toggle, OPTIONAL first
   prompt. gxserver-rs then atomically: create worktree (temp branch `ghostex/<8hex>`) → run
   the project's existing `worktreeCommand` setup → spawn session inside → rollback on failure.
   Auto-rename branch to a descriptive slug later (reuse existing auto-rename machinery).
   Also an "open existing worktree/branch" path (ideas salvaged from the old
   `sidebar/worktree-create-modal.tsx`, which this flow eventually retires). Per-project
   "default new sessions to worktree" setting.
6. **Rollout**: V1 default everywhere; V2 pure opt-in. Toggle surfaced in BOTH: (a) the
   sidebar Sort & Filter menu, (b) the app Settings — as the FIRST setting at the very top,
   marked with a "New" badge. Setting key `sidebarVersion` in `shared/ghostex-settings.ts`
   rides the existing settings file → hud.settings pipeline (no Rust changes needed).
7. **Git/PR data**: server-side in gxserver-rs, shipped via presentation snapshot/deltas.
   Since each worktree session's cwd IS the worktree folder, resolve per-session git state
   from the session cwd: branch, +n/−n diff stats vs merge-base, PR number+state via `gh`
   when available (aggressive caching; extend the existing 60s worktree-topology cache
   pattern). No `gh` → no PR badge, auto-settle falls back to inactivity-only.
8. **Concept translation in V2** (nothing disappears, but inbox-hostile features stay V1-only):
   - Quick/chat sessions → pseudo-project "Quick" in scope filter; normal inbox rows.
   - Browser sessions → in flat mode: a dedicated "Browser" section; clicking a row
     shows/activates the browser tab tied to that project/worktree (existing machine-scoped
     project-id keyed activation paths). In Group-by-Project mode: browser tabs render as
     today, above the agent sessions inside each project group.
   - Project collections → ordering/grouping in Group-by-Project mode + sections in scope dropdown.
   - Pinned sessions → float above the inbox.
   - Tag filters + search → filter the inbox exactly as today.
   - DROPPED from V2 (V1-only): manual sorting, named session sub-groups.
9. **Phasing**: P1 skeleton → P2 lifecycle → P3 git/PR cards → P4 worktree flow → P5
   cross-machine. Each phase shippable behind the toggle.

## Existing architecture (verified 2026-07-29)

- ONE shared React sidebar: `sidebar/sidebar-app.tsx` (~300KB). gpui mounts it via
  `gpui/sidebar/main.tsx` (CEF, `data-sidebar-mode="combined"`); the runtime adapter is
  `gpui/sidebar/gxserver-runtime.ts` (implements the `vscode` WebviewApi + message source).
  The Swift macOS app is DEPRECATED (see AGENTS.md "Active apps vs deprecated apps") —
  do not do work for it; its adapter is `native/sidebar/native-sidebar.tsx`.
- Contracts: `shared/session-grid-contract-sidebar.ts` (`SidebarSessionItem` line ~267,
  `SidebarSessionGroup` ~440, `SidebarHudState` ~615, message unions ~1036/1067).
  gxserver protocol: `shared/gxserver-protocol.ts` (`GxserverPresentationSnapshot` ~1673,
  `...Session` ~1594, `...Delta` ~1683). Projection:
  `shared/gxserver-presentation-sidebar-projection.ts` (+ existing `.test.ts` precedent).
- Settings: schema `shared/ghostex-settings.ts` (`ghostexSettings`, `DEFAULT_ghostex_SETTINGS`,
  `normalizeghostexSettings`); UI `sidebar/settings-modal.tsx` (Sidebar section ~3409,
  `SidebarPresetField` ~11572 is the pattern template); storage
  `GHOSTEX_HOME/state/native-sidebar-settings.json` via `gpui/src/shared_settings.rs`
  (Rust preserves unknown keys — new UI-only settings need NO Rust change).
- Sort & Filter menu: `SidebarReferenceSectionHeader` in `sidebar/sidebar-app.tsx` (~6011),
  menu body ~6366 via `SidebarContextMenuPortal`. Rendered per-section-header → a global
  option must be threaded from SidebarApp state.
  KNOWN GAP: in gpui, `activeSessionsSortMode` is hardcoded in
  `gpui/sidebar/gxserver-runtime.ts` (~14311) and `setActiveSessionsSortMode` is unhandled →
  Manual Sorting silently no-ops in gpui. Do NOT repeat this pattern for V2; `sidebarVersion`
  goes through the settings pipeline instead.
- Current sort logic: `shared/active-sessions-sort.ts` (`createDisplaySessionLayout`).
- Worktree legacy surface (to be superseded in P4 but kept working):
  `sidebar/worktree-create-modal.tsx`, `worktree-delete-modal.tsx`,
  `shared/project-worktree-order.ts`, gxserver endpoints
  `/api/{listProjectWorktrees,createProjectWorktree,openProjectWorktree,mergeWorktreeIntoMain,deleteWorktreeProject,runWorktreeAction}`,
  `gxserver-rs/src/typed_operations.rs` (build_worktree_command ~1039),
  `gxserver-rs/src/domain.rs` (worktree topology probe ~866, 60s TTL cache).
- gpui CSS entry: `sidebar/styles.css` import chain (last import wins). Reskin precedent:
  `sidebar/styles/hierarchy-panels.css` gated reskin; V2 should use a
  `data-sidebar-version="v2"` attribute on the sidebar root for its stylesheet.

## Testing rules (repo law — see AGENTS.md)

- NO tests in `gpui/` and none for the deprecated macOS app.
- Unit tests in `shared/` ARE welcome (existing precedent: `*.test.ts` next to modules) —
  use them for all ported pure logic.
- Storybook is the primary UI verification harness: `bun run storybook` (port 6006, config
  `sidebar/.storybook`). Follow the precedent of `sidebar/sidebar-app.interactions.stories.tsx`
  (mock message source / vscode shim driving the real `SidebarApp`). Every V2 phase must add
  stories exercising its states with mock data (flat mode, grouped mode, shelves, statuses,
  empty states). Verifier agents drive Storybook via CDP.
- Never run `bun run start` or anything that restarts the user's app.
- Temp dev changes to speed up verification are allowed but MUST be clearly marked
  (`// TEMPDEV:` comment) and removed before a phase is declared done.

## Concurrency rules

Multiple agents (ours and others) share this checkout. Re-read files before editing, targeted
edits only, never whole-file rewrites from stale content, never revert foreign hunks, no
commits unless the orchestrator says so. See AGENTS.md.

## Phase acceptance criteria

### P1 — skeleton (no server changes)
- `sidebarVersion: "v1" | "v2"` + `sidebarV2Layout: "flat" | "byProject"` settings keys
  (defaults "v1"/"flat"), normalized + defaulted in `shared/ghostex-settings.ts`.
- Settings modal: new entry rendered at the very TOP of the General settings tab, "New" badge,
  copy pattern from `SidebarPresetField`. Searchable.
- Sort & Filter menu: "Sidebar" radio group (Classic / V2 Inbox) above the sort radios; works
  in gpui (via settings pipeline, not the broken sort-mode channel). When V2 active, a
  "Group by Project" toggle item appears; the Manual Sorting radio is hidden/disabled.
- V2 render tree in new files (e.g. `sidebar/v2/`): flat inbox (position-stable
  creation-order, newest first; pinned float on top; attention/working statuses shown with
  the three-hue system), Group-by-Project mode (collapsible groups, browser tabs above
  agent sessions per group, per-project Settled shelf placeholder), Browser section (flat
  mode), scope filter dropdown fed by projects (+ "Quick"), search + tag filters applied,
  session click / context menu / rename delegate to the SAME message handlers V1 uses.
- Ported pure logic in `shared/sidebar-v2-*.ts` with unit tests (status resolution, sorting,
  settled/snoozed partition — partition can run on derived data until P2 wires real state).
- Storybook stories for all of the above; V1 stories unchanged and passing.
- Zero behavior change when `sidebarVersion` is "v1" (the default).

### P2 — lifecycle (settle/snooze)
- gxserver-rs: session fields settledAt/settledOverride/snoozedUntil/snoozedAt persisted,
  RPCs (settle/unsettle/snooze/unsnooze), presentation fields + deltas, capability flags in
  bootstrap/summary, server-side auto-settle sweep (inactivity, guards per decision 2).
- Client: shelves live, hover actions, snooze popover, wake timers, un-settle, bulk ops via
  existing multi-select if trivial (else defer, note it). `sidebarAutoSettleAfterDays` setting.
- Unit tests in shared/ for predicates; Storybook stories for shelves/popover; gxserver-rs
  `cargo test` where the crate already has test precedent.

### P3 — git/PR cards
- gxserver-rs per-session-cwd probe: branch, diff stats vs merge-base, PR via `gh`
  (cached, throttled, non-blocking); presentation fields; card row 3 UI (branch, PR badge
  colored by state, +n −n); PR-merged/closed auto-settle; tooltip with branch/mismatch info.

### P4 — worktree flow v2
- Per decision 5. Atomic server path with rollback; compact popover; split + button;
  open-existing path; last-session-delete cleanup prompt with dirty check; temp branch
  `ghostex/<8hex>` + auto-rename integration; per-project default-env-mode setting.
  Old modal remains functional until this phase fully replaces its entry points, then its
  entry points switch to the new flow (modal code may remain for V1 delete flow if needed).

### P5 — cross-machine logical projects
- Per decision 3. Remote-URL probe in gxserver-rs → presentation; client logical grouping +
  overrides UI (project context menu / project actions dialog pattern); machine
  badges; scope filter shows logical projects; Group-by-Project groups merge across machines.

## Status log

- 2026-07-29: Spec written. P1 started (P1a settings/toggle agent + P1b logic-port agent in
  parallel, then P1c UI, then P1v verification loop).
- 2026-07-29: **P1 COMPLETE and verified clean** (implement → adversarial verify → fix →
  re-verify). Sidebar V2 lives in `sidebar/v2/`, logic in `shared/sidebar-v2-*.ts`, settings
  keys `sidebarVersion`/`sidebarV2Layout`, stylesheet `sidebar/styles/sidebar-v2.css`
  (scoped `[data-sidebar-version="v2"]`), 13 passing V2 stories. Notable: sidebar-originated
  `updateSettingsPatch` now actually persists in gpui (rides the app-modal host bridge).
- 2026-07-29: **P2-server COMPLETE**: `gxserver-rs/src/session_lifecycle.rs`, migration 0016,
  endpoints `/api/{settleSession,unsettleSession,snoozeSession,unsnoozeSession}`
  (remote-allowed; settle rejects working+attention, snooze rejects attention only —
  snoozing a WORKING session is allowed by design), presentation fields + snapshot
  `capabilities: {sessionSettlement, sessionSnooze}`, 60s sweep for auto-settle
  (`sidebarAutoSettleAfterDays` read server-side from native-sidebar-settings.json,
  default 3, null/<=0 disables; capped 100 writes/pass).
- ACCEPTED DEVIATIONS — do NOT "fix" these, they are deliberate:
  - Snooze expiry is DERIVED client-side from retained `snoozedUntil` (wake-to-the-ms);
    server GCs spent snooze fields only after +24h so the "Woke" indicator survives.
  - Snoozing a settled session does NOT un-settle it.
  - Auto-settle sets `settledOverride:"settled"` but leaves `settledAt` NULL (so settled
    sort falls back to the activity clock); manual settle stamps `settledAt`.
  - `settledOverride` values are `"settled" | "active"`. Server keeps an internal
    `settledOverrideAt` (not published) so real activity newer than the override clears it.
  - Browser-row ordering in Group-by-Project mode follows V1's activity order (explicit
    user decision), not the position-stable rule.
  - `shared/sidebar-v2-snooze|logical-project|worktree-cleanup.ts` shipped in P1 ahead of
    their consuming phases (intentional).
  - Default/unclassified attention renders `data-kind="input"` with `data-hue="amber"` —
    CSS and tests must key attention colors off `data-hue`, never `data-kind`.
- Known foreign breakage (NOT ours): tests `discover-ghostex-modal-source` +
  `watch-ghostex-video-modal-source`; V1 interaction stories broken at HEAD by a concurrent
  top-row/command-palette rework; `LightOrange` settings story ignores its theme setting.
- 2026-07-29: **P2 COMPLETE and verified clean** (server + client, live isolated-daemon
  matrix passed). Further accepted items: the Woke pill is amber BY DESIGN;
  activity-reset lag on the Settled shelf (≤60s, server-owned override stamp) is inherent
  and correct; client-side auto-settle window vs remote machines' own windows is a known
  P5 item (fix by trusting the remote server's classification / carrying the window
  per-machine). Bulk settle/snooze deferred until V2 has multi-select.
- P3 WIRE CONTRACT (agreed): `GxserverPresentationSession.gitStatus?: { branch: string|null,
  additions: number, deletions: number, prNumber?: number, prState?: "open"|"draft"|
  "merged"|"closed", prUrl?: string, updatedAt: string }` + snapshot capability
  `sessionGitStatus: true`. TS type is owned by the P3-client agent; Rust emit must match
  it exactly. Probes are per unique session CWD (many sessions share one cwd), cached
  (~60s git TTL, ~5min PR TTL), throttled, non-blocking, git commands time-boxed; `gh`
  absent/unauthed → no PR fields, no errors. Diff stats = session cwd worktree vs
  merge-base with the repo default branch (committed-on-branch + uncommitted).
  PR merged/closed → auto-settle eligible immediately (same working/attention guards).
- 2026-07-29: **P3 COMPLETE and verified clean** (live isolated-daemon matrix + Storybook +
  wire-contract byte parity all passed). Optional minors carried forward: probe skips
  pinned-stopped rows (only live sessions probed — semi-intentional); silent cache pruning
  can leave stale gitStatus on published stopped rows until next snapshot; transient gh
  auth flaps drop/restore PR badges one pass (self-healing); light-theme story surface
  stays dark (pre-existing harness limitation). A fixer is bounding the probe's
  reader-join timeout (the one non-time-boxed wait).
- P4 WIRE CONTRACT (agreed): POST `/api/createWorktreeSession`
  `{ projectId, agentId?, baseBranch?, startFromOrigin?, firstPrompt?,
     existingWorktree?: { path: string } }` → `{ sessionId, worktreePath, branch }`;
  server-side atomic sequence (optional fetch origin → `git worktree add -b ghostex/<8hex>`
  → run the project's worktreeCommand setup → create+register the session with
  cwd=worktreePath via the NORMAL gxserver createSession machinery → optional first prompt
  → rollback (remove worktree) if any step fails; reuse existing typed_operations worktree
  command builders + path-safety normalization). `existingWorktree.path` skips creation and
  spawns into that path. Snapshot capability `worktreeSessions: true`. Branch display needs
  NO new fields (P3 gitStatus already shows the worktree's branch from the session cwd).
  Cleanup: POST `/api/removeSessionWorktree` `{ projectId, worktreePath, force? }` →
  `{ removed, dirty?, warnings? }` (dirty check first; force overrides), used by the
  client's "last session in this worktree closed → remove worktree?" prompt.
  Sidebar messages: `createWorktreeSession` (mirror of the endpoint params) and
  `removeSessionWorktree`; TS types owned by the P4-client agent, Rust matches exactly.
  Temp-branch auto-rename: when a temp `ghostex/<8hex>` branch's session gets a real
  (non-temporary) title, server may rename the branch to `ghostex/<slug>`; if wiring into
  the existing title flow is too entangled, defer with a note — do NOT hack it.
- 2026-07-29: **P4 IMPLEMENTED** (server + client, verification pending). Server:
  `gxserver-rs/src/worktree_sessions.rs` + endpoints in server.rs; auto-rename WIRED via a
  60s reconcile pass (marker-stamped sessions, renames once, re-probes git cache);
  worktrees are sibling dirs `<project>-<hex>`, sessions NEVER registered as projects.
  Client: split + button (V2 toolbar + byProject group headers), compact popover on the
  context-menu portal, "New session on <branch>", cleanup prompt with dirty→force
  re-prompt, `requestId`-paired result messages `worktreeSessionResult` /
  `sessionWorktreeRemovalResult`. FURTHER ACCEPTED DEVIATIONS (deliberate):
  - Remote machines: endpoints are RemoteAllowed server-side, but the gpui remote bridge
    allowlist (`gpui/src/main.rs` ~81130/81439) does not carry them yet → the runtime
    REFUSES remote with a clear toast. Flip in P5 (one line per list + param shaper).
  - Global `newSessionsDefaultEnvMode: "local"|"worktree"` (not per-project), surfaced in
    the "+" chevron menu, NOT the Settings modal.
  - `SidebarSessionItem.cwd?` added (projected from existing presentation data) — cleanup
    needs the path; not a server change.
  - Cleanup prompt is a sidebar-document dialog reusing `.confirm-modal-*` chrome (the
    native deleteWorktree modal stays V1-only). "Managed" = the whole `ghostex/` branch
    namespace; anything else is never offered for deletion.
  - `removeSessionWorktree` warnings are plain user-safe strings (never raw git output).
- 2026-07-29: P4 verification: NEEDS FIXES → fix round dispatched. MAJOR: "New session on
  <branch>" fails for project-root sessions (client must hide it when the row's cwd is the
  project root — plain + covers that case). Minors: session-row orphan on identity-apply
  failure; delta failure after provider start must not roll back a live session; failed
  `worktree add` must prune; shared-worktree detection must count sessions by cwd equality
  (not only gitStatus-probed rows); removeSessionWorktree must refuse a REGISTERED worktree
  project's checkout (V1 delete flow owns those). Accepted nits (documented, no fix):
  raw git text on the error channel (pre-existing pattern), adopted-marker sweep cost,
  existingWorktree ignoring baseBranch/startFromOrigin, symlinked-cwd string compare,
  light-theme popover button (harness limitation). Load-dependent V1 story flakes noted
  for future verifiers (9 stable foreign failures; drag/sort/card-actions flake under load).
- P5 WIRE CONTRACT (agreed): `GxserverPresentationProject.gitRemoteOriginUrl?: string|null`
  (probed server-side with TTL caching like the topology probe; null = no origin; absent =
  not yet probed/non-git). Snapshot gains `autoSettleAfterDays?: number|null` — the window
  THAT daemon actually uses — so clients partition each machine's sessions with the right
  window (fixes the P2 remote-window minor). Client-side: logical grouping via the existing
  `shared/sidebar-v2-logical-project.ts` module; per-project grouping overrides stored in
  `ghostex-settings.ts` as `sidebarProjectGroupingOverrides:
  Record<string, "repository"|"repositoryPath"|"separate">` (key = the module's physical
  project key); machine badges on merged rows (remoteMachineContext already exists).
  gpui remote bridge: add createWorktreeSession + removeSessionWorktree to the allowlist in
  gpui/src/main.rs (+ param shaper mirroring the lifecycle shaper), then flip the runtime's
  remote refusal to real routing.
- 2026-07-29: **P4 FIX ROUND COMPLETE** (all six findings fixed, non-vacuously tested).
  **P5 IMPLEMENTED** (server + client; final verification pending). Key P5 facts:
  probe module `gxserver-rs/src/project_git_remote.rs` (10min repo TTL / 30min non-repo,
  family-root keying, piggybacks the 60s git-status task, warm probe on projectAdded);
  P3's git runner is now shared as `run_git_probe_command`. Client: grouping module mode
  renamed `repository_path` → `repositoryPath` (one spelling end-to-end); override key =
  `<machineId>:<normalizedPath>`; `"separate"` stored explicitly; merged groups addressed
  by a representative host group id with memberGroupIds; merging happens AFTER per-daemon
  capability/window/partition decisions; per-machine windows: remote daemon that doesn't
  publish autoSettleAfterDays → NULL window client-side (server override is the source);
  settings source `sidebar:projectGrouping`; override UI = byProject group-header context
  menu only (no Settings-modal control — per-checkout keys aren't a Settings shape);
  remote worktree endpoints allowlisted + shaped in gpui/src/main.rs, runtime routes remote
  (120s create / 60s remove timeouts). 38 V2 stories total.
- 2026-07-29: FINAL VERIFICATION: whole feature clean EXCEPT two P5 findings → last fix
  round dispatched. (1) MAJOR: "Repository + path" override is inert — nothing publishes a
  repository root. CONTRACT ADDITION: presentation project gains
  `gitRepositoryRootPath?: string` (probed via `git rev-parse --show-toplevel` in the same
  project_git_remote cache entry; absent for non-git), client populates
  `repository.rootPath` from it so `repositoryPath` keys become real; tests must use the
  shipped shape. (2) minor: restoring a parked project loses its origin ~60s (warm the
  remote cache on projectUpdated deltas too / don't evict merely-unpublished projects).
  Plus UX fix: choosing "Repository" mode on a split group applies the override to every
  project sharing that repository identity (symmetric one-click re-merge), and the
  remote-probe abandoned-reader log gets its own event name. Accepted (documented, no
  fix): probe-on-create bounded blocking, 24-probes/pass saturation ~240 projects,
  non-`git@` scp spellings not canonicalized, literal "local" machine-id collision,
  Woke/Approval both amber, shelf-header style asymmetry, 260px truncation
  (SUPERSEDED — fixed 2026-07-29, see the row-1 entry at the end of this log),
  Storybook light-theme harness limitation.
- 2026-07-29: **FINAL FIX ROUND COMPLETE** (all four items, non-vacuously tested).
  (1) `gitRepositoryRootPath` now rides the SAME `project_git_remote` probe/cache entry/TTL
  as the origin URL (one extra `rev-parse --show-toplevel`, family-root keyed, root changes
  delta exactly like URL changes; two states only — string or ABSENT, no null). Carried
  through protocol → sidebar contract `projectContext` → projection → `toSidebarV2Project`,
  so `repository.rootPath` is finally populated and `repositoryPath` keys are real.
  (2) The `origin` warm now follows PUBLICATION instead of `delta_type == "projectAdded"`
  (`ensure_published_project_git_remote_probed`): a restored parked project carries its
  remote in the delta that restores it, and a parked/hidden project is never probed at all
  (no evict↔warm ping-pong). (3) Choosing "Repository"/"Repository + path" writes the
  override for every VISIBLE row sharing the repository (new `SidebarV2GroupModel.
  repositoryCanonicalKey`), so one click on ANY split row re-merges the set; "Keep separate"
  keeps its narrow scope. (4) The `origin` probe has its own abandoned-reader counter and
  logs `projectGitRemoteReaderAbandoned`.
  Verification: gxserver-rs 565 tests green; new story fixture `sidebar-v2-monorepo` (two
  sub-projects of one checkout + the same sub-path on a remote machine) proves the three
  modes now produce three DIFFERENT lists; 41 V2 stories green (38 + 3); both the
  repositoryPath-splits story and the re-merge step were confirmed to FAIL with their fix
  temporarily removed; live isolated-daemon run confirmed the published root and the
  park→pass→restore no-gap behaviour.
- 2026-07-29: **ROW-1 FIX ROUND** (two user-reported issues on the card's project line).
  (1) TRUNCATION: the F8 hover-stability fix had stacked status and actions in ONE grid cell,
  so `.sidebar-v2-row-slot` reserved the WIDER child — the ~135px action bar — on every
  resting row; at the default 260px width the project name got a 59px box and ellipsised
  against a half-empty line. The action bar is now out of flow (absolute inside the slot,
  `right: 0`), so the slot reserves the STATUS width only and the name flexes into the rest
  (59px → 162px measured). F8 still holds and is now measured, not assumed: the status keeps
  its box under `visibility: hidden` and the bar never participates in layout, so the name's
  box is byte-identical at rest and revealed. The bar paints an opaque row scrim
  (`--sv2-row-scrim`, the same tint as `--sv2-row-hover` composited over `--app-background`)
  with an 18px fade, and the two pointer-less reveals (`data-menu-open`, keyboard focus) now
  also tint the row, so the bar can never read as a lighter patch. Slim shelf rows moved
  their PR badge INSIDE the resting slot (it used to rely on the reserved width to stay
  clear of the actions), so it swaps with the time label instead of sitting under the fade.
  (2) PROJECT ICONS: row 1 always showed a folder because only `iconDataUrl` (the IMAGE
  variant) was carried, while almost every real Ghostex project carries a TYPED Tabler icon
  (`identityIcon.icon`). `projectContext.icon` now rides protocol → projection →
  view-model identity → row / group header / scope menu, and `SidebarV2ProjectIcon` resolves
  image → Tabler glyph → folder, mirroring `RecentProjectIcon` exactly; folder is now only
  for projects with no icon at all.
  Verification: new fixture `sidebar-v2-row-width` + story `ProjectLineWidth` measures a
  fitting name (scrollWidth ≤ clientWidth), a genuinely-too-long name (still truncates), and
  zero reflow across the reveal driven through the SHIPPED `data-menu-open` rules; both
  halves were confirmed to FAIL with their fix temporarily removed (in-flow bar → name box
  20px; in-flow reveal → 162px→20px on hover). 42/42 V2 stories green,
  typecheck clean, 630 shared + 365 sidebar tests green (only the two documented foreign
  modal-source failures), V1 suite unchanged at its 10 documented failures.

## 2026-07-30 UX FIX BATCH — locked user decisions (implementation in flight)

User-reviewed decisions for the next batch. These override earlier text above where they
conflict. Each item names its owner surface; implementation agents must not drift from
these.

1. **Hover action bar: no scrim.** Remove the `--sv2-row-scrim` gradient background behind
   `.sidebar-v2-row-actions`. The bar stays out-of-flow/absolute (the F8 no-reflow
   invariant is NOT relaxed); only the painted background goes. The individual buttons
   are restyled to match the React Native mobile app's project-header buttons (spec from
   the RN research report; token-mapped to sidebar CSS variables).
2. **Pin button:** leftmost in the action bar and rendered ONLY when the session is
   pinned (acts as unpin). Pinning an unpinned session happens via the context menu.
   Additionally add a small resting pin indicator on pinned rows (user approved).
3. **Meta line shows branch, never folder path.** Kill the `detailText` folder-path
   fallback on the meta line: render the git line (branch/PR/±) when available, otherwise
   drop the meta line. Root-cause and fix why `gitStatus` never reaches the user's local
   cards (diagnosis agent report; suspects: v2-gating from perf batch 4a59b50d7 or the
   `gitStatusCapabilityByGroupId` keying at sidebar-v2-root.tsx:940-950).
4. **Single create control.** In V2 mode the shared V1 header trio
   (Quick Browser Tab, Quick Terminal, agent split button) is no longer rendered; V2's
   split "+" is the only create control. Main click = primary-agent session in the
   resolved target project (see 6). Chevron menu: agent picker, "New worktree session…",
   "Quick Terminal", "Quick Browser Tab", default-to-worktree toggle. Grouped mode keeps
   its per-group "+".
5. **Browser shelf moves to the TOP of the flat list** (above active cards). Grouped mode
   already renders browser rows first per group — unchanged.
6. **Create-target fix.** The plain "+" resolves its target as scope → active project →
   first project group (same resolution as `headerWorktreeGroupId` minus the worktree
   capability filter) instead of falling into `createReferenceAgentChat`'s hard-wired
   Quick substitution (sidebar-app.tsx:4667-4680). Quick creation remains available but
   only via the explicit chevron menu items.
7. **Context-menu parity.** Bring the applicable V1 session context-menu items into the
   V2 row menu (per the V1 parity research table); keep the V2 lifecycle items.
8. **Remove the ⋯ (dots) button from V2 rows.** Right-click is the only menu trigger.
9. **Group-by-Project mode adopts V1's project UX.** Grouped V2 shows only OPEN projects
   (V1 open/closed semantics, closable from the UI, re-openable via the existing recent
   projects flow), supports V1-style project reordering, and reuses the V1 project header
   look — while sessions inside each project render as V2 big cards. Architecture per the
   grouped-UX research recommendation (reuse V1 components vs V2-tree reimplementation —
   whichever that report justifies).
10. **V2 context menu looks and behaves like V1's** (user, 2026-07-30, after item 7
   landed): the inbox sidebar's context menu must match the classic sidebar's menu in
   LOOK and BEHAVIOR — same visual chrome/CSS, same submenu/positioning/interaction
   feel. Prefer reusing V1's actual menu rendering (`SidebarContextMenuPortal` and its
   styles) over restyling V2's custom menu, unless code reality argues otherwise. Item
   set/gating stays as delivered by item 7.

### Status log (continued)

- 2026-07-30: **ITEM 3 SERVER HALF COMPLETE** — root cause of "no branch on V2 cards" found
  and fixed in gxserver. Agent sessions are created with `cwd = NULL` on purpose (they run
  in the project's path; no `createAgentSession` caller sends a cwd), and the git-status
  subsystem was the ONLY place in gxserver reading `session.cwd` raw — every launcher
  (`zmx.rs`, `agents.rs`) already resolves `session.cwd` else `project.path`. So the probe
  set skipped every agent session, nothing ever probed the project root, and presentation
  had no cache entry to attach. (Live evidence before the fix: 57 published sessions, the
  only one with `gitStatus` was a terminal session with an explicit cwd.)
  Fix: one resolver, `session_git_status::effective_session_git_cwd(session, project)` —
  session `cwd` (trimmed, non-empty) else `project.path`, using the project's OWN path, not
  the worktree family root, so a worktree project still probes its own checkout. Applied at
  the three previously-blind call sites: presentation's `gitStatus` attach, the 60s refresh
  pass (project lookup map built once per pass, both the probe set and the delta loop), and
  `session_pull_request_disposition` (now takes the project, so PR-driven auto-settle can
  fire for agent rows; the lifecycle sweep's injected-resolver signature is unchanged — the
  server passes a project-resolving closure). Because the cache is keyed by cwd, one probe
  of a project path lights up every row pointing there, including the stopped/pinned rows
  the pass deliberately never probes for.
  Deliberately NOT changed: the published `session.cwd` field stays raw (V2 worktree logic
  reads it to tell a managed worktree checkout apart from a project-root session), and
  nothing persists `project.path` into the session row (stale on project move, and it would
  not heal existing rows). TTL/negative-cache/counter behaviour untouched.
  Verification: `cargo check` exit 0, `cargo test` exit 0 (597 passed, 0 failed) with three
  new tests — the resolver's rules (session cwd wins, blank/whitespace falls through,
  no project path → None, worktree project probes its own checkout), the PR-disposition
  fallback, and a presentation test proving a cwd-less agent session publishes `gitStatus`
  while still publishing NO `cwd`; all three were confirmed to FAIL with the fallback
  temporarily removed. LIVE: two disposable `env -i` daemons on isolated HOMEs (ports
  58891/58892, user daemon on 58744 untouched) against a scratch repo on branch
  `sv2-fallback-probe` with 2 unstaged additions — the fixed daemon published
  `{"branch":"sv2-fallback-probe","additions":2,"deletions":0}` on an agent session with
  `"cwd": null` within one probe pass, while the same setup on a pre-fix control binary
  still published no `gitStatus` after ~84s of passes.
- 2026-07-30: **UX BATCH ITEMS 1, 2, 8 + CLIENT HALF OF 3 DONE** (row chrome + meta line;
  files: `sidebar/v2/sidebar-v2-session-row.tsx`, `sidebar/styles/sidebar-v2.css`, V2
  stories/fixtures).
  (1) NO SCRIM: `--sv2-row-scrim`/`--sv2-row-scrim-active` and the gradient behind
  `.sidebar-v2-row-actions` are gone; the bar stays absolute inside the 20px row slot
  (F8 unrelaxed, `ProjectLineWidth` still measures zero reflow across the reveal). The
  controls are now the project header's chip token-for-token (`.group-add-button` /
  RN `buttonStyles.button`): 20×20, 1px border, 6px radius, accent-tinted glyph, accent
  hover/active fills; Settle is the one `width:auto` labelled chip. 20px not 22px because
  the slot and line 1 are both 20px tall and a 22px chip spills 1px past its own line.
  ONE deliberate token deviation: the chip fill mixes with `var(--app-background)`, not
  `transparent` — at 0.88 alpha a long project name read straight THROUGH the buttons
  (screenshotted), so the chip hides what it covers itself instead of a scrim doing it; the
  4px gaps between chips are the only place text still shows.
  (2) PIN: the pin control is rendered ONLY on pinned rows (it unpins) and is LEFTMOST;
  pinning is the context menu's job. Pinned rows carry a 12px `IconPinned` mark inside the
  resting slot (before the status), so it is resting content that swaps out for the unpin
  chip — the reveal still changes nothing about line 1's boxes. The old
  `[data-pinned="true"] .sidebar-v2-row-action` accent smear is replaced by
  `[data-row-action="unpin"]`.
  (8) The ⋯ button is gone (with its `IconDots` import); right-click is the only menu
  trigger and the menu still anchors to the pointer (`menuStyle` left/top from the
  contextmenu coordinates, now asserted).
  (3, client) The meta line renders GIT or nothing: `session.detail` is dropped entirely
  (`.sidebar-v2-row-meta` deleted). Root cause of the "folder path" report is confirmed in
  the wire shape, not in the row: gxserver's `snapshot_subtitle` publishes `session.cwd`
  else `project.path`, i.e. detail is ALWAYS a path (the P3 comment claiming it was the
  agent name, and the fixtures' `detail: "Claude Code"`, were both wrong). DECISION: a
  machine badge with no git data KEEPS the line (`data-meta="machine"`, badge gets
  `margin-left:auto` so it holds the line's right edge in both shapes) — remote context is
  the one fact a row cannot state anywhere else, and a remote daemon without the git probe
  is exactly that case. Cards therefore drop to `data-card-lines="2"` when they have
  neither.
  Verification: 48/48 V2 stories green (42 baseline + this batch's + the concurrent
  create-button/browser-shelf stories) driven through a fresh CDP target per story in
  chrome-headless-shell against `storybook dev` on a free port (6199) — `bun run storybook`
  was never used, `shadcn.generated.css` untouched. New story `RowActionChips` reads the
  SHIPPED rules: bar `background-image: none` + `position: absolute`, chip 20×20 with a 1px
  solid 6px-radius border and an OPAQUE fill (an alpha < 1 fails the assertion), Settle
  wider than 20px, unpin first and only on the pinned row, no `Pin session` control and no
  `Session actions` control anywhere, resting pin mark inside `.sidebar-v2-row-slot-status`,
  right-click menu anchored to the click coordinates with a `Pin` item. Meta-line coverage
  updated in `GitAndPullRequestCards` / `WithoutGitCapability` / `RendersPullRequestStateOnCards`
  (no git → no `[data-line="meta"]`, card-lines 2) and in `BadgesRemoteRowsOnly` (badge
  without git keeps the line as `data-meta="machine"`, card-lines 3; unbadged local rows
  drop it). `RunsSessionContextMenuActions` now pins through the menu and unpins through the
  chip (asserting the chip is the bar's first child). Real-pointer hover screenshots (CDP
  `Input.dispatchMouseEvent`) of a pinned row and of a genuinely-too-long project name
  confirmed the chip look and drove the opaque-fill decision. Typecheck clean; 648 shared
  tests green; sidebar 371 passed / 3 failed — the two documented foreign modal-source
  failures plus a NEW foreign one (`command-palette.test.ts` expecting `"closeAfterDone"` in
  `PANE_ACTION_COMMAND_IDS`, from a concurrent agent's work, untouched here).
  **CORRECTION (same day, after the honest-runner finding below):** the 48/48 above came from
  a phase-polling runner, which reports a thrown play function as PASS. Re-run under the
  CHANNEL-event runner, two of this batch's stories were genuinely FAILING, and both were bad
  STORY EXPECTATIONS, not bad shipped behaviour:
  (a) `BadgesRemoteRowsOnly` asserted `data-card-lines="3"` for a badged remote row and `"2"`
  for the unbadged local ones. That story set runs in **Group-by-Project**, where the card
  drops its project line — so a badge-only card is 2 lines and a card with neither git nor a
  badge is 1. Fixed to 2 / 1, plus an explicit `[data-line="project"]` null check so the
  grouped premise is stated rather than assumed. The machine-badge-keeps-the-line decision
  itself is unchanged and still verified (`data-meta="machine"`, no `[data-sidebar-v2-git]`).
  (b) `RowActionChips` asserted the context menu's inline `left`/`top` equal the right-click
  coordinates, taken from the ROW's rect. `SidebarContextMenuPortal` viewport-CLAMPS those
  coordinates, so on a wide canvas the row sat far enough right that the assertion measured
  the clamp (596.375px vs the expected 708px) — a canvas-width-dependent expectation. Fixed
  by firing the contextmenu at a fixed 40/40 (a contextmenu event's coordinates need not lie
  inside its target), which is unclamped on any canvas, with a non-vacuity check that the
  menu really fits in the viewport there.
  Honest re-run: **48/48 V2 stories green** under the channel-event runner (`storybook dev` on
  free port 6301, chrome-headless-shell on 9421, one fresh target per story). The runner was
  itself re-proved non-vacuous here: temporarily expecting a 21px chip made
  `RowActionChips` FAIL with `PLAY_ERROR:expected '20px' to be '21px'`. Typecheck still clean.
- 2026-07-30: **UX BATCH ITEMS 4 / 5 / 6 IMPLEMENTED** (single create control, create-target
  resolution, Browser shelf on top).
  (4) V2's header no longer mounts the classic create trio. The V2
  `SidebarReferenceSectionHeader` mount withholds `onCreateBrowserChat`, `onCreateChat`,
  `onRunAgent` and `onConfigureAgents` (plus `agents` / `primaryAgentId` /
  `useColoredAgentIcons`, which only fed that split button); all four are optional props, so
  the buttons simply do not render and the header is left with Sort & Filter. Every V1 mount
  still passes the whole set. V2's split "+" is now the only create control, and its chevron
  menu is, in order: the agent picker (every configured agent, brand icon + checkmark on the
  last-used one, launched through the caller's OWN agent path so the last-used bookkeeping is
  unchanged), "New worktree session…", separator, "Quick Terminal", "Quick Browser Tab",
  separator, "Default new sessions to worktree". The chevron now renders whenever the CALLER
  supplied menu content: the toolbar always has the picker + Quick items, so its chevron is
  present even on daemons without `worktreeSessions` (it used to vanish), while the
  per-project group headers pass neither, so their control is byte-identical to before — no
  chevron without the capability. The worktree preference alone can never open a menu, and
  both worktree items still hide per capability. The only header entry point dropped is
  "Configure agents", which stays reachable through the command palette
  (`configureAgents` command).
  (5) Flat mode renders the Browser shelf ABOVE the active cards (grouped mode already put
  each project's browser rows first, and both the view model and the CSS were
  position-agnostic, so this is a pure JSX move).
  (6) New `headerCreateGroupId` in `sidebar-v2-root.tsx`: scoped project → active project →
  first project group, keyed on `projectContext !== undefined` (the Quick collection has
  none) and WITHOUT the worktree-capability filter that `headerWorktreeGroupId` applies. The
  plain "+" and every agent-picker item target it, so `runSidebarV2Agent`'s Quick
  substitution is no longer reachable from an ordinary create path. That branch is KEPT and
  documented as the zero-project-groups case, where Quick is the only truthful destination
  rather than a downgrade; deliberate Quick creation happens only through the two labelled
  chevron items, which post the same `createChat` / `openBrowserChat` the classic header
  always posted.
  Verification: typecheck clean; 648 shared tests green; sidebar 371 passed / 3 failed (the
  two documented foreign modal-source failures plus the foreign `command-palette.test.ts`
  one already recorded above — none in V2 files). Stories: new
  `HeaderHasOneCreateControl` (header shows ONLY Sort & Filter, toolbar owns the single
  split control, plain "+" posts `runSidebarAgent` with the RESOLVED project id and nothing
  with any other group id, the chevron lists picker + worktree + both Quick items + the
  toggle, an agent pick posts that agent into the same project, and the Quick items post
  `createChat` / `openBrowserChat`), `BrowserShelfLeadsTheFlatList` (shelf header is the
  list's first child; tones are browser, snoozed, settled), and
  `ClassicSidebarKeepsItsCreateTrio` (V1 mode still renders Quick Browser Tab, Quick
  Terminal and the agent split button, and mounts no V2 toolbar — the guard against this
  removal leaking into the shared component). Updated
  `HidesWorktreeAffordancesWithoutCapability` (the chevron now stays; what must disappear is
  every worktree item inside it), `PlainCreateButtonStartsAnInstantSession` (asserts the
  resolved project id) and `FlatInbox` (browser cards lead the list, so the inbox's first
  card is the first non-browser card). Both halves were confirmed NON-VACUOUS with the fix
  temporarily removed: `runInstantSession(undefined)` and re-adding one header create prop
  each made `HeaderHasOneCreateControl` fail.
  **HARNESS FINDING (affects every earlier "N/N green" in this log):** the CDP runners used
  so far decide a story's verdict by polling `storyRenders[last].phase`. A play function that
  THROWS transitions errored → completing → completed → finished, so a 250-300ms poll almost
  always observes `finished` and reports the story as PASSING. Proved with a deliberately
  broken assertion, which the poll-based runner called PASS. Verdicts must come from the
  Storybook CHANNEL instead: install a listener before navigation
  (`Page.addScriptToEvaluateOnNewDocument`) for `playFunctionThrewException`,
  `storyThrewException`, `storyErrored` and `storyRenderPhaseChanged`, and fail on the first
  of those. Re-running the whole V2 set that way gives **46/48**, with the two failures both
  in the concurrent row-actions/meta-line surface and NOT in this batch's files:
  `sidebar-v2-logical-projects--badges-remote-rows-only` (`data-card-lines` is 2 where the
  story expects 3 — the badge-keeps-the-meta-line exception does not hold on disk;
  width-independent, so it is a real mismatch) and `sidebar-v2-inbox--row-action-chips`
  (measured 596.375px vs an expected 708px — a width measurement, so it may be canvas-width
  dependent in a 1200×900 headless window). Flagged to that owner rather than touched here.
  The V1 `sidebar-app.interactions` set measures 5/20 under the honest runner (element-not-
  found failures in V1 project/session-card markup, none of them about the section header);
  this batch cannot affect V1 rendering — every hunk is inside the `isSidebarV2Active`
  branch, in V2-only files, or a comment.
- 2026-07-30: **UX BATCH ITEM 7 DONE — context-menu parity** (files:
  `sidebar/v2/sidebar-v2-context-menu.tsx`, `sidebar/v2/sidebar-v2-messages.ts`,
  `sidebar/v2/sidebar-v2-root.tsx`, `sidebar/v2/sidebar-v2-story-fixtures.ts`,
  `sidebar/v2/sidebar-v2.interactions.stories.tsx`, `sidebar/sidebar-story-workspace.ts`).
  The V2 row menu now carries the applicable V1 session-menu items, and — the load-bearing
  decision — it no longer RE-DERIVES their gates. `sidebar-v2-root` calls V1's exported,
  dnd-free `getSidebarSessionContextMenuEligibility` and passes the answer in; the menu
  module imports only its TYPE, so it still pulls in no V1 runtime. That is the only way the
  two menus can be guaranteed to agree about which agents can fork, which can be resumed
  from a copied command, and what a remote row may do; every alternative was a second copy
  of thirteen predicates.
  FINAL STRUCTURE (five sections, in order), with the gate for each item:
  (1) primary — Rename (not a browser row), Focus (**gate fixed**: the clicked row's group
  `canFocusMode`; V2 showed it unconditionally before), New session on `<branch>`
  (unchanged worktree gate).
  (2) NEW per-session section, in V1's order — View 1st message (`firstUserMessage`),
  Copy resume (`showSessionCommandCopyActions` + a
  resume-capable agent), Copy attach command (same flag + a stored provider/name pair),
  Copy details (`showSessionDetailsCopyAction`; V1 gates this on "is a concrete row", NOT on
  having an agent, so it is the one parity item a browser tab keeps), Delayed Send
  (`canScheduleDelayedSend` on remote rows, always local), Fork (codex/claude/pi),
  Generate Title (same three + a captured 1st message), Full reload (reload-capable agent
  locally; terminal-kind rows remotely).
  (3) lifecycle — Settle / Un-settle / Wake now / Snooze (unchanged) plus **Close After
  Done** (`canToggleCloseAfterDone` on remote rows). It sits beside Snooze rather than in
  V1's copy section: in the inbox model all three answer "when should this row stop asking
  for attention", and Close After Done is just the answer that ends with the session gone.
  (4) state — Pin/Unpin, **Tag as ▸**, Sleep/Wake. Tag as reuses the INLINE submenu the
  snooze presets introduced instead of V1's second flown-out portal, because a flyout in a
  ~260px sidebar opens over the rows the menu came from. Options come from the shared
  enabled-and-visible resolver with the row's current tag force-included, each carrying its
  colored tag glyph and a checkmark on the one in force; there is no separate "Clear tag"
  row for the same reason V1 has none — re-picking the current marker IS the clear (sends
  `sessionTag: null`).
  (5) destructive — Close, still UNCONDITIONAL. Documented in code as a deliberate
  divergence: V1 hides it behind `showSessionCloseContextMenuAction` (default off), but
  settle/snooze/close are the three verdicts a triage row can get and hiding one behind a
  setting leaves the model incomplete.
  SKIPPED, each with the reason in the file header: Move to New Group, Sleep below, Close
  below (all name a V1 structure — session groups, the project's rendered order below the
  clicked row — that V2 does not render, so their target would have to be invented), Pop Out
  Pane (`popOutPane` is unhandled in gpui's sidebar runtime, so it could only ever be a
  silent no-op), and every bulk "… selected" action (V2 has no multi-select).
  Nine one-line post helpers were added to `sidebar-v2-messages.ts`, each the SAME contract
  variant `sortable-session-card` posts. Generate Title is a sibling of the rename poster
  rather than a flag on it (`postSidebarV2GenerateSessionTitle`), sending the captured 1st
  message as `title` with `shouldGenerateTitle: true` — the host path that already owns
  summarization, the agent-CLI `/name` sync and the "Generating title…" state. Delayed Send
  and View 1st message call the module-level `openAppModal` bridge directly with V1's exact
  payloads, so no new prop and no second modal implementation. `gxserver-runtime.ts`, the
  shared contract and the V2 mount were confirmed to need NO change.
  **STORY-HARNESS BUG FOUND AND FIXED** (`sidebar/sidebar-story-workspace.ts`): the
  Storybook workspace round trip rebuilds sessions from a whitelist, and `firstUserMessage`,
  `sessionPersistenceName`, `sessionPersistenceProvider`, `sessionTag` and the group's
  `canFocusMode` were all being dropped. The first three stories written for this item
  therefore failed with FOUR items missing that were in fact implemented correctly — the
  harness, not the product. Anything gated on those fields was previously untestable in
  Storybook in EITHER sidebar. All five now survive the round trip.
  Verification: typecheck clean; shared 648/648; sidebar 371 passed / 3 failed (the same
  three documented foreign modal-source/command-palette failures, none in V2 files).
  Stories under the HONEST channel-event runner (never `bun run storybook`; `storybook dev`
  on free port 6247 + chrome-headless-shell on 9447, fresh CDP target per story):
  **V2 52/52 green** — including the two the previous entry flagged, which their owner has
  since fixed — and V1 `sidebar-app.interactions` unchanged at **5/20**, matching the
  recorded pre-existing baseline, so the shared harness edit regressed nothing. Four new
  stories: `ShowsV1ParityContextMenuItems` (asserts the ENTIRE 17-label sequence in DOM
  order, which is what proves the new section landed between primary and lifecycle; then
  that a browser tab collapses to exactly `Copy details / Pin / Sleep / Close`; then that
  Focus is absent in a group that cannot zoom while Rename and Unpin are present),
  `HidesCopyActionsWithoutTheirSettings` (both flags default off → the exact 14-label
  sequence, with the agent-capability items untouched), `RunsV1ParityContextMenuCommands`
  (each item posts its own host command — fork, full reload, toggleCloseAfterDone, the
  rename-with-`shouldGenerateTitle`, both command copies, `copySessionDetails` carrying text
  built in the sidebar — plus the Tag as submenu marking Favorite as checked, hiding the
  default-disabled tags, assigning Done, and clearing on a re-pick; the two modal items are
  checked against a stand-in `window.webkit` app-modal host, which the story installs and
  restores, because that bridge THROWS when the host is absent by design), and
  `HidesLocalOnlyActionsOnRemoteRows` (Delayed Send and Close After Done vanish on a
  remote-machine row while Full reload, Copy resume, Rename and Tag as stay — asserting the
  survivors is what proves the remote branch of the resolver ran rather than the section
  having been dropped wholesale). The runner was confirmed honest on these exact stories:
  the first run reported 4/4 FAIL with the real messages, and after the fixes, temporarily
  un-gating Focus made `ShowsV1ParityContextMenuItems` fail again. Real-pointer screenshots
  of the full menu and of the expanded Tag as submenu confirmed the colored tag glyphs and
  the checkmark column fit the inline submenu without layout damage.
- 2026-07-30: **UX BATCH ITEM 9 DONE — grouped mode adopts V1's project UX** (files:
  `sidebar/session-group-section.tsx` (one export), `sidebar/v2/sidebar-v2-group-header.tsx`
  (new), `shared/sidebar-v2-group-order.ts` + `.test.ts` (new), `sidebar/sidebar-app.tsx`,
  `sidebar/v2/sidebar-v2-root.tsx`, `sidebar/v2/sidebar-v2-context-menu.tsx`,
  `sidebar/v2/sidebar-v2-messages.ts`, `sidebar/styles/sidebar-v2.css`, V2 + logical-project
  stories).
  ARCHITECTURE: V2 keeps its own tree. `SessionGroupSection` is NOT mounted inside it —
  that component renders the V1 session list too (pinned reorder, list overflow, per-session
  menus), so borrowing a header would mean mounting the V1 list and then suppressing it.
  Instead the new `SidebarV2ProjectGroupSection` emits V1's header DOM verbatim
  (`section.group[data-project-group][data-collapsed][data-dragging][data-group-drop-position][data-active][data-sidebar-group-id]`
  > `.group-head` > `.group-title-wrap` > `.group-title-row` > collapse button
  `.group-collapse-button.section-titlebar-toggle`, `.group-title-handle` >
  `.group-title-button` > `.group-title.section-titlebar-label`, `.group-title-spacer`,
  `.group-header-actions`), and the grouped container carries
  `group-list workspace-group-list reference-project-group-list`. Because the V2 root already
  mounts inside `.sidebar-reference-layout[data-reference-sidebar="true"]`, the ENTIRE look
  arrives from the existing reference-layout override block in `groups.css` with zero CSS
  moved: full-bleed row with the hover `::before` surface, 16px/550 title with the
  active-project white/650 state, the `::before`/`::after` drop lines, the 0.18 dragging
  opacity, and the hover-revealed right-aligned action cluster. V2 keeps only its own
  `SidebarV2ProjectIcon`, its chevron, and its session count inside that row; sessions inside
  stay V2 cards, browser rows still lead, and the per-project Snoozed/Settled shelves are
  untouched.
  The component owns the `<section>` as well as the header because the section is the
  sortable element while `.group-head` is the drag HANDLE and the drop-bounds element V1's
  pointer resolvers measure — splitting those across two files would split one sortable
  across two owners. `groupSensors` is now EXPORTED from `session-group-section.tsx` and
  reused rather than copied: two of its properties are load-bearing bug fixes a divergent
  copy would silently lose (the Distance constraint beside the Delay one, and the deliberate
  absence of a KeyboardSensor, whose uncommitted drags leave the shared dnd manager non-idle
  and disable EVERY pointer drag in the sidebar). `shouldPreventGroupDragActivation` was
  already exported and is reached through the sensors. Quick renders with the same header
  look but is drag-DISABLED and excluded from the candidate list, rather than taking V1's
  `data-chat-collection` variant (which swaps in a message-circle glyph and hides the
  trailing actions — chrome V2 does not have).
  DND PROVIDER: the single `DragDropProvider` (and both cursor-ghost portals) moved OUT of
  the `isSidebarV2Active ? null :` branch so ONE stable provider wraps both bodies. Two
  providers would mean two dnd managers, two sensor sets and two registries; and mounting it
  outside the version switch means switching sidebars no longer unmounts the manager
  mid-session. The ghost had to come with it: V2 project rows drag with `feedback: "none"`
  exactly as V1's do, so that ghost is the only thing following the pointer.
  REORDER PROJECTION (the real problem): a grouped V2 row is a LOGICAL project that can merge
  several checkouts across several machines, and `syncGroupOrder` rejects a mixed
  local/remote or cross-machine list outright, because each machine owns its own project
  order. So a drop is PROJECTED: `shared/sidebar-v2-group-order.ts` moves the dragged row
  among the logical rows, then expresses that same intent inside each participating machine's
  own list, and the host posts one `syncGroupOrder` per machine that actually changed. Two
  deliberate properties, both unit tested: (a) only the dragged row's members move — every
  other id on a machine keeps its slot, because the logical order is a MERGE of several
  machines' orders and never equals any one machine's saved order, so rewriting a whole list
  would reshuffle projects the user never touched on machines they were not even looking at;
  (b) a row's members move as a BLOCK in that machine's existing relative order, since under
  the default "repository" grouping one row legitimately holds a project plus its worktrees.
  A machine owning no member of the dragged row is never messaged at all. Local rides under a
  `sidebar-v2:local-project-order` sentinel key, not a bare `"local"`, so a remote machine a
  user happened to id "local" cannot clobber this Mac's list. The rendered rows are REPORTED
  UP from `SidebarV2Root` (`onGroupedRowsChange` into a ref) instead of re-derived in
  SidebarApp: a second copy of the cross-machine merge rules would be free to drift from the
  list the pointer is actually over. `resolveGroupDropTargetFromPoint` grew ONE optional
  parameter — the no-op predicate — so grouped V2's drop LINE and its committed reorder
  answer the same question; V1's answer (the physical project-with-worktrees move) is
  unchanged as the default. A separate ref, not a V2 mode on `groupIdsRef`, because several
  V1 paths read that ref during a drag.
  CLOSE PROJECT + **THE REPORTED BUG, CONFIRMED**: the hypothesis was right, and the
  view-model logic is where it is visible. `closeProjectForGroup` removes ONE physical
  project from the presentation, while a grouped row is `logicalGroup.memberGroupIds` — every
  checkout sharing a normalized git origin — and the row survives as long as ANY member is
  still open. Closing this Mac's clone of a repository that is also open on a remote machine
  therefore leaves the row on screen, now titled by the shared repository name and backed only
  by the machine the user was not thinking about; the same happens with a second local clone.
  Close Project therefore fans out over every CLOSABLE member, using V1's exact per-member
  rule (`projectContext` present AND (`canRemoveProject` OR a remote machine context) — close
  eligibility is deliberately not `canRemoveProject` alone, because remote rows park into
  Recent Projects even though remote DELETE stays disabled). The host already routes remote
  group ids. The group menu is also no longer gated on cross-machine merging being possible:
  that gate is exactly why a non-git project had no way out of the grouped list at all. The
  builder now decides which items exist and the mount suppresses the menu when it decided
  none do (Quick: no project, so no menu rather than an empty popover). Reopening is the
  existing Recent Projects flow, version-agnostic, no work.
  CSS: the grouped container drops to `gap: 0` (project-row spacing is owned by
  `.group-head`'s own vertical padding — V1 removed the outside row gaps so right-click,
  hover and drag ownership stay row-local, and an 11px gap here would reintroduce the dead
  strips between headers). Retired with the bespoke header: `.sidebar-v2-group-header`, its
  `:hover`, `.sidebar-v2-group-title`, `.sidebar-v2-group-header-row` and its nested rule —
  nothing else in that file was touched.
  OUT OF SCOPE, deliberately: V1 collection interleaving (grouped V2 renders no collection
  panels; logical merging is V2's answer to "these belong together"), local-vs-remote machine
  SECTIONS (same reason — a merged row already spans machines, so a section per machine would
  have to split rows it exists to join), and keyboard list navigation.
  VERIFICATION (honest CHANNEL-event runner only, never the phase-poll runner; `storybook dev`
  on free port 6411 + chrome-headless-shell on 9511, fresh CDP target per story; `bun run
  storybook` was never used and `shadcn.generated.css` is untouched): **V2 59/59 green** and
  V1 `sidebar-app.interactions` **11/20**, up from its recorded 5/20 baseline — strictly
  better, and the important part is WHICH ones recovered: three V1 session-DRAG stories
  (`drag-to-reorder-within-group`, `drag-across-groups-repeatedly`,
  `drag-across-three-groups-stress`) now pass end to end through the hoisted provider, which
  is the direct evidence the riskiest change in this item did not regress V1 dragging. The 9
  remaining V1 failures are all the pre-existing element-not-found/fixture kind, none about
  the section header. `bunx tsc --noEmit` clean; shared **659/659** (648 baseline + 11 new
  projection tests); sidebar **372 passed / 3 failed** — the same three documented foreign
  failures (watch-video, discover, command-palette).
  Five new stories: `GroupedHeadersUseTheClassicProjectChrome` (asserts the whole DOM
  contract per row — a `SECTION.group[data-project-group="true"]` whose
  `data-sidebar-group-id` equals its V2 id, a `.group-head` that is its DIRECT child, V1's
  wrap/row/collapse-button/title-handle/title-button/title/spacer nesting, V2's icon and count
  inside it, the container's three V1 list classnames, the create control inside
  `.group-header-actions`, exactly one `data-active="true"` row, V2 cards and no
  `.session-frame` inside, and all three retired classnames absent),
  `CollapsesGroupedProjectsThroughTheSharedState` (collapse from V1's title button, expand
  from the collapse control, and the `ghostex-sidebar-ui-collapse-state` localStorage entry
  really carrying the representative id — which is what proves V2 wrote through the shared
  pipeline and not into V2-local state), `ClosesEveryMemberCheckoutOfAGroupedProject` (the
  merged row's menu is exactly `Group across machines / Close Project`, one click posts
  `closeWorkspaceProjectForGroup` for ALL THREE members and nothing more, and a non-git
  project's menu is exactly `Close Project`), `HidesCloseProjectOnRowsThatAreNotProjects`
  (Quick opens no menu at all), and `ReordersGroupedProjectsByDrag` (a real dnd-kit drag
  through V1's sensors: mid-drag the source row carries `data-dragging="true"` and the target
  row carries `data-group-drop-position="after"`, and release posts exactly ONE
  `syncGroupOrder` carrying the local machine's whole list with the merged row's two local
  checkouts moved together — Build Box, which owns no member of that row, is told nothing).
  Two existing logical-project stories were updated for the deliberate behavior change
  (`GroupingOverrideMenuRegroupsTheList` now expects grouping + Close Project;
  `NonGitProjectHasNoGroupingMenu` now expects a menu containing only Close Project, since
  suppressing the whole menu was the bug).
  Every new story was proved NON-VACUOUS: inverting the `reference-project-group-list`
  assertion failed `GroupedHeadersUseTheClassicProjectChrome`; expecting 1 close message
  failed with `expected 3 to be 1`, i.e. the fan-out is genuinely three messages; and
  swapping the expected reordered list failed `ReordersGroupedProjectsByDrag`.
  **HARNESS FINDINGS** worth recording. (1) The Storybook stand-in host cannot echo a
  per-machine reorder back: `syncGroupOrderInWorkspace` only accepts a list covering every
  group in the snapshot, while a per-machine order deliberately covers one machine's groups.
  That is the stand-in being stricter than the real host — gxserver's
  `syncWorkspaceGroupOrder` is built for exactly the partial shape (local ids normalize into
  the workspace project order, remote ids into that machine's order overlay) — so the drag
  story asserts messages and drag chrome rather than a re-render. The shared reducer was left
  alone on purpose; it is production code the deprecated macOS app also runs. (2) A plain CDP
  `Input.dispatchMouseEvent` drag does NOT activate dnd-kit's PointerSensor in this
  environment; a HEAD control worktree with its own Storybook produced the identical
  non-activation, so this is a harness limitation and not a regression. Drag coverage
  therefore goes through the story helpers' `pointerDown`/`pointerMove`/`pointerUp` path,
  which does activate. (3) Concurrent story edits cause Vite HMR to abort in-flight renders,
  which the channel runner correctly reports as errored; those re-pass individually, and a
  static `storybook build` bundle can lag a concurrent agent's newest source (one such story
  failed on the bundle and passed against the live tree).
- 2026-07-30: **UX BATCH ITEM 10 DONE — the V2 context menu IS the classic menu** (files:
  `sidebar/v2/sidebar-v2-context-menu.tsx`, `sidebar/v2/sidebar-v2-root.tsx`,
  `sidebar/sidebar-context-menu-portal.tsx`, `sidebar/styles/session-overlays.css`,
  `sidebar/styles/sidebar-v2.css`, `sidebar/v2/sidebar-v2.interactions.stories.tsx`).
  APPROACH: reuse, not restyling. The renderer was already inside V1's
  `SidebarContextMenuPortal`, so the gap was never the portal — it was everything the portal
  does NOT own. Measured side by side over CDP (V1 `sidebar-interactions--session-card-actions`
  vs the V2 parity stories, both in a 300px-wide window), the two menus already agreed on
  padding (6px), border, radius (0), background, shadow, item height (32.38px), item padding
  (8px 10px) and font — and disagreed on FIVE things, all now closed:
  (1) WIDTH. V1's session menu is a deterministic `min(178px, 100vw - 24px)` from its own
  `.sidebar-session-context-menu` class; V2 had only `min-width: 156px`, so each row's longest
  label decided the width (measured 156px on one row, 182.67px on another). V2 now carries
  V1's class, so the width is V1's by construction rather than by copy.
  (2) SUBMENU PRESENTATION. V2 expanded Snooze/Tag as INLINE with a 28px indent; V1 opens a
  SECOND portal panel. V2 now renders `SidebarV2ContextSubmenuPanel`: its own `createPortal`
  into the body, `session-context-menu` chrome, V1's 204px submenu width, the submenu
  z-index, anchored to the parent ROW's left edge and 4px below it (V1's own anchor), and
  clamped from its RENDERED rect — not an item-count height estimate — through a newly
  exported `getClampedSidebarContextMenuCoordinate` / `SIDEBAR_CONTEXT_MENU_VIEWPORT_MARGIN_PX`
  so parent and flyout cannot disagree about the webview edges. The panel width/stacking rule
  is now ONE rule shared with V1's `.session-tag-submenu` instead of a second copy (the
  z-index moved from V1's inline style into that rule, same value).
  (3) SUBMENU AFFORDANCE. V1 draws `IconChevronRight` in `.session-context-menu-trailing-icon`
  on a submenu parent; V2 drew nothing, so Snooze and Tag as looked like ordinary commands.
  (4) SECTION STRUCTURE. V1 wraps each section in a `Fragment`, so the divider and the
  section are siblings in the menu's own 2px grid; V2 wrapped each in a `<div>`, which made
  every inter-section gap 2px tighter than V1's. Now a Fragment, like V1.
  (5) TAG SUBMENU GROUPING. V1's tag flyout keeps the Priority/Progress/Type blocks with a
  divider between them (and no heading text); V2 flattened the resolver's sections into one
  run of eight markers. Submenu items now carry an optional `sectionKey` — set from the
  shared resolver's own sections, order and set untouched — and the panel draws consecutive
  same-key items inside V1's `.session-tag-menu-section`, which already owns that divider.
  The GROUP (grouping-mode) menu gets all of the above through the same renderer, and its
  width is now V1's PROJECT-menu width, not its session-menu width: the classic sidebar sets
  178px for a session row and 196px for a project row (`CONTEXT_MENU_WIDTH_PX` in
  `session-group-section`), so the menu takes a `variant` and the group mount names
  `projectGroup`. V1's project menu could not be opened in any Storybook fixture (the story
  groups carry no `projectContext`), so that 196px is taken from V1's source constant rather
  than from a screenshot.
  ONE deliberate refinement over V1, forced by V2's longer labels: item labels are now spans
  with a bounded grid column (`minmax(0, 1fr)`), `min-width: 0` on the row, and an ellipsis.
  V1 leaves labels as bare nowrap text nodes, so a label wider than the fixed menu spills the
  box and turns the menu into a horizontal scroller — and, for a submenu parent, pushes the
  chevron out of view entirely. That is exactly what "Group across machines" did (measured:
  row 195.84px inside a 176px content box, chevron right edge 232.84 vs the menu's 218), and
  what "New session on <branch>" would do. The ellipsis costs nothing on labels that fit, so
  every classic row still reads exactly as before; the two that do not fit now truncate
  visibly instead of losing their affordance.
  UNCHANGED ON PURPOSE: the item set, gating, ORDER and section grouping from item 7 (V2's
  five sections are not V1's four — that was item 7's documented model, and the parity stories
  assert the exact 17-label sequence), pointer-anchored opening with the portal's viewport
  clamp, `data-menu-open` row pinning, Escape and click-outside dismissal, and every host
  message. Nothing was added to the builder except the `sectionKey` pass-through.
  CSS CLEANUP: the inline-expansion indent (`.sidebar-v2-session-context-menu
  .sidebar-v2-context-submenu-item { padding-left: 28px }`) is deleted, and the two submenu
  rules that were scoped to the parent menu (`-item` flex row, `-check`) are retargeted to the
  panel, since the items no longer live inside the parent menu.
  Verification: typecheck clean; shared 659/659; sidebar vitest 372 passed / 3 failed — the
  same three documented foreign failures (watch-video, discover, command-palette), none in V2
  files. Stories under the HONEST channel-event runner (`storybook dev` on free port 6357 +
  chrome-headless-shell on 9471, fresh CDP target per story, `bun run storybook` never used,
  `shadcn.generated.css` untouched): **V2 59/59 green**, including the concurrent grouped-mode
  work landing alongside this change. Two new stories: `MatchesTheClassicContextMenuChrome`
  (the menu carries `session-context-menu` + `sidebar-session-context-menu`, is a body-level
  portal with exactly one shared backdrop, measures 178px/6px padding/8px-10px items, its
  direct children are only sections and dividers — the Fragment contract — both submenu
  parents carry the trailing chevron, Tag as opens a panel that is NOT inside the menu, is
  stacked above it, is placed at the anchor-or-clamp position the shipped code computes and
  stays inside the 12px margin, leaves the parent menu open with `aria-expanded="true"`,
  keeps more than one `.session-tag-menu-section` and 8 items at un-indented 8px 10px padding,
  and Escape dismisses panel and menu together) and
  `GroupMenuMatchesTheClassicContextMenuChrome` (196px, no session class, one backdrop, the
  chevron's right edge inside the menu box, the grouping flyout as one radio block of three,
  and Close Project still a top-level item). Both were proved NON-VACUOUS: removing
  `sidebar-session-context-menu` failed both with `expected false to be true`, and dropping
  the `sectionKey` pass-through failed the row story with `expected 1 to be greater than 1`.
  V1's `sidebar-app.interactions` set measures **11/20**, up from the 5/20 recorded earlier
  today (concurrent V1 fixes, not this change) — and the one that matters here,
  `session-card-actions`, which drives V1's own session menu including its Tag as submenu,
  PASSES, so the shared portal/CSS edits regressed nothing in the classic sidebar. Screenshots
  (2x, 300px window) captured before/after for the row menu, its Tag as flyout, and the group
  menu with its grouping flyout, against the V1 originals.
  CONCURRENCY: item 9's grouped-mode work was landing in the same three files throughout. All
  edits here were re-read-then-surgical; the only overlap was the group-menu mount (its
  `Close Project` item and every group section flow through the item model untouched — only
  the renderer and one new prop changed) and the end of the interactions story file, where the
  new stories were appended after theirs. A human commit at 06:28 ("Inbox Sidebar") swept the
  whole working tree, including the first half of this change, so the earlier portal/CSS hunks
  are already committed and the remaining four files are the post-commit half.

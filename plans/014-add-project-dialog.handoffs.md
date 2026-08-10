# 014 — Add Project dialog: implementation handoffs

Each implementing agent APPENDS a section here. Never rewrite earlier sections.

## Part A — gxserver foundation (endpoints, protocol, CLI verbs, TS contracts)

Date: 2026-07-30.

### Files changed

- `gxserver-rs/src/domain.rs` — `add_project_path` reads `createIfMissing`; new `normalize_project_root_path(value, field, create_if_missing)`; `normalize_existing_directory_path` now delegates to it. New test `add_project_path_creates_workspace_root_when_create_if_missing_is_requested`.
- `gxserver-rs/src/server.rs` — browse parity fixes; new `/api/discoverSourceControl` + `/api/lookupRepository` route arm; new `handle_source_control_http` / `source_control_error_response`; 3 new tests.
- `gxserver-rs/src/source_control.rs` — NEW module: provider discovery + repository lookup (10 unit tests).
- `gxserver-rs/src/lib.rs` — `pub mod source_control;`.
- `gxserver-rs/src/protocol.rs` — remote allowlist gains `/api/discoverSourceControl`, `/api/lookupRepository`.
- `gxserver-rs/src/repository_clone.rs` — `remoteUrl` alias, `destinationPath` shape, `destinationBlocked`, parent-dir creation in the job; 3 new tests.
- `gxserver-rs/src/ghostex_cli/mod.rs` — 4 new verbs registered + routed (`NAMES` array length 108 → 112).
- `gxserver-rs/src/ghostex_cli/actions.rs` — new `Parser::{BrowseDirectories, LookupRepository, CloneRepository}`, parsers, action routes, `clone_repository_and_wait`, `--create-if-missing` in `parse_project_path`.
- `gxserver-rs/src/ghostex_cli/usage.rs` — help rows.
- `shared/gxserver-protocol.ts` — 2 endpoint paths, `GxserverAddProjectPathParams`, 10 source-control types, clone params/result changes.
- `native/sidebar/gxserver-client.ts` — 6 new typed methods + 2 endpoint-description entries; `GxserverAddProjectPathParams` now imported from `shared/` instead of declared locally.

### Exact new names

Endpoints (both **remote-allowed**):

- `POST /api/discoverSourceControl` — params `{ cwd?: string }` → `{ discovery: { checkedAt: string, providers: Array<ProviderDiscovery> } }`
- `POST /api/lookupRepository` — params `{ provider, repository, cwd? }` → `{ repository: { nameWithOwner, provider, sshUrl, url } }`

`ProviderDiscovery` JSON (optional keys are OMITTED, never null):

```json
{
  "auth": { "account?": "…", "detail?": "…", "host?": "…", "status": "authenticated|unauthenticated|unknown" },
  "detail?": "…",
  "executable?": "gh",
  "installHint": "…",
  "label": "GitHub",
  "provider": "github|gitlab|bitbucket|azure-devops",
  "status": "available|missing|unsupported",
  "version?": "gh version 2.96.0 (2026-07-02)"
}
```

Array order is always `github, gitlab, bitbucket, azure-devops`. The key is
`provider` rather than `kind`. `status: "unsupported"` means gxserver has no implementation —
Bitbucket and Azure DevOps always report it and are never probed. Readiness
mapping per spec §3.3 still works unchanged (`status !== "available"` → not
ready, hint = `installHint`).

New params / result fields:

- `/api/addProjectPath` params: `createIfMissing?: boolean`.
- `/api/previewRepositoryClone` + `/api/startRepositoryClone` params: `remoteUrl?: string` (alias of `repositoryInput`), `destinationPath?: string`.
- Clone preview result: `destinationBlocked: boolean` (new; this is what `start` enforces).

TypeScript types added in `shared/gxserver-protocol.ts`:

`GxserverAddProjectPathParams`, `GxserverSourceControlProviderKind`,
`GxserverSourceControlDiscoveryStatus`, `GxserverSourceControlAuthStatus`,
`GxserverSourceControlProviderAuth`, `GxserverSourceControlProviderDiscovery`,
`GxserverSourceControlDiscovery`, `GxserverDiscoverSourceControlParams`,
`GxserverDiscoverSourceControlResult`, `GxserverLookupRepositoryParams`,
`GxserverSourceControlRepositoryInfo`, `GxserverLookupRepositoryResult`.

Typed-client methods on `createNativeSidebarGxserverClient(...)`:

`browseProjectDirectories(params)`, `discoverSourceControl(params?)`,
`lookupRepository(params)`, `previewRepositoryClone(params)`,
`startRepositoryClone(params)`, `readRepositoryCloneJob({ jobId })`,
`cancelRepositoryCloneJob({ jobId })`. `addProjectPath` / `addProjectPathSync`
now accept `createIfMissing`.

New CLI verbs (all print JSON; all support `--server` / `--token-stdin`):

- `ghostex browse-directories <partialPath> [--cwd dir] [--limit n] --json`
- `ghostex discover-source-control --json`
- `ghostex lookup-repository <github|gitlab> <owner/repo> [--cwd dir] --json`
- `ghostex clone-repository <remoteUrl> <destinationPath> [--branch-name b] [--shallow-clone] [--clone-main-only] [--wait-timeout-ms n] --json`
- `ghostex add-project <path> [--name name] [--create-if-missing] --json`

CLI action names (internal, for `send_gxserver_cli_action`): `browseDirectories`,
`discoverSourceControl`, `lookupRepository`, `cloneRepository`.

`clone-repository` is a blocking wrapper: start → poll `/api/readRepositoryCloneJob`
every 500 ms → print the final `{ job }` record. Exit code 1 on `failed`/`canceled`
or on wait timeout (default 900 000 ms; result then carries `waitTimedOut: true`
and the job is left running server-side — the wait NEVER cancels the clone).

### Decisions taken

1. **`createIfMissing` keeps the existing error strings.** Spec §6.2 wants
   "Workspace root does not exist: …" / "… is not a directory: …", but gxserver's
   whole project API already answers `path does not exist: <p>` /
   `path is not a directory: <p>` and existing tests pin those. Only the new
   failure mode gets the wording: `Failed to create workspace root: <p>`.
   Flag-absent behavior is byte-identical to before.
2. **Browse parity gaps found and fixed** (spec §6.1): (a) a permission-denied
   `readdir` used to return `notFound`; it now returns an empty entry list with
   the resolved `parentPath`, matching the silent-empty contract, while
   every other read failure still errors; (b) entries were byte-sorted, which
   filed all capitalized folders before lowercase ones; they now sort
   `localeCompare`-style (case-insensitive, raw name as tiebreak). Everything
   else already matched: `~`/bare-`~` expansion, trailing-separator → prefix "",
   case-insensitive prefix, hidden-only-when-`.`-or-separator, `parentPath` =
   server-resolved absolute, dirs only, `cwd` required for explicit relatives.
3. **Clone gap found and closed.** `cloneRepository` takes
   `{ remoteUrl, destinationPath }`; gxserver only took `parentPath` +
   `destinationFolderName` with an existing parent and refused ANY existing
   destination. Rather than making the dialog split paths client-side (it can't
   create parents anyway), `destinationPath` is now a first-class input.
   Semantics per spec §6.5: expand+resolve, exists&non-empty →
   "Destination path already exists and is not empty.", exists&not-a-dir →
   "Destination path already exists and is not a directory.", existing EMPTY dir
   is allowed, missing parents are `mkdir -p`'d by the job right before git runs.
   The legacy `parentPath` shape is untouched (Clone Repository modal still gets
   its "A directory already exists at … Choose a new folder name" warning), which
   is why `destinationBlocked` exists as a separate field from `destinationExists`.
4. **Bitbucket / Azure DevOps report `status: "unsupported"`, not `"missing"`.**
   Reporting "missing" would tell the user to install a CLI that still would not
   make the row work. Their `installHint` points at the Git URL source instead.
   No `az` probe is run at all.
5. **Token safety.** `gh auth status` prints `- Token: gho_…`; every `token:` /
   `token scopes:` line is stripped before any auth `detail` leaves
   `source_control.rs` (unit-tested). Only the first surviving line is forwarded.
6. **Lookup input hardening.** `repository` is trimmed, ≤512 chars, must not
   contain whitespace or NULs, and must not start with `-` (it is a positional
   argument to `gh`/`glab`).
7. **Probe cwd** defaults to the daemon's own home dir; an explicit `cwd` must be
   an existing absolute (or `~/`) directory.
8. **Timeouts**: 5 s per discovery probe (version and auth are separate probes,
   per spec §6.3), 15 s for a lookup. Clone keeps its existing 30-minute job
   timeout (gxserver clones large repos as a background job
   with cancel, so shortening it would be a regression).
9. **glab lookup** uses `glab api projects/<percent-encoded path>` and maps
   `path_with_namespace`/`web_url`/`ssh_url_to_repo`.
10. GitLab auth parsing uses a simplified per-host block parser:
    account comes from the first `Logged in to … as <account>` (or
    `account: <x>`) line anywhere in the output, host from the first hostname-ish
    sanitized line. The readiness decision (account present → authenticated) is
    identical.

### Verification run

- `cargo check --all-targets` (gxserver-rs): clean.
- `cargo clippy --all-targets` (gxserver-rs): no new warnings in any touched file.
- `cargo test` (gxserver-rs): **614 passed, 0 failed** (13 of them new).
- `bun run typecheck` (root tsconfig: shared/ + sidebar/ + native/sidebar/): pass.
- `bun run web:typecheck`: pass.
- Live smoke against an **isolated** daemon (scratch `HOME`, port 58791, never
  the user's 58744 daemon; torn down + scratch dir deleted afterwards):
  - `discoverSourceControl` → gh `available` + `authenticated` (account/host,
    no token text) with a real gh config; with an empty HOME the same gh reported
    `unauthenticated` with the `gh auth login` hint; glab `missing`; bitbucket
    and azure-devops `unsupported`.
  - `lookupRepository github octocat/Hello-World` → correct nameWithOwner/url/sshUrl.
    Unknown repo → `notFound` with the GraphQL message. gitlab with no `glab` →
    `dependencyUnavailable`.
  - `addProjectPath` missing path → `notFound`; same path with
    `createIfMissing: true` → directory created, project registered, name from
    the leaf; repeat call idempotent.
  - `browseProjectDirectories` on `~/` → dirs only, hidden shown for a trailing
    separator, server-resolved absolute `parentPath`.
  - Clone `{ remoteUrl, destinationPath }` into a NON-EXISTENT nested path →
    parents created, git clone succeeded, project registered at the destination,
    job `completed` with `project`/`projectPath`. Re-clone into the now-populated
    destination → `Destination path already exists and is not empty.` Empty
    existing directory → `destinationBlocked: false`.
  - All five CLI verbs exercised end to end, including `clone-repository`
    (exit 0 on success, exit 1 + `ok:false` on the occupied destination) and
    `add-project --create-if-missing`.

### Known gaps / notes for later parts

- **No `sidebar/` or contract-message work was done.** `shared/session-grid-contract-sidebar.ts`
  request/result message variants, `sidebar/v2/sidebar-v2-messages.ts` posters, and the
  `gpui/sidebar/gxserver-runtime.ts` / `ghostex-web/src/sidebar-runtime/sidebar-runtime.ts`
  handlers are Parts B/C/D. The typed client methods above are the pieces those
  handlers should call.
- Clone progress is still coarse: the job reports `running` → `completed|failed|canceled`
  with a `message`, no percentage.
- `/api/previewRepositoryClone` is not required by the dialog — `startRepositoryClone`
  runs the same validation and returns the same preview inside the job — but it is
  available if a step wants to pre-flight a destination without starting anything.
- `GxserverRepositoryClonePreviewParams.repositoryInput` became optional (because
  `remoteUrl` is now an alternative). The only existing consumer,
  `sidebar/add-repository-modal.tsx`, always sends `repositoryInput` and reads
  `destinationExists`, both unchanged.
- The CLI's local target is still hardcoded to port 58744; testing verbs against a
  non-default daemon requires `--server http://127.0.0.1:<port> --token-stdin`.
- `sidebar/styles/shadcn.generated.css` shows as modified in this worktree; that
  hunk is NOT from Part A and was left untouched.

---

## Part B — shared React dialog + Storybook

### Files changed (all new; nothing outside `sidebar/add-project-modal/` was touched)

- `sidebar/add-project-modal/types.ts` — callback/result contracts.
- `sidebar/add-project-modal/add-project-modal-logic.ts` — pure copy/ordering/readiness logic.
- `sidebar/add-project-modal/add-project-modal.tsx` — the dialog.
- `sidebar/add-project-modal/add-project-modal-mocks.ts` — in-memory fixture filesystem + scripted callbacks.
- `sidebar/add-project-modal/add-project-modal.story-harness.tsx` — shared Storybook harness (not a story file).
- `sidebar/add-project-modal/add-project-modal.stories.tsx` — 6 visual stories.
- `sidebar/add-project-modal/add-project-modal.interactions.stories.tsx` — 10 play-function stories.

`sidebar/styles/shadcn.generated.css` is regenerated by `bun run build:sidebar-css`
(which `bun run storybook` runs); the 1-line diff there is that generated artifact,
not hand-edited. No CSS file was added: the dialog is Tailwind-only on top of the
house `Dialog`/`CommandDialog`/`InputGroup`/`Button` primitives.

### Exact names Part C/D must use

Component: `AddProjectModal` (default-less named export) from `sidebar/add-project-modal/add-project-modal`.

Props (`AddProjectModalProps`): `activeProjectCwd?`, `cloneJobPollIntervalMs?` (default 900),
`initialMachineId?`, `isOpen`, `onClose`, `onOpenSourceControlSettings?`, `onProjectAdded?`,
`platform?`, `slowOperationNoticeMs?` (default 8000), plus the 8 callbacks:

- `listMachineOptions(): Promise<readonly AddProjectMachineOption[]>`
- `browse({ machineId, partialPath, cwd? }): Promise<AddProjectBrowseResult | null>` → `{ parentPath, entries: [{ name, fullPath }] }`
- `addProject({ createIfMissing, machineId, path }): Promise<AddProjectAddResult>` → `{ machineId, path, projectId?, alreadyExists? }`
- `discoverSourceControl({ machineId }): Promise<AddProjectSourceControlDiscovery | null>` → `{ providers: [...] }`
- `lookupRepository({ machineId, provider, repository }): Promise<AddProjectRepositoryInfo>` → `{ provider, nameWithOwner, url, sshUrl }`
- `startClone({ destinationPath, machineId, remoteUrl }): Promise<{ jobId }>`
- `readCloneJob({ jobId, machineId }): Promise<AddProjectCloneJob>`
- `cancelCloneJob?({ jobId, machineId }): Promise<void>`

Types exported from `sidebar/add-project-modal/types`: `AddProjectMachineOption`
(`{ machineId, label, description?, platform?, addProjectBaseDirectory? }`),
`AddProjectBrowseInput/Entry/Result`, `AddProjectAddInput/Result`,
`AddProjectProviderId` (`"github"|"gitlab"|"bitbucket"|"azure-devops"`),
`AddProjectSourceId` (those + `"url"`), `AddProjectProviderAuthStatus`,
`AddProjectProviderStatus` (`"available"|"error"|"missing"|"unsupported"`),
`AddProjectProviderDiscovery`, `AddProjectSourceControlDiscovery`,
`AddProjectSourceReadiness`, `AddProjectRepositoryLookupInput`,
`AddProjectRepositoryInfo`, `AddProjectCloneStartInput`, `AddProjectCloneJobHandle`,
`AddProjectCloneJobInput`, `AddProjectCloneJobState`
(`"running"|"completed"|"failed"|"canceled"` — Part A's spelling), `AddProjectCloneJob`
(`{ jobId, state, error?, message?, projectPath? }` — Part A's `GxserverRepositoryCloneJobStatus`
field names, so a host can forward the job record unchanged),
`AddProjectModalCallbacks`, `AddProjectModalProps`.

Logic helpers (`add-project-modal-logic.ts`): `ADD_PROJECT_PROVIDER_SOURCES`,
`ADD_PROJECT_SOURCES`, `ADD_PROJECT_DEFAULT_BROWSE_PATH`, `addProjectSourceLabel`,
`addProjectSourcePathHint`, `addProjectSourceRowTitle`, `addProjectSourceRowDescription`,
`buildAddProjectSourceReadiness`, `sortAddProjectProviderSources`, `orderedAddProjectSources`,
`addProjectRepositoryPlaceholder`, `addProjectRepositoryActionLabel`, `addProjectPathPlaceholder`,
`addProjectInitialBrowseQuery`, `addProjectSubmitActionLabel`, `addProjectEmptyStateMessage`,
`isPrimaryModifierPlatform`, `addProjectModifierLabel`, `matchesAddProjectFilter`.

`data-add-project-field` values (test/automation hooks): `pathInput`, `submit`,
`repositoryAction`, `back`, `list`, `emptyState`, `error`, `notice`, `cloneCancel`,
`repositoryCard`, `footer`, `machineLabel`, `machineOption`, `sourceOption`,
`setupRequired`, `directoryEntry`, `directoryUp`, `discoveryPending`.
Extra attributes: `data-add-project-modal` (dialog body root),
`data-add-project-machine-id`, `data-add-project-source` (`local`, `url`, provider ids),
`data-add-project-path` (directory rows), `data-add-project-value` (highlight key).

Storybook: titles `Modals/Add Project` and `Modals/Add Project Interactions`;
harness exports `AddProjectStoryHarness`, `getAddProjectStoryMocks`,
`findAddProjectStoryCall`; mock factory `createAddProjectStoryMocks(options)` with
`ADD_PROJECT_STORY_LOCAL_MACHINE`, `ADD_PROJECT_STORY_REMOTE_MACHINE`,
`ADD_PROJECT_STORY_READY_PROVIDERS`, `browseStoryTree`.

### Decisions taken

1. **cmdk is NOT used for this dialog.** cmdk re-runs `selectFirstItem()` on every
   search-state change (`node_modules/cmdk/dist/index.mjs`), which is exactly the
   Enter trap the plan blames for `remote-project-picker-modal.tsx`: a controlled
   `value` cannot suppress it because the auto-select still fires `onValueChange`.
   Spec §2.7/gotcha 7 makes `autoHighlight={false}` load-bearing, so the dialog owns
   its highlight state and ArrowUp/ArrowDown/Enter/Backspace handling. It still uses
   the house shell (`CommandDialog`) and input (`InputGroup`/`InputGroupInput`/`Button`)
   and the shared `sidebar/remote-project-picker/remote-project-paths.ts` +
   `remote-command-palette-logic.ts` helpers (`filterBrowseEntries` reused verbatim).
2. **Submit/action buttons live in the input's `inline-end` addon**, not absolutely
   positioned over it, so they never overlap the house command-input clear button.
3. **Highlighting `..` counts as a highlighted browse item** (the
   `highlightedEntry` ignores the up-row, which makes Enter submit the typed path
   while `..` is visibly selected). Ghostex treats it as a row: Enter walks up,
   mod+Enter still submits.
4. **Popping the last view closes the dialog is NOT done** — `canPopView` requires
   `viewStack.length > 1`, so Back/Backspace/clear only pop to a real previous step;
   Esc is the only way out of the first step,
   which Ghostex does not have here).
5. **Machine step gating** matches spec §1.5: `initialMachineId` (if it matches an
   option) or exactly one option → straight to Sources; >1 → Machines step; 0 →
   persistent error "No machine is available.".
6. **Errors are one persistent inline region** (`data-add-project-field="error"`,
   `role="alert"`), cleared only when the user edits the query or changes step —
   never a transient list line. A pending call longer than `slowOperationNoticeMs`
   adds a `notice` row ("Still working. The machine may be reconnecting.") with a
   Cancel-clone link when a clone job is in flight. The hard timeout stays with the
   host (plan Part C: 60s).
7. **`onOpenSourceControlSettings` does not close the dialog** (the palette
   closes and navigates); the host decides, so gpui can open Settings in its own
   window without losing the add-project flow.
8. **Dialog position/size**: `CommandDialog` + `max-w-xl sm:max-w-xl` (547px measured),
   keeping the house command-palette `top-1/3` anchor.
   The body root carries `w-full min-w-0` because `DialogContent` is a grid and a long
   row description would otherwise push the body past the popup's clipped edge.
9. **Nothing is transport-aware.** `machineId` is the only routing token the dialog
   sees; no hosts, tokens, URLs, or SSH details appear in props or logs (the dialog
   logs nothing at all).

### Verification

- `bun run typecheck` (repo-wide `tsc --noEmit`) — clean, exit 0.
- `bun run storybook` (port 6006, runs `build:sidebar-css` first) — starts clean.
- All 16 stories driven in real Chrome over CDP (isolated profile, port 9333),
  reading Storybook's `storyFinished` channel event per story. Result:
  16/16 `status: "success"`, zero console errors / uncaught exceptions.
  - Interactions (10): `BrowsesDescendsAndGoesUp`, `EnterSubmitsTypedPath`,
    `ModifierEnterOverridesHighlight`, `SubmitLabelFlipsToCreateAndAdd`,
    `MachineStepPicksRemoteMachine`, `SourceReadinessOrdersAndDisablesProviders`,
    `ClonesFromGitUrlAndAddsProject`, `LookupFailureStaysOnRepositoryStep`,
    `AddFailureShowsPersistentError`, `BackNavigationPopsSteps`.
  - Visual (6): `Sources`, `MachineStep`, `PreselectedRemoteMachine`,
    `NoProvidersReady`, `SlowMachine`, `AddAlwaysFails`.
- Screenshots reviewed for the Sources step, the local-browse list + persistent error
  region, and the clone-destination step (repository card, "Select where to clone",
  "Create & Clone", footer hints). Fixed two layout bugs found that way: the dialog
  was inheriting `sm:max-w-md` and the Setup Required buttons were clipped.

### Known gaps

- No host wiring: the dialog is not mounted anywhere yet (Part C: `GpuiAppModalKind::AddProject`
  + `modal-host.tsx`; Part D: `add-project-modal-host.tsx`). No entry point calls it.
- `readCloneJob` polling has no cap: it polls until the job leaves `running`. The host
  must own the abort (dismissing the dialog unmounts it and abandons the poll, and
  `cancelCloneJob` is optional).
- Duplicate-project handling is server-side only: the dialog shows whatever
  `addProject` rejects with. Existing projects open silently instead; if
  Ghostex wants that, the host should resolve it inside `addProject` and answer with
  `alreadyExists: true` (the field exists on `AddProjectAddResult` but the dialog
  currently only closes on success).
- No unit test file was added for `add-project-modal-logic.ts`; behavior is covered
  through the play-function stories instead.
- `activeProjectCwd` is threaded through browse (`cwd`) and `./`/`../` resolution, but
  no host supplies it yet, so relative-path queries currently show
  "Relative paths require an active project." everywhere.

---

## Part D — ghostex-web integration

Date: 2026-07-30. Scope: `ghostex-web/**` only. No file outside `ghostex-web/src/`
was touched (verified with `git diff --stat ghostex-web/` + `git status`).

### Files changed

- `ghostex-web/src/app/action-events.ts` — new exported interface
  `OpenAddProjectModalDetail { machineId?: string }` and a new
  `WindowEventMap` entry `"ghostex-web:openAddProjectModal"`.
- `ghostex-web/src/app/app-modal-host-shim.ts` — the app-modal shim now accepts
  the add-project open message; new helpers `isAddProjectModal(modal)` and
  `openAddProjectModal(message)`.
- `ghostex-web/src/app/add-project-modal-host.tsx` — **NEW**. Exports
  `AddProjectModalHost` (no props). Renders the shared `AddProjectModal` in page
  and fulfils all 8 callbacks with `rpcForMachine`.
- `ghostex-web/src/routes/__root.tsx` — mounts `<AddProjectModalHost />` next to
  `<RecentProjectsModalHost />` inside `GhostexWebShell`.
- `ghostex-web/src/sidebar-runtime/sidebar-runtime.ts` — new
  `case "pickWorkspaceFolder"` in `handleSidebarMessage` (previously fell through
  to `nativeOnlyNoOp`), dispatching the open event.

### Exact new names

- Custom event: `"ghostex-web:openAddProjectModal"`, detail type
  `OpenAddProjectModalDetail` (`{ machineId?: string }`). Closing still reuses the
  existing `"ghostex-web:closeAppModal"` event.
- Component: `AddProjectModalHost` from `ghostex-web/src/app/add-project-modal-host.tsx`.
- Constant: `ADD_PROJECT_TIMEOUT_MS = 60_000`; helper `withTimeout(operation, timeoutMessage)`.
- Helpers: `listConnectedMachineOptions()`, `resolveActiveProjectCwd()`.
- Accepted app-modal kinds on web: `"addProject"` (machine key `machineId`) and
  `"remoteProjectPicker"` (machine key `remoteMachineId`).

Endpoint mapping used by the callbacks (all through
`rpcForMachine(machineId, path, params)`):

| dialog callback | endpoint | params sent | value returned |
| --- | --- | --- | --- |
| `browse` | `/api/browseProjectDirectories` | `partialPath`, `cwd?` | `{ entries, parentPath }` |
| `addProject` | `/api/addProjectPath` | `path`, `createIfMissing` | `{ machineId, path: project.path, projectId }` |
| `discoverSourceControl` | `/api/discoverSourceControl` | — | `{ providers: discovery.providers }` |
| `lookupRepository` | `/api/lookupRepository` | `provider`, `repository` | `result.repository` |
| `startClone` | `/api/startRepositoryClone` | `remoteUrl`, `destinationPath` | `{ jobId: job.jobId }` |
| `readCloneJob` | `/api/readRepositoryCloneJob` | `jobId` | `job` (forwarded unchanged) |
| `cancelCloneJob` | `/api/cancelRepositoryCloneJob` | `jobId` | `void` |
| `listMachineOptions` | — | — | connection registry, see below |

### Decisions taken

1. **`pickWorkspaceFolder` is the web add-project entry point, not a no-op.**
   The sidebar's local Projects-header "Add project" button and the command
   palette's `addProject` command both post `{ type: "pickWorkspaceFolder" }`
   (`sidebar/sidebar-app.tsx:5294`, `sidebar/command-palette.tsx:316`). Native
   answers with an OS folder picker, which the browser has no equivalent for, so
   web answers the same intent with the shared dialog. This makes both
   affordances work on web TODAY, independently of Part C.
2. **The shim resolves `remoteProjectPicker` to the same dialog.** That kind is
   the remote machine header's Add-project entry (`sidebar/sidebar-app.tsx:5556-5563`);
   it carries the identical intent under an older payload name, so on web it
   opens the new dialog preselected to `remoteMachineId`. When Part C rewires
   that call site to `openAppModal({ modal: "addProject", machineId })` the
   `machineId` branch takes over with no further web change.
3. **The shim reads the open message structurally**, not through
   `OpenAppModalMessage`, because `AppModalKind` does not yet contain
   `"addProject"` (that is Part C's edit to `sidebar/app-modal-host-bridge.ts`).
   Web therefore does not block on Part C and needs no change when it lands.
4. **Machine options = connected connections only.** `listConnectedMachineOptions()`
   maps `getConnectionStates()` filtered to `status === "connected"`, local first
   then remotes by label. A `connecting` machine cannot answer a browse call, so
   offering it would only produce a failed round trip inside the dialog. Only
   `machineId` + `label` are passed; `description` is deliberately omitted so no
   base URL / host reaches the dialog.
5. **Host-owned 60s timeout** (plan Part C item 2 / Part B decision 6) is applied
   to `addProject` and `startClone` — the two calls that mutate state and were the
   subject of the original remote-add bug. Browse/lookup keep the dialog's own
   8s "still working" notice, which is the designed UX for a slow read.
6. **A registered project without a `path` is an error, not a fallback.**
   `GxserverProjectDomainState.path` is optional only because quick/chat projects
   have no workspace root; `/api/addProjectPath` always answers with one. The host
   throws "gxserver registered the project without a workspace path." instead of
   substituting the unnormalized input path.
7. **`activeProjectCwd` is resolved at open time** from
   `getActiveSidebarProject()` + the machine's presentation snapshot `path`, which
   closes Part B's "no host supplies activeProjectCwd" gap on web with no extra
   round trip.
8. **Nothing is optimistic.** The added/cloned project reaches the sidebar as an
   ordinary presentation delta (verified live).

### Verification run

- `bun run web:typecheck` — clean, exit 0.
- `bun run web:build` — clean (only the pre-existing chunk-size warning).
- Live end-to-end in real Chrome (isolated throwaway profile, CDP port 9335)
  against `bun run web:dev` (vite on :5173, machine bootstrap points the browser
  at the user's LOCAL daemon on 127.0.0.1:58744):
  - Sidebar "Add project" button → dialog opens (`[data-add-project-modal]`
    present, machine step skipped because only `local` is connected, footer shows
    the machine label).
  - Local folder → browse `~/` (21 dirs), descend into `~/dev/` by clicking a row,
    `..` row returns to `~/`, typing `/tmp/` re-browses.
  - Clicked `/tmp/add-project-dialog-web-test/`, submit label read `Add`, Enter on
    the path input registered the project; dialog closed and the project appeared
    in the sidebar as a normal group (aria-labels "Collapse
    add-project-dialog-web-test", "Create a terminal in …").
  - Cleanup: `POST /api/removeProject { projectId: "P53ls" }` → sidebar row gone,
    `listProjects` shows zero `add-project-dialog-web*` rows on the user's daemon,
    `/tmp/add-project-dialog-web-*` deleted. The user's real projects were never
    touched.
  - Screenshot reviewed: dialog is centered, top-1/3 anchored, sources list +
    Setup Required buttons + footer hints render correctly over the web shell.
  - Console: zero errors other than the `discoverSourceControl` CORS/404 noted below.
- Second pass against an **isolated Part-A daemon** (scratch `HOME`, port 58792,
  started from `gxserver-rs/target/debug/gxserver`, stopped and deleted
  afterwards) added as a second web machine, to exercise the endpoints the user's
  released daemon does not have yet:
  - Machine step rendered both machines (`data-add-project-machine-id` = `local`,
    `machine-parta-test`), local first.
  - `discoverSourceControl` through the host adapter rendered per-provider
    readiness correctly: GitHub → `gh auth login` hint (available/unauthenticated),
    GitLab → install-`glab` hint (missing), Bitbucket/Azure DevOps → "Ghostex
    cannot clone … by name yet. Choose Git URL…" (unsupported).
  - Git URL → `https://github.com/octocat/Hello-World.git` → destination
    `/tmp/add-project-dialog-web-clone/hello` showed `Create & Clone`; Enter ran
    start → poll `readCloneJob` → register; repo cloned on disk and the project
    was registered on the scratch daemon. Scratch daemon stopped, `HOME` and
    `/tmp/add-project-dialog-web-clone` deleted.

### Known gaps

- **The user's running gxserver (58744) predates Part A**, so
  `/api/discoverSourceControl` fails its CORS preflight there and every provider
  row shows the dialog's "Provider status unavailable. Open Settings -> Source
  Control and rescan." treatment. Local folder / Git URL are unaffected. This
  resolves itself when the daemon is rebuilt from Part A's source; it was verified
  working against a Part-A build.
- **Pre-existing, NOT introduced here:** projects belonging to a non-`local`
  machine do not render as rows under that machine's sidebar section on web, even
  though `createMergedSidebarGroups` builds the groups and the machine's
  presentation snapshot contains the project (reproduced with the scratch daemon:
  section header "Part A scratch daemon" renders, the `hello` project row does
  not). Remote adds on web therefore succeed server-side but stay invisible until
  that rendering gap is fixed. Worth a separate ticket.
- `onProjectAdded` and `onOpenSourceControlSettings` are not wired: web has no
  Settings > Source Control surface to navigate to, and the presentation delta
  already delivers the added project, so the host passes neither prop.
- No Storybook/unit coverage was added for the web host; behavior was verified in
  a real browser against real daemons instead.

---

## Part E — mobile (RN/Expo SDK 57) multi-screen Add Project flow

Date: 2026-07-30. Scope: `mobile/**` only. Expo SDK 57 versioned docs were read
before writing code (`https://docs.expo.dev/versions/v57.0.0/` index +
`sdk/haptics`), per `mobile/AGENTS.md`.

### Files changed

New (`mobile/src/addProject/` — the flow's own module):

- `mobile/src/addProject/paths.ts` — POSIX browse-path helpers (spec §2.3), no Windows/relative support.
- `mobile/src/addProject/client.ts` — the four CLI verbs as typed async functions + the flow's own CLI runner.
- `mobile/src/addProject/sources.ts` — source labels/hints/placeholders, readiness, ordering (spec §§3.2-3.4).
- `mobile/src/addProject/submit.ts` — submitted-path resolution, "will create" detection, client-known duplicate lookup.
- `mobile/src/addProject/useDirectoryBrowse.ts` — directory-keyed browse state hook.
- `mobile/src/addProject/primitives.tsx` — `AddProjectShell`, `SectionTitle`, `MutedText`, `ListSection`, `ListRow`, `SetupRequiredBadge`, `PrimaryActionButton`, `ProjectPathInput`, `ErrorBanner`, `PendingRow`, `RepositoryCard`.
- `mobile/src/addProject/FolderBrowser.tsx` — "Browse folders" section (spec §5.6).

New screens:

- `mobile/src/screens/AddProjectSourceScreen.tsx`
- `mobile/src/screens/AddProjectLocalScreen.tsx`
- `mobile/src/screens/AddProjectRepositoryScreen.tsx`
- `mobile/src/screens/AddProjectDestinationScreen.tsx`

Modified:

- `mobile/src/commands/ghostexCli.ts` — 4 new builders + `addProjectCommand` options; new `requirePositional` / `positiveIntegerFlag` guards.
- `mobile/src/navigation/types.ts` — 4 new routes.
- `mobile/App.tsx` — 4 `Stack.Screen` registrations.
- `mobile/src/copy.ts` — new `AddProjectCopy` block.
- `mobile/src/screens/SessionsScreen.tsx` — the Projects context-menu "Add Project" item now navigates; the `addProject` `Overlay` variant, its `PromptDialog` block, and the `addProjectCommand` import were removed.

### Exact new names

Navigation routes (`RootStackParamList`):

- `AddProjectSource: { machineId: string }`
- `AddProjectLocal: { machineId: string }`
- `AddProjectRepository: { machineId: string; source: AddProjectSourceId }`
- `AddProjectDestination: { machineId: string; remoteUrl: string; repositoryTitle: string; repositoryUrl: string }`

CLI command builders (`mobile/src/commands/ghostexCli.ts`):

- `addProjectCommand(path, options?: { createIfMissing?: boolean })` → `ghostex add-project '<path>' [--create-if-missing] --json`
- `browseDirectoriesCommand(partialPath, options?: { limit?: number })` → `ghostex browse-directories '<path>' [--limit n] --json`
- `discoverSourceControlCommand(options?: { timeoutMs?: number })` → `ghostex discover-source-control [--timeout ms] --json`
- `lookupRepositoryCommand(provider, repository, options?: { timeoutMs?: number })` → `ghostex lookup-repository '<provider>' '<repo>' [--timeout ms] --json`
- `cloneRepositoryCommand(remoteUrl, destinationPath, options?: { waitTimeoutMs?: number; timeoutMs?: number })` → `ghostex clone-repository '<url>' '<dest>' [--wait-timeout-ms n] [--timeout ms] --json`
- Exported type `SourceControlLookupProvider = 'github' | 'gitlab'`.

Client functions (`mobile/src/addProject/client.ts`):

- `browseDirectories(machine, partialPath): Promise<AddProjectBrowseResult>`
- `discoverSourceControl(machine): Promise<AddProjectSourceControlDiscovery>`
- `lookupRepository(machine, provider, repository): Promise<AddProjectRepositoryInfo>`
- `cloneRepository(machine, remoteUrl, destinationPath): Promise<AddProjectCloneResult>`
- `addProjectPath(machine, path, { createIfMissing }): Promise<void>`
- Types: `AddProjectProviderId`, `AddProjectSourceId`, `AddProjectProviderAuthStatus`,
  `AddProjectProviderStatus`, `AddProjectProviderDiscovery`, `AddProjectSourceControlDiscovery`,
  `AddProjectBrowseEntry`, `AddProjectBrowseResult`, `AddProjectRepositoryInfo`, `AddProjectCloneResult`.

Logic helpers: `ADD_PROJECT_PROVIDER_SOURCES`, `isLookupProvider`, `addProjectSourceLabel`,
`addProjectSourcePathHint`, `addProjectSourceRowTitle`, `addProjectSourceRowDescription`,
`addProjectRepositoryPlaceholder`, `addProjectRepositoryActionLabel`,
`buildAddProjectSourceReadiness`, `sortAddProjectProviderSources`, `orderedAddProjectSources`,
`AddProjectSourceReadiness`; `ADD_PROJECT_DEFAULT_PATH` (`"~/"`), `resolveSubmittedPath`,
`willCreateSubmittedPath`, `duplicateProjectFor`; `hasTrailingPathSeparator`,
`getBrowseDirectoryPath`, `getBrowseLeafPathSegment`, `appendBrowsePathSegment`,
`getBrowseParentPath`, `canNavigateUp`, `ensureBrowseDirectoryPath`,
`stripTrailingSeparators`, `joinPathSegment`, `cloneFolderName`.

Copy: `AddProjectCopy` in `mobile/src/copy.ts`.

`testID` automation hooks: `add-project-source-local`, `add-project-source-<sourceId>`,
`add-project-path-input`, `add-project-submit`, `add-project-browse-up`,
`add-project-browse-entry-<name>`, `add-project-repository-input`,
`add-project-repository-action`, `add-project-destination-input`, `add-project-clone`.

### Decisions taken

1. **No second add after a clone.** `gxserver-rs/src/repository_clone.rs` already
   registers the cloned project (`add_cloned_project` + presentation delta) before the
   job reaches `completed`, so the Destination screen stops at `clone-repository`.
   Issuing `add-project` afterwards would be a redundant no-op call, not a fix.
2. **The flow has its own CLI runner** (`runAddProjectCli`) instead of reusing
   `components/sessions/cli.ts::runGhostexCli`: it needs per-command exec timeouts
   (clone: 300 s exec / 240 s CLI wait vs. the 20 s inventory budget), the parsed JSON
   even on a non-zero exit (a failed clone job exits 1 while the RPC succeeded and the
   reason lives in `job.error`), and structured error text for the inline banners.
   Failure text order is `job.error` → `job.message` → envelope `message` → envelope
   `error` (gxserver's envelope `error` is the machine code, `message` is the sentence),
   falling back to the existing `summarizeFailure` SSH copy when there is no JSON.
3. **`--timeout` is raised for discovery and lookup** (30 s). The CLI's own gxserver HTTP
   budget defaults to 15 s, which is shorter than the worst case of four 5 s provider probes.
4. **Positional CLI arguments starting with `-` are rejected at the builder** — the Rust
   CLI parses argv before gxserver sees it, so such a value would silently become a flag.
5. **No environment/machine list on the Source screen.** The machine is chosen by the
   Projects context menu that opens the flow (plan Part E), so the "Connected
   environments" section has no counterpart; the machine is named in a muted line instead.
6. **Bitbucket / Azure DevOps rows are never enabled**, even if a future discovery marked
   them available: gxserver's `lookup-repository` only accepts `github|gitlab`. They render
   with their readiness hint plus a "Setup required" badge, and the badge has no action
   (mobile has no source-control settings screen to deep-link into).
7. **Discovery failure disables every provider** and shows an inline `ErrorBanner`; Local
   folder and Git URL stay enabled because neither depends on a hosting CLI. Readiness is
   never guessed from a failed probe.
8. **Submit label flips `Add project` ↔ `Create & add project`** (the "Create & Add"
   semantics) with a muted "This folder does not exist yet and will be created." hint. The
   flip is suppressed while a browse is pending or failed, so an unknown state never claims
   the folder is missing.
9. **Destination path gets the repository folder appended when the input ends with `/`.**
   gxserver refuses a non-empty existing destination, so a browsed directory alone is never
   valid; the effective destination is rendered under the input ("Clones into …") so nothing
   is hidden. The screen also opens pre-filled with `~/<repo-name>`.
10. **Duplicate handling** is the client-known check only (spec §5.8): an absolute resolved
    path matching a project already in the machine's inventory shows
    `Alert.alert("Project already exists", …)` and returns to Sessions instead of calling the
    CLI. `~`-prefixed paths are not compared (expansion is server-side) and fall through to
    gxserver's idempotent add. Nothing is added optimistically; every success calls
    `refreshMachineFresh` before navigating back.
11. **Browse never sends `cwd`** and hidden folders are always excluded (spec §5.6). The
    browse request is keyed on the directory portion of the query only, so typing a leaf name
    never refetches and no debounce exists.

### Verification run

- `bunx tsc --noEmit` in `mobile/` — clean (exit 0), before and after the simulator run.
- `bun install` — no changes (520 installs checked).
- `bunx expo prebuild` — regenerated `ios/`+`android/` and reinstalled CocoaPods, clean.
- `bunx expo run:ios --device "Ghostex Test iPhone"` — **Build Succeeded, 0 errors**;
  `iOS Bundled 19878ms index.ts (1168 modules)`, zero Metro errors/warnings; the app booted
  to the Sessions screen (screenshotted through the Simulator window) with no red box. That
  proves every new module (all reachable from `App.tsx`) bundles and evaluates on device.
- Pure-logic checks run under `bun` against the real modules (42 assertions, all passing):
  every path helper (`appendBrowsePathSegment`, `getBrowseParentPath`, `canNavigateUp`,
  `cloneFolderName` for `owner/repo`, `https://…/repo.git` and `git@host:org/repo.git`,
  `joinPathSegment`, `stripTrailingSeparators`), readiness for available/missing/
  unauthenticated/absent discovery, both orderings (`url, github, azure-devops, bitbucket,
  gitlab` with gh ready; `url, azure-devops, bitbucket, github, gitlab` with no discovery),
  labels/placeholders/action labels, all five command builders' exact strings, and the
  flag-shaped-positional rejection.
- CLI contract checked against a real daemon with the repo-built CLI
  (`gxserver-rs/target/debug/ghostex browse-directories '~/dev/' --json`): envelope keys are
  exactly `["entries","ok","parentPath","requestId"]`, `parentPath` is the server-resolved
  absolute path, entries are `{fullPath,name}` — i.e. what `client.ts` parses.

### Known gaps (honest)

- **The flow could not be walked in the simulator.** Its only entry point is the Projects
  section context menu, which exists only for a machine with a live inventory; the simulator
  has no saved SSH machine and this agent will not stand up an SSH server on the user's Mac,
  install the user's private key into the simulator, or mutate the user's live project list.
  So the Source/Local/Repository/Destination screens were verified by typecheck, the logic
  checks above, and a clean device bundle+boot — not by tapping through them. Anyone with a
  configured machine can walk them in one pass.
- **`discover-source-control` and `lookup-repository` could not be exercised end to end**:
  the *installed* `/opt/homebrew/bin/ghostex` and the running gxserver daemon both predate
  Part A, so the running daemon answers
  `No gxserver endpoint for POST /api/discoverSourceControl`. `browse-directories` (which the
  released daemon does support) was exercised and matched. Mobile users need an updated
  gxserver on the machine; until then the Source screen shows that error in its banner and
  every provider row is disabled — Local folder and Git URL still work.
- A Metro dev server (`bunx expo run:ios`) and the "Ghostex Test iPhone" simulator were left
  running with the app installed, so a follow-up verifier can reload without rebuilding.
- No tests were added (mobile has no test harness in-tree); the logic script lives in the
  session scratchpad, not the repo.
- `expo prebuild` reported the pre-existing `[Expo Dev Launcher] Strip Local Network Keys for
  Release` build-phase warning; unrelated to this work.

---

## Part C — gpui integration (app-modal kind, requestId bridge, entry points)

Date: 2026-07-30.

### Files changed

- `gpui/src/main.rs` — new `GpuiAppModalKind::AddProject` (5 arms: `from_modal_id`,
  `modal_id`, `window_title`, `window_size`, `open_message`); window-size constants;
  add-project timeout constants; `GpuiAddProjectDialogOperation` enum + param whitelist
  + `gpui_add_project_dialog_local_platform`; `handle_gpui_add_project_dialog_request_message`,
  `gpui_add_project_dialog_machine_options`, `dispatch_gpui_add_project_dialog_result`;
  `gpui_add_project_dialog_rpc_result` + `gpui_add_project_dialog_error_message`;
  command registered in BOTH app-modal routers; existing
  `handle_gpui_add_remote_project_path_message` fixed (60s timeout, presentation refresh
  on both arms).
- `native/sidebar/modal-host.tsx` — `addProject` modal kind, state, open/clear/renderable
  wiring, `<AddProjectModal>` render with all 8 callbacks, `requestAddProjectDialogOperation`
  waiter + per-operation timeouts, 7 result readers; legacy `waitForRemoteProjectAddResult`
  raised 20s → 60s.
- `sidebar/app-modal-host-bridge.ts` — `"addProject"` in `AppModalKind` + payload variant
  `{ machineId?, modal: "addProject", type: "open" }`.
- `shared/session-grid-contract-sidebar.ts` — `SidebarAddProjectDialogOperation`,
  `SidebarAddProjectDialogRequestParams`, and the `addProjectDialogRequest` variant of
  `SidebarToExtensionMessage`.
- `sidebar/sidebar-app.tsx` — `pickWorkspaceFolder` replaced by `openAddProjectModal(machineId?)`;
  V1 Projects header `onAddProject`, V1 remote machine header `onAddProject` (was
  `remoteProjectPicker`), and the new V2 `onAddProject` prop all call it.
- `sidebar/v2/sidebar-v2-create-button.tsx` — optional `onAddProject` prop + "Add project…"
  menu item (own section, `data-sidebar-v2-create-menu-item="addProject"`).
- `sidebar/v2/sidebar-v2-root.tsx` — optional `onAddProject` prop threaded to the toolbar
  create button only (group headers deliberately do not get it).
- `sidebar/command-palette.tsx` — "Add Project" moved from `kind: "sidebarMessage"`
  (`pickWorkspaceFolder`) to `kind: "appModal"`, `modal: "addProject"`; icon moved to
  `AppModalCommandIcon`.
- `sidebar/command-palette.test.ts` — assertion updated to the app-modal shape.
- `sidebar/styles/modals.css` — native-child-window override pinning `.add-project-modal`
  to `top: 0` (see decision 5).

`gpui/src/cef/*` and `gpui/sidebar/gxserver-runtime.ts` needed NO changes (see decision 4).

### Exact new names

Bridge message (renderer → host, `SidebarToExtensionMessage`):

```ts
{ machineId?: string; operation: SidebarAddProjectDialogOperation;
  params?: SidebarAddProjectDialogRequestParams; requestId: string;
  type: "addProjectDialogRequest" }
```

- `SidebarAddProjectDialogOperation` = `"add" | "browse" | "cancelCloneJob" |
  "discoverSourceControl" | "listMachines" | "lookupRepository" | "readCloneJob" | "startClone"`.
- `SidebarAddProjectDialogRequestParams` = `{ createIfMissing?, cwd?, destinationPath?,
  jobId?, partialPath?, path?, provider?, remoteUrl?, repository? }` (all optional; the host
  validates per operation and forwards only that operation's fields).

Host → modal-host answer (modal-host-only union member, like `remoteProjectAddResult`):

```ts
{ error?: string; ok: boolean; requestId: string; result?: unknown;
  type: "addProjectDialogResult" }
```

`result` is the gxserver `result` object forwarded unchanged
(`{ entries, parentPath }`, `{ project }`, `{ discovery }`, `{ repository }`, `{ job }`),
except `listMachines`, which is host-built: `{ machines: Array<{ description?, label,
machineId, platform? }> }`.

App-modal kind: `"addProject"` / `GpuiAppModalKind::AddProject`, window title
"Ghostex Add Project", fixed window `APP_MODAL_HOST_ADD_PROJECT_WINDOW_WIDTH` 640 ×
`..._HEIGHT` 520.

Rust: `GpuiAddProjectDialogOperation`, `gpui_add_project_dialog_params`,
`gpui_add_project_dialog_bounded_text`, `gpui_add_project_dialog_local_platform`,
`gpui_add_project_dialog_rpc_result`, `gpui_add_project_dialog_error_message`,
`GhostexGpuiApp::handle_gpui_add_project_dialog_request_message`,
`::gpui_add_project_dialog_machine_options`, `::dispatch_gpui_add_project_dialog_result`.
Constants: `GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT` (60s, also used for `startClone` and by the
legacy `addRemoteProjectPath` handler), `..._BROWSE_TIMEOUT` (15s), `..._DISCOVERY_TIMEOUT`
(30s), `..._LOOKUP_TIMEOUT` (20s), `..._JOB_TIMEOUT` (20s),
`GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID = "local"`.

TS: `ADD_PROJECT_DIALOG_{ADD,BROWSE,DISCOVERY,LOOKUP,JOB}_TIMEOUT_MS`,
`requestAddProjectDialogOperation`, `createAddProjectRequestId`,
`readAddProject{MachineOptions,BrowseResult,AddResult,Discovery,RepositoryInfo,CloneHandle,CloneJob}`,
`AddProjectModalState`. Sidebar: `openAddProjectModal(machineId?)`.
Support-log breadcrumb: `gpui.addProject.request` with `{ operation }` only.

Endpoint map owned by Rust: `add → /api/addProjectPath`,
`browse → /api/browseProjectDirectories`, `discoverSourceControl → /api/discoverSourceControl`,
`lookupRepository → /api/lookupRepository`, `startClone → /api/startRepositoryClone`,
`readCloneJob → /api/readRepositoryCloneJob`, `cancelCloneJob → /api/cancelRepositoryCloneJob`.

### Decisions taken

1. **The local machine id is the literal string `"local"`.** Remote ids must match
   `gpui_normalize_remote_machine_id` (`remote-…`), so the two vocabularies cannot collide,
   and an unrecognized id is rejected ("That machine is unavailable.") instead of silently
   being treated as local. Part D should use the same id for its local machine so the shared
   dialog behaves identically on web.
2. **One bridge command for all eight callbacks** instead of eight message types. `main.rs`
   is huge and concurrently edited; the operation enum keeps endpoint choice, timeouts, and
   the per-operation field whitelist in one place, which is also what keeps hosts/ports/tokens
   out of the renderer.
3. **`listMachines` lists ALL saved remote machines plus this computer**, not only connected
   ones. A remote entry point preselects its machine, and hiding a disconnected machine would
   silently drop the preselection back to the local filesystem. Disconnected machines carry
   `description: "Not connected"`, and any operation against them fails with
   "That machine is not connected." Only `label`/`machineId`/`platform`/`description` cross
   the boundary — never `sshHost`, `sshUser`, identity files, or tunnel ports. The `add`
   request sends no `name`; gxserver derives the project name from the resolved leaf.
4. **Local adds are performed by Rust against the local daemon, then announced to the sidebar
   runtime with the existing `workspaceFolderPicked` payload.** The sidebar runtime owns local
   project state and receives no daemon pushes, so it re-registers idempotently, focuses the
   project, and pulls a fresh local presentation — exactly what the OS-folder-picker path did.
   That is why `gpui/sidebar/gxserver-runtime.ts` needed no change.
5. **Fixed 640×520 window with a CSS top-anchor override, not one-shot fit-height.** The
   dialog changes height on every step, so a fitted frame would freeze at the first step. The
   shared `CommandDialog` anchor is `top-1/3`, which is right over the app window but pushes
   the dialog a third of the way down its own child window, so
   `.app-modal-host-native-window-body .add-project-modal { top: 0; max-height: 100vh; }`
   pins it. The dialog's list already caps at `min(28rem, 60vh)` and scrolls.
6. **Verbatim errors are limited to gxserver's own structured `message`** on a rejected
   request (control chars stripped, ≤512 chars). Transport failures stay as fixed local copy
   ("The remote machine did not answer." / "gxserver is not reachable.") so no tunnel host,
   port, token, or raw body can reach CEF.
7. **Presentation refresh runs on BOTH the Ok and Err arms** for `add` (and for a
   `readCloneJob` answer whose job is `completed`) on remote machines — the root-cause fix:
   an add that lands after the request gives up still registered the project, and the
   machine's presentation stream is often the broken part. The same fix was applied to the
   pre-existing `addRemoteProjectPath` handler, whose 20s timeout (vs ~19s reconnect-time
   adds) is now 60s on both the Rust and JS sides.
8. **Local Err does not dispatch anything.** There is no local project path to announce and
   no local presentation stream to repair; the dialog keeps the daemon's error visible and the
   user retries. The after-the-timeout hazard is specific to remote tunnels.
9. **The command palette's Add Project became an app-modal command.** `pickWorkspaceFolder`
   now has no sender in the shared sidebar (the deprecated Swift app's `native-sidebar.tsx`
   still posts it and was left untouched per AGENTS.md); the contract message, the Rust
   handler, and the gxserver-runtime handler all remain and are still used by the
   local-add announcement path.
10. **V2's "Add project…" lives in the toolbar create menu only** (its own section under the
    Quick entries). Group headers pass no `onAddProject` — adding a project is not a
    per-project action — and the chevron now appears whenever that callback exists.

### Verification run

- `cargo check` in `gpui/` — clean (no errors; no new warnings attributable to these edits).
- `bunx vite build --config gpui/vite.config.ts` (CEF sidebar/modal-host/titlebar bundles) — built.
- `bun run typecheck` (repo tsconfig: shared/ + sidebar/ + native/sidebar/) — clean.
- `bun run web:typecheck` — clean (contract addition does not break Part D's in-flight work).
- `bunx vitest run ./sidebar/` — 542 passed, 90 files; the only failing file,
  `native/sidebar/native-agent-prompt-text.test.ts`, is a pre-existing `bun:test` file that
  cannot run under vitest and is unrelated.
- `bun test ./sidebar/command-palette.test.ts` — 14 passed.
- The app was NOT launched and no gpui tests were added (repo rule).

### Known gaps

- **Not verified in the running app** (rule: no `bun run start`). The bridge round trip,
  the 640×520 frame, and the top-anchor CSS are reasoned from the existing app-modal
  conventions, not observed. A verifier with the app already running should check: machine
  step for local+remote, remote browse, "Create & Add", a Git-URL clone, and the child-window
  layout at both the sources step and a long browse list.
- `activeProjectCwd` is still not supplied (Part B's gap): `./` and `../` queries report
  "Relative paths require an active project." in gpui. Wiring it needs the sidebar's focused
  project path in the open message.
- `onOpenSourceControlSettings` is not wired, so a "Setup Required" row has no Settings jump
  in gpui.
- Remote machine options carry no `platform`, so the dialog falls back to the modal host's
  `navigator.platform` for the submit-modifier label and Windows-path detection on remote
  machines. Ghostex's saved machine settings have no OS field to report today.
- The deprecated macOS Swift app hosts the same `modal-host.tsx` but its native side does not
  know the `addProject` kind, so Add Project is broken there. Deliberate per AGENTS.md
  (no parity work in `native/`/`src/`).
- `remoteProjectPicker` is now entry-point-less (it stays wired in the modal host and inside
  `add-repository-modal.tsx` as the clone-destination browser). Removing the kind is a
  separate cleanup.

---

## Part V — Final verification (round 1)

Date: 2026-07-30. Independent re-verification of Parts A–E against the plan's
acceptance criteria. No product code was changed by this pass; the only writes
were scratch dirs/daemons that were removed afterwards.

### Files changed

None (verification only).

### New endpoint/message/prop/verb names

None introduced. All names from the Part A–E handoffs were confirmed present as
spelled (`/api/discoverSourceControl`, `/api/lookupRepository`,
`addProjectDialogRequest`/`addProjectDialogResult`,
`SidebarAddProjectDialogOperation`, `GpuiAppModalKind::AddProject`,
`openAddProjectModal`, `ghostex browse-directories|discover-source-control|lookup-repository|clone-repository`,
`add-project --create-if-missing`).

### Verification commands run and results

- `cargo check --all-targets` (gxserver-rs): clean.
- `cargo check` (gpui): clean (pre-existing warnings only).
- `cargo test` (gxserver-rs): 614 passed, 0 failed. Targeted re-runs:
  `source_control` 11 passed, `clone` 9 passed, `browse` 17 passed,
  `create_if_missing` 1 passed.
- `bun run typecheck`: clean. `bun run web:typecheck`: clean.
  `bun run web:build`: clean (pre-existing chunk-size warning only).
  `bunx tsc --noEmit` (mobile/): exit 0.
  `bunx vite build --config gpui/vite.config.ts` (CEF bundles): built.
- `bun test ./sidebar/command-palette.test.ts`: 14 passed.
- Storybook (`bun run storybook`, driven headless via CDP, `storyFinished`
  channel event read per story): all 10 `Modals/Add Project Interactions`
  stories `status: "success"`; all 6 `Modals/Add Project` visual stories render
  the dialog with no error display and no console errors.
- fidelity spot-checks (live DOM, Sources fixture): source order
  Local folder → Git URL → ready providers → unready A→Z with "Setup Required";
  local browse opens with `~/`, placeholder "Enter path (e.g. ~/projects/my-app)",
  NO auto-highlight (Enter submits typed path; ArrowDown+Enter descends —
  observed `~/` → `~/Desktop/`); label flips Add ↔ Create & Add with the
  "Press Enter to create this folder and add it as a project." hint; clearing
  the initialQuery pops to Sources; repo step placeholder "Enter Git clone URL",
  "Continue"+Enter kbd, hint per spec; destination step shows repository card,
  "Select where to clone", Clone/Create & Clone; footer hints present; no up-row
  at `~/` (matches `canNavigateUp`).
- ghostex-web end-to-end (`bun run web:dev` + user's daemon on 58744): sidebar
  "Add project" button → dialog → Local folder → typed `/tmp/add-project-verify-scratch`
  (live server browse showed the entry, label "Add") → Enter → dialog closed and
  the project appeared in the sidebar. Cleanup: `/api/removeProject` (P2amk)
  succeeded, daemon shows zero matching projects, `/tmp` scratch dir removed.
  Providers show "unavailable" on 58744 because the running daemon predates
  Part A (known Part D gap; resolves when the daemon is rebuilt).
- Isolated Part-A daemon (scratch HOME, `GHOSTEX_GXSERVER_DEV_PORT=58793`,
  stopped afterwards): `/api/discoverSourceControl` → gh available/unauthenticated,
  glab missing, bitbucket+azure-devops unsupported with Git-URL hints;
  `/api/browseProjectDirectories` `~/` → hidden shown for trailing separator,
  resolved parentPath; `/api/addProjectPath` `createIfMissing:true` → dir created
  on disk + project registered. CLI verbs `browse-directories`,
  `discover-source-control`, `add-project --create-if-missing` all exercised
  against it with `--server/--token-stdin`, correct JSON.
- Mobile: reused Part E's prebuild; `Ghostex Test iPhone` simulator + fresh
  Metro; app launched cleanly to the Sessions screen (screenshot verified, no
  red box) with all new AddProject modules in the bundle. The four flow screens
  remain unreachable interactively: the only entry is the Projects context menu
  of a machine with live inventory, and no SSH machine exists in the simulator
  (deep links only handle `ghostex://session`). Same limitation Part E reported.
- gpui bug-fix criteria confirmed in code: legacy `addRemoteProjectPath` and the
  dialog's add/startClone use the 60s timeout on both Rust
  (`GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT`) and JS (`ADD_PROJECT_DIALOG_ADD_TIMEOUT_MS`,
  legacy waiter raised 20s→60s); remote presentation refresh runs on BOTH arms
  for `add` (and legacy add); entry points rewired (V1 header :5309, V1 remote
  machine header :5571 preselecting `machine.id`, V2 create menu, command
  palette app-modal); web host applies its own 60s `withTimeout` to
  addProject/startClone; CEF receives only `label`/`machineId`/`platform`/
  `description:"Not connected"` machine fields, sanitized gxserver `message`
  strings (control-chars stripped, ≤512 chars), fixed transport copy, and a
  `gpui.addProject.request { operation }` breadcrumb — no hosts/tokens/paths in
  logs. Remote allowlist carries both new endpoints (protocol.rs:466-467).

### Defects found (round 1)

1. MINOR (Part C, `gpui/src/main.rs` ~:30875-30891): the remote presentation
   refresh does not run on the Err arms of `startClone`/`readCloneJob`. A remote
   clone whose `readCloneJob` poll answer is lost (20s transport timeout) aborts
   the dialog's poll loop, but the server job keeps running and registers the
   project on completion — which then stays invisible exactly like the original
   add bug. Suggested fix: for remote machines, also call
   `refresh_gpui_remote_gxserver_presentation_in_background` on the Err arm of
   `StartClone` and `ReadCloneJob` (or on `ReadCloneJob` Err only).

### Known gaps (carried, not defects)

- Mobile flow screens not walked interactively (no SSH machine in simulator).
- User's running daemon predates Part A: provider rows show "unavailable" until
  it is rebuilt; remote projects on web don't render rows (pre-existing).
- `activeProjectCwd` unsupplied in gpui; `onOpenSourceControlSettings` unwired
  in gpui/web (documented Part B/C/D gaps).
- Everything verified is uncommitted working-tree state — commit promptly so
  concurrent agents cannot clobber it.

---

## Part C — Fix round 1 (remote clone invisibility hazard)

Date: 2026-07-30. Fixes the single Part C defect from Part V round 1.

### Files changed

- `gpui/src/main.rs` only:
  - `handle_gpui_add_project_dialog_request_message`: new `clone_watch_job_id`
    (captured from the validated params before the request is spawned, only for
    `readCloneJob`) and new `clone_answer_lost` flag (`result.is_err()` for
    `StartClone` | `ReadCloneJob`). The refresh guard is now
    `operation != Add && !clone_completed && !clone_answer_lost`, so the remote
    arm also runs `refresh_gpui_remote_gxserver_presentation_in_background` when
    a clone request fails.
  - New `GhostexGpuiApp::watch_gpui_remote_add_project_clone_job(remote_machine_id,
    job_id, cx)` — native follow-up poller for a remote clone job whose dialog
    poll answer was lost.
  - New constants `GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_INTERVAL` (5s),
    `GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_POLLS` (120 ≈ 10 minutes),
    `GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_CONSECUTIVE_ERRORS` (3).

### Exact new names

Rust only; no new endpoints, bridge messages, props, or CLI verbs. No TS or CEF
change (the watcher is invisible to the renderer by design — the dialog already
gave up and must not be resurrected behind the user's back).

- `GhostexGpuiApp::watch_gpui_remote_add_project_clone_job`
- `GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_INTERVAL`
- `GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_POLLS`
- `GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_CONSECUTIVE_ERRORS`

### Decisions taken

1. **The immediate refresh alone would have been cosmetic for the reported
   scenario.** The failure the verifier described is "the server-side clone keeps
   running and registers the project on completion": a refresh fired at the
   moment the poll answer is lost snapshots a state where the project does not
   exist yet. So the Err arms now do both: refresh immediately (covers the case
   where the job had already completed when the answer was lost, and repairs a
   dead presentation stream) and, for `readCloneJob`, hand the job id to a native
   watcher that follows it to a terminal state and refreshes then.
   `gxserver-rs/src/repository_clone.rs:278-300` confirms the job registers the
   project itself, so the watcher only needs to make it visible.
2. **`startClone` Err gets the refresh but no watcher.** A lost `startClone`
   answer means the job id never reached us, and gxserver exposes no
   list-jobs endpoint, so there is nothing to follow. Documented as a gap below.
3. **The watcher is bounded on two axes**: at most 120 polls at 5s (~10 minutes
   of clone time) and at most 3 consecutive transport failures, because a tunnel
   that cannot answer a poll cannot deliver a presentation refresh either. It
   re-resolves the tunnel target from the app on every iteration (a reconnect
   changes the port), stops if the machine has no target, and stops on any
   non-`running` job state (`completed`/`failed`/`canceled`/unexpected shape) —
   all of them are worth one final refresh, none of them is worth more polling.
4. **No breadcrumb was added for the watcher.** `gpui.addProject.request` already
   records the operation; a per-poll or per-watch line would add volume without a
   bounded id worth reading, and job ids are gxserver-minted opaque strings that
   the support log has no use for.
5. **Nothing was changed in `sidebar/add-project-modal/*` (Part B).** The dialog
   still ends the clone on the first rejected poll and shows the error verbatim;
   the native side now makes the eventual result visible. Making the poll loop
   itself retry transient transport failures is a Part B behavior change with
   its own interaction stories and was deliberately left alone.

### Verification commands run and results

- `cargo check` in `gpui/` — clean, no errors; no warning references
  `watch_gpui_remote_add_project_clone_job`, `clone_answer_lost`,
  `clone_watch_job_id`, or the new constants.
- `bunx vite build --config gpui/vite.config.ts` — built in 3.37s (no TS surface
  changed, run to confirm the CEF bundles still build against this tree).
- `bun run typecheck` — clean (no output).
- The app was NOT launched; no gpui tests were added.

### Known gaps

- A lost `startClone` answer whose job did start is still only covered by the
  immediate refresh (no job id to follow). Closing it would need a gxserver
  endpoint that lists in-flight clone jobs, or a client-supplied job id on
  `/api/startRepositoryClone`.
- The watcher gives up after ~10 minutes of clone time; a clone longer than that
  becomes visible on the next natural presentation refresh/reconnect.
- Still not verified in the running app (repo rule): the watcher path needs a
  remote machine plus an interrupted poll to observe.

---

## Part V2 — Final verification (round 2)

Date: 2026-07-31. Independent re-verification of Parts A–E plus the round-1 fix
(remote clone watcher). No product code changed; writes were limited to scratch
dirs, a scratch project (added and removed), and the simulator.

### Files changed

None (verification only).

### New endpoint/message/prop/verb names

None introduced. Names from all previous handoffs confirmed present as spelled.

### Verification commands run and results

- `cargo check --all-targets` (gxserver-rs): clean. `cargo test` (gxserver-rs):
  614 passed, 0 failed. Targeted: `source_control` 11, `clone` 9, `browse` 17,
  `create_if_missing` 1 — all passed.
- `cargo check` (gpui): clean (pre-existing warnings only).
- `bun run typecheck`: exit 0. `bun run web:typecheck`: exit 0.
  `bun run web:build`: clean (pre-existing chunk-size warning).
  `bunx tsc --noEmit` (mobile/): exit 0.
  `bunx vite build --config gpui/vite.config.ts`: built.
- `bun test sidebar/command-palette.test.ts`: 14 passed.
  `bunx vitest run sidebar/`: 542 passed; only failure is the pre-existing
  `native/sidebar/native-agent-prompt-text.test.ts` bun:test/vitest mismatch.
- Storybook driven in real headless Chrome over CDP (isolated profile, port
  9333), reading the `storyFinished` channel event per story: all 10
  `Modals/Add Project Interactions` stories and all 6 `Modals/Add Project`
  visual stories → `status: "success"` (16/16).
- Independent fidelity spot-checks on the live DOM: source order
  Local folder → Git URL → ready providers → unready A→Z with disabled rows +
  Setup Required; local browse opens `~/` with placeholder "Enter path (e.g.
  ~/projects/my-app)", NO auto-highlight, Enter submits typed path,
  ArrowDown+Enter descends (`~/` → `~/Desktop/`), footer flips to ⌘ Enter when a
  row is highlighted; Add ↔ Create & Add flip with the "Press Enter to create
  this folder…" hint; clearing initialQuery pops to Sources; Backspace-on-empty
  from the clone-destination step pops the whole clone flow to Sources; url step
  placeholder "Enter Git clone URL" + Continue/Enter + spec hint; provider step
  "Enter GitHub repository (owner/repo)" + Lookup/Enter + spec hint; destination
  step repository card + "Select where to clone" + Clone label + query reset to
  `~/`.
- ghostex-web end-to-end (`bun run web:dev`, user's daemon 58744): sidebar
  "Add project" button → dialog (machine step skipped, one machine) → Local
  folder browse `~/` (21 dirs, live server), descend/`..`/retype → typed
  `/tmp/add-project-verify-r2` (entry shown, label "Add") → Enter → dialog
  closed, project appeared in sidebar. Cleanup: `remove-project` (P1qw4) via the
  repo-built CLI, sidebar row confirmed gone, `/tmp/add-project-verify-r2`
  deleted. Providers show "unavailable" on 58744 because the running daemon
  predates Part A (known, resolves on rebuild).
- gpui bug-fix criteria re-confirmed in code: 60s add/startClone timeouts on
  both sides (`GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT` main.rs:79541,
  `ADD_PROJECT_DIALOG_ADD_TIMEOUT_MS` modal-host.tsx:737, legacy waiter 60s);
  presentation refresh on BOTH arms for add + legacy add; round-1 fix verified
  present (`clone_answer_lost` main.rs:30880, refresh guard :30902-30904,
  `watch_gpui_remote_add_project_clone_job` :30960 with bounded polls/errors);
  machine options carry only label/machineId/platform/"Not connected"
  (:31040-31086); error strings sanitized (control chars stripped, ≤512,
  fixed transport copy — :103374-103410); breadcrumb `gpui.addProject.request`
  `{ operation }` only; remote allowlist protocol.rs:466-467; entry points
  rewired (sidebar-app.tsx:5175/:5309/:5571, command palette app-modal, V2
  create menu).
- Mobile, iOS simulator: the original "Ghostex Test iPhone" device went bad
  (two SIGBUS crashes inside RN Fabric/Expo native view registration, then
  installs stopped registering even after CoreSimulator restart). The same
  Part-E build installed and ran cleanly on "Ghostex Test 2" against Part E's
  still-running Metro. Unlike round 1, the flow WAS walked: the user's saved
  SSH machine (madda@127.0.0.1 via Tailscale) gave a live inventory, so
  Projects-context-menu → "Add Project" → Source screen (Local folder + Git URL
  enabled; Azure DevOps/Bitbucket/GitHub/GitLab disabled with "Provider status
  unavailable on this machine." + SETUP REQUIRED, discovery ErrorBanner shown) →
  Local folder screen (banner → `~/` path input → "Add project" button →
  "Browse folders"/"No folders here.") → back → Git URL Repository screen
  (placeholder `https://github.com/org/repo.git`, Continue) all rendered and
  navigated correctly. Not exercisable: text entry (background driver cannot
  type into the iOS keyboard path; simctl pbcopy timed out), so the Destination
  screen and a real mobile add/clone/lookup remain unexercised; additionally the
  machine's installed `ghostex` predates Part A, so `discover-source-control`
  and `browse-directories` answer "Unknown command" (shown inline — the honest
  outdated-CLI state).

### Defects found (round 2)

1. MINOR (Part E): the mobile inline ErrorBanner renders the FULL raw CLI
   stderr when a verb is unknown — "Unknown command: discover-source-control"
   followed by the entire multi-page ghostex usage dump, on both the Source and
   Local screens. Any machine whose installed CLI predates Part A (the common
   state right after ship) gets a nearly unusable screen. Suggested fix: in
   `mobile/src/addProject/client.ts`, truncate non-JSON failure text to the
   first meaningful line (or map "Unknown command:" to a short "This machine's
   Ghostex CLI is too old for this feature — update Ghostex on the machine."
   message).

### Known gaps (carried)

- Mobile Destination screen + real mobile add/clone/lookup unexercised (input
  injection limitation + outdated machine CLI); logic covered by Part E's
  42-assertion script and typecheck.
- "Ghostex Test iPhone" simulator device is wedged (app crashes/installs don't
  register); "Ghostex Test 2" now has the app installed and running against
  Part E's Metro (left running, matching Part E's handoff).
- User's daemon 58744 and installed /opt/homebrew/bin/ghostex predate Part A;
  provider rows degrade gracefully until both are rebuilt/reinstalled.
- Remote projects on web don't render rows (pre-existing, Part D handoff).
- `activeProjectCwd` unsupplied in gpui; `onOpenSourceControlSettings` unwired
  in gpui/web (documented Part B/C/D gaps).
- All verified work is uncommitted working-tree state — commit promptly.

---

## Part E — Fix round 1 (outdated-machine CLI dump in the inline banner)

Date: 2026-07-31. Fixes verifier finding "MINOR (Part E): full raw CLI usage dump
in the mobile ErrorBanner".

### Root cause (not a truncation problem)

`ghostex <unknown verb> --json` does not print to stderr — `ghostex_cli::run()`
sees `--json` in argv and prints the failure as a JSON envelope:
`{"ok":false,"error":"Unknown command: discover-source-control\n\n<entire usage>"}`
(measured: 11 080 bytes on this machine). `runAddProjectCli` parsed that envelope,
`structuredError()` picked `error`, and it was thrown verbatim — so the 220-char
`summarizeFailure` truncation never even applied. The real condition is not "long
error text", it is "this machine's Ghostex predates the feature", which has exactly
one actionable answer, so it is now classified at the source instead of trimmed.

### Files changed

- `mobile/src/copy.ts` — added `FailureCopy.outdatedForFeature`.
- `mobile/src/inventory/client.ts` — added exported `isOutdatedMachineFailure()`;
  `summarizeFailure` returns the new copy for that class (after, and without
  disturbing, the existing `unknown command: android-check` → `FailureCopy.oldCli`
  branch used by the connect check).
- `mobile/src/addProject/client.ts` — `runAddProjectCli`'s structured-error path
  maps the same class to the new copy before throwing; imports `FailureCopy` and
  `isOutdatedMachineFailure`.

### Exact new names

- `FailureCopy.outdatedForFeature` = `"This machine's Ghostex is too old for this
  feature. Update Ghostex on the machine, then try again."`
- `isOutdatedMachineFailure(text: string | null | undefined): boolean` (exported from
  `mobile/src/inventory/client.ts`) — true for `Unknown command:` (any verb) and for
  `No gxserver endpoint for` (case-insensitive).

### Decisions taken

1. **Classify, don't truncate.** A blanket "first meaningful line" trim of non-JSON
   failure text would have been wrong twice: the dump arrives as JSON (so no trim
   would run), and SSH login-shell banners mean the first line is often a motd rather
   than the error. The 220-char tail truncation in `summarizeFailure` is unchanged.
2. **The outdated-daemon sibling is folded in.** A machine with a new CLI but an old
   gxserver answers `No gxserver endpoint for POST /api/discoverSourceControl` — same
   user, same remedy — so it maps to the same copy instead of leaking endpoint paths
   into the banner.
3. **The fix lives in `summarizeFailure` + the Add Project runner**, so every mobile
   surface that summarizes CLI failures (sessions, inventory, machines, terminal)
   benefits; nothing is special-cased per screen.
4. **`android-check` keeps its own message** (`FailureCopy.oldCli` names the verb the
   connect check needs); its branch runs first.
5. No verb name is interpolated into the banner: it is CLI jargon for a phone user,
   and it would let arbitrary CLI text back into the UI string.

### Verification run

- `bunx tsc --noEmit` in `mobile/` — clean (exit 0).
- Real-output logic checks (11 assertions, all passing) against the REAL modules
  bundled with native stubs: captured `ghostex discover-source-control --json` from
  this machine's installed (pre-Part-A) CLI — an 11 080-byte `error` string — fed
  through `discoverSourceControl()` → banner is the single short line; raw-stderr
  `Unknown command: browse-directories` + 300 usage lines through
  `browseDirectories()` → same short line; `No gxserver endpoint for POST
  /api/discoverSourceControl` → same short line; a genuine structured error
  ("Destination exists and is not empty.") still passes through verbatim; and the
  `android-check`, permission-denied, unrelated-text and empty-text `summarizeFailure`
  branches are unchanged.
- **Live iOS simulator, against the user's saved SSH machine** (`Ghostex Test 2`,
  reloaded from the running Metro server, machine `madda@127.0.0.1` whose installed
  CLI predates Part A): Projects context menu → Add Project → the Source screen shows
  the two-line banner with `Local folder`, `Git URL` and all four provider rows
  visible above the fold; `Local folder` → the Local screen shows the same two-line
  banner with `PROJECT PATH`, the input, `Add project` and `BROWSE FOLDERS` all above
  the fold. Screenshots: `fix-source3.png`, `fix-local1.png` in the session scratchpad.
  Navigated back to Sessions afterwards; nothing was added, cloned or mutated.

### Known gaps

- Repository/Destination screens still unexercised live (they need a machine with a
  post-Part-A CLI + daemon); their failure text flows through the same runner, so the
  outdated-machine class is covered by the logic checks above.
- The banner still shows gxserver's own sentence for every other failure, by design.
- Change is uncommitted working-tree state — commit promptly.

---

## Part V3 — Final verification (round 3)

Date: 2026-07-31. Independent re-verification of Parts A–E plus both fix rounds
(round 1: remote clone watcher; round 2: mobile outdated-CLI banner). No product
code changed; writes were limited to scratch dirs, one scratch project (added and
removed), and the simulator.

### Files changed

None (verification only).

### New endpoint/message/prop/verb names

None introduced. All names from previous handoffs confirmed present as spelled.

### Verification commands run and results

- `cargo check --all-targets` (gxserver-rs): clean. `cargo check` (gpui): clean —
  the only non-pre-existing-looking warning (`unused variable: cx`,
  main.rs:39046 `receive_sidebar_create_project_terminal_payload`) is in
  unrelated windows-gated create-project-terminal code from another effort, not
  add-project.
- `cargo test` (gxserver-rs): 614 passed, 0 failed. Targeted filters:
  `source_control` 11, `clone` 9, `browse` 17, `create_if_missing` 1 — all pass.
- `bun run typecheck`: exit 0. `bun run web:typecheck`: exit 0.
  `bun run web:build`: clean (pre-existing chunk-size warning only).
  `bunx tsc --noEmit` (mobile/): exit 0.
  `bunx vite build --config gpui/vite.config.ts`: built.
- Storybook (`bun run storybook`, isolated headless Chrome, CDP 9333, channel
  hook installed via `Page.addScriptToEvaluateOnNewDocument` reading
  `storyFinished`): all 10 `Modals/Add Project Interactions` + all 6
  `Modals/Add Project` stories → `storyFinished: success` (16/16). Only console
  error across runs was a network favicon-style 404 with no failing story
  resource (harness noise).
- Independent fidelity spot-checks on the live DOM: source order local →
  url → github(ready) → azure-devops/bitbucket/gitlab (unready A→Z,
  aria-disabled, 3 Setup Required buttons); local browse opens `~/` with
  placeholder "Enter path (e.g. ~/projects/my-app)", NO auto-highlight, submit
  label "Add", no `..` at `~/`; typing `~/brand-new-folder` flips to
  "Create & Add" + "Press Enter to create this folder and add it as a project.";
  ArrowDown highlight flips footer to "⌘ Enter Add" and Enter descends
  (`~/` → `~/Desktop/`, `..` row appears); clearing the initialQuery pops to
  Sources; Git URL step placeholder "Enter Git clone URL", Continue+Enter kbd,
  hint per spec §2.11.
- ghostex-web end-to-end (`bun run web:dev` + user's daemon 58744): sidebar
  "Add project" button → shim posts `{modal:"addProject", type:"open"}` →
  dialog opens (verified with instrumented shim; one earlier click before the
  app finished hydrating did nothing — retry after hydration worked, not a
  product defect); Local folder → live browse `~/` (21 dirs) → typed
  `/tmp/add-project-verify-r3` (entry row shown, label "Add") → Enter → dialog
  closed, project row appeared in the sidebar. Cleanup:
  `ghostex remove-project --project-id P3rqi --json` → ok, daemon shows zero
  matching projects, sidebar row gone live, `/tmp/add-project-verify-r3` removed.
- gpui bug-fix criteria re-confirmed in code: 60s add/startClone timeout both
  sides (`GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT` main.rs:79541 + legacy handler
  main.rs:30681; `ADD_PROJECT_DIALOG_ADD_TIMEOUT_MS` modal-host.tsx:737 with the
  legacy waiter on it); presentation refresh on BOTH arms for add + legacy add;
  round-1 fix intact (`clone_answer_lost` :30880, refresh guard, watcher
  :30960 with bounded polls/errors + startClone-Err refresh); machine options
  carry only label/machineId/platform/"Not connected" (:31040); error strings
  sanitized via `gpui_add_project_dialog_error_message` (control chars stripped,
  bounded length, fixed transport copy); breadcrumb `gpui.addProject.request`
  `{ operation }` only; remote allowlist protocol.rs:466-467; entry points
  rewired (sidebar-app.tsx:5175/:5309/:5571, command palette app-modal
  `commandId:"addProject"`, V2 create menu). Web host 60s `withTimeout` on
  addProject/startClone (add-project-modal-host.tsx:49).
- Mobile round-2 fix re-verified in code (`FailureCopy.outdatedForFeature`,
  `isOutdatedMachineFailure` in inventory/client.ts:110 used by
  summarizeFailure:154 and addProject/client.ts:153) AND live on the iOS
  simulator ("Ghostex Test 2", app reconnected to the running Metro): Projects
  context menu → Add Project → Source screen shows the SHORT two-line banner
  ("This machine's Ghostex is too old for this feature. …"), Local folder +
  Git URL enabled, four provider rows disabled with "Provider status unavailable
  on this machine." + SETUP REQUIRED; Local screen shows banner → PROJECT PATH
  `~/` → "Add project" → BROWSE FOLDERS "No folders here."; Git URL Repository
  screen shows placeholder `https://github.com/org/repo.git` + Continue.
  Navigated back to Sessions and restored the Projects section state; nothing
  was added or mutated on the machine.

### Defects found (round 3)

None. Zero blocker/major/minor.

### Known gaps (carried, unchanged)

- Mobile Destination screen + real mobile add/clone/lookup still unexercised
  live (machine's installed CLI predates Part A; simulator text entry not
  drivable in background); covered by Part E's logic checks + the fix round's
  11 assertions.
- User's daemon 58744 + installed /opt/homebrew/bin/ghostex predate Part A;
  provider rows degrade gracefully until rebuilt/reinstalled.
- Remote projects on web don't render rows (pre-existing, Part D handoff).
- `activeProjectCwd` unsupplied in gpui; `onOpenSourceControlSettings` unwired
  in gpui/web (documented Part B/C/D gaps).
- All verified work is uncommitted working-tree state — commit promptly so
  concurrent agents cannot clobber it.

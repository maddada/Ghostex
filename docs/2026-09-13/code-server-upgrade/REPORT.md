# code-server upgrade assessment

Updating the fork is manageable, but it is more than changing a submodule pin. The application integration is small; the runtime and packaging changes need a real build and an editor smoke check.

## Isolated checkout

- Copy: `/Users/madda/dev/_active/ghostex-3`, created with macOS `cp -cR` (copy-on-write), including the current working files and Git repositories.
- Ghostex branch: `chore/code-server-4.136.2`.
- code-server and nested VS Code branches: `ghostex-upgrade-4.136.2`.
- All source work and candidate builds use the copied checkout. The normal app was reopened during the editor-isolation repair; its source and installed bundle were not replaced.
- The copy inherited unrelated pending changes and two merge conflicts: `packages/core-ui/styles/shadcn.generated.css` and `skills/ghostex-help/references/features.md`. Those are preserved. Publication uses a separate clean worktree at `/Users/madda/dev/_active/ghostex-3-pr`, branch `feat/code-server-upgrade-view-controls`, based on `main`, with only this task's changes and required formatting.

## Versions and revisions

| Component | Before | Candidate |
| --- | --- | --- |
| code-server | 4.119.0 source base, fork `78c92fee8d9a` | 4.136.2, local merge `08e12f53da81` |
| VS Code | 1.119.0, fork `f835649e079` | 1.136.1, local fork `68d14fc03be8` |
| Bundled Node | 22.22.1 | 24.18.1 |

The old code-server upstream base is `53d981a724a55e90f980dd8b147ba0028a956c24`. Its pinned upstream VS Code revision is `8b640eef5a6c6089c029249d48efa5c99adf7d51`.

The target is the [official code-server 4.136.2 release](https://github.com/coder/code-server/releases/tag/v4.136.2), released September 8, 2026, with Code 1.136.1. The new upstream VS Code pin is `a44adf7f53e00964ab890f9f8758a334f1fc15bc`. This release also fixes a data-directory regression in the withdrawn 4.136.1 release.

## What is actually custom

The code-server fork has 12 commits beyond its upstream base, affecting 20 files (996 additions, 88 deletions). Both dependency checkouts were clean before this work: there were no extra uncommitted code-server or VS Code edits to recover.

The nested VS Code fork initially appears to change 61 files. Applying the old upstream code-server patch series to its exact upstream VS Code pin and comparing the result with the fork isolates only **10 Ghostex-specific files**. Eight are runtime source files; the other two are product and Copilot build metadata. Copilot packaging functions and most browser/server integration changes are already standard code-server patches.

| Customization | Where it lives | Upgrade treatment |
| --- | --- | --- |
| Link local VS Code settings, keybindings, snippets, MCP, and tasks | code-server `vscodeUserConfig.ts`, CLI and startup | Preserved |
| Persist embedded GitHub/OAuth secrets through authenticated server storage | code-server `routes/vscode.ts`; VS Code workbench secret provider | Preserved, including origin/auth checks and owner-only storage |
| Correct remote grammar, icon, theme and other resource URLs | VS Code network/product configuration and web client server | Preserved alongside upstream's new app-name configuration |
| Correct VS Code version metadata in development | VS Code product and web client configuration | Preserved; source version advanced to 1.136.1 |
| Close loose-file editor tabs through IPC | VS Code browser host service | Preserved |
| Report readiness only after editor sockets listen | code-server app/health/socket manager; VS Code CLI and extension host | Preserved |
| Queue file opens until the matching workspace is ready, including files outside the workspace | code-server CLI, entry, main and socket manager | Preserved |
| Cross-architecture esbuild, TypeScript and ripgrep packaging; build cleanup and GitHub download authentication | code-server build script and REH patch | Preserved; TypeScript package names updated for TypeScript 7 |
| code-server product branding and stable Copilot build metadata | VS Code `product.json`, Copilot `package.json` | Preserved without overwriting new upstream metadata |
| Fetch the nested source from the Ghostex VS Code fork | code-server `.gitmodules` | Preserved |

### Original fork commits

```text
87b6c41f feat: add Ghostex embedded VS Code settings bridge
fba21bd4 Update embedded VS Code fork
3eb110b1 Fix REH ripgrep packaging for Ghostex
e7972640 Fix Copilot esbuild packaging for cross-arch builds
67af424e Fix TypeScript native packaging for cross-arch builds
5d114108 Fix VS Code esbuild packaging for cross-arch builds
a5a24b9b Apply REH ripgrep patch during VS Code packaging
4f7eae38 fix(build): harden Ghostex VS Code runtime packaging
6b4cfff1 fix(code-server): point VS Code submodule at Ghostex fork
390f119a feat: report prompt editor IPC readiness
9157ff6b feat(cli): queue file opens until the matching workbench is ready
78c92fee feat(cli): deliver queued file opens to a named workspace folder
```

## Candidate changes

1. Merged official code-server 4.136.2 into the existing fork.
2. Rebuilt the nested VS Code integration from its new official pin and the new upstream code-server patches, then reapplied the isolated Ghostex delta.
3. Moved the Ghostex-only VS Code changes out of the modified upstream `base-path.diff` into [ghostex-runtime.diff](../../../.dependencies/code-server/patches/ghostex-runtime.diff). Upstream patches can now be compared verbatim. The existing `reh-ripgrep-bin.diff` remains a separate build patch.
4. Updated the native compiler helper from `@typescript/native-preview` and its platform packages to the current `@typescript/native` alias and `@typescript/typescript-*` platform packages. A real build exposed this incompatibility.
5. Updated macOS Node checksums/defaults to 24.18.1. Both macOS CI workflows now read the dependency's `.node-version` instead of hard-coding Node 22.

There were four code-server merge conflicts (the nested pin, two patch files, and the secret-storage route). Reapplying the isolated VS Code source changes caused one conflict where upstream added app-name fields next to Ghostex's resource/version metadata. Both behaviors were retained.

## Verification

- code-server `npm run build`: passed under Node 24.18.1.
- Full macOS ARM64 `build:vscode`: passed, including core TypeScript checking, built-in extension checking, Copilot compilation, minification and REH web packaging.
- Ghostex component identity and archive validation: 35 tests passed.
- Patch replay: all 27 upstream patches plus `ghostex-runtime.diff` apply from the pristine new VS Code pin and reproduce the candidate source tree exactly. The separate REH ripgrep build patch also applies.
- Existing Linux ripgrep-validation release patch: still applies to the upgraded build script.
- Shell syntax and formatting of the intentionally edited code-server files: passed. Unified patch context is preserved rather than whitespace-formatted.
- Existing code-server unit suite, with a short macOS temporary path: 22 suites passed; 268 tests passed, one failed, and one suite could not compile. The CLI suite still assumes `makeEditorSessionManagerServer()` returns a raw server, although the pre-existing Ghostex fork returns `{ server, promptEditorIpcReady }`. The filesystem test expects Linux's `EISDIR`, while macOS reports `EPERM`. Both assumptions are present in the original fork; no tests were changed or added.
- The first unit run also hit macOS Unix-socket path-length limits. `TMPDIR=/tmp/gx3-code-server-test` removed those additional failures.
- Local dependency installation initially inherited `ignore-scripts=true` for VS Code's remote dependencies, leaving native modules absent. Rebuilding remote and Git-extension dependencies with command-scoped `npm_config_ignore_scripts=false` fixes that environment issue without changing user npm settings.

Final isolated browser smoke check: passed against the final `08e12f53da81` build in headless Chrome. A CLI request queued before any workbench existed opened a file outside the requested workspace after the extension host became ready. An Explorer file also opened with the expected contents. `/healthz` reported `promptEditorIpcReady: true`; a synthetic secret round-trip passed and its backing file had mode `0600`. No browser page errors occurred. The temporary server and browser were stopped afterwards.

The server logs still contain 404s for optional `vsda` signature-verification resources. They did not prevent management/extension-host connections or the smoke checks, but this is not a claim of warning-free operation.

Build and diagnostic logs are under `build/code-server-upgrade-audit/` in the copied checkout. This report is included in the PR; raw build logs and local patches remain untracked audit artifacts.

## Publication and validation scope

The VS Code commit is published to `maddada/vscode` and the code-server merge to
`maddada/code-server`, both on `ghostex-upgrade-4.136.2`. The parent PR pins that
published code-server commit. The nested VS Code source retains only three
pre-existing generated Mermaid output files as untracked artifacts.

macOS ARM64 is the locally built target. Linux and other architectures require
their release builds. Real GitHub credentials were not exercised by the editor
smoke check.

## Launching Ghostex-3 separately

From `/Users/madda/dev/_active/ghostex-3`:

```sh
bun run start:isolated
```

This builds and installs `/Users/madda/Applications/Ghostex-3.app`. Once installed, open that app from Finder to launch it without rebuilding. Its bundle retains the isolated environment for Finder and Dock launches.

| Resource | Ghostex-3 |
| --- | --- |
| macOS bundle ID | `com.madda.ghostex.gpui.ghostex-3` |
| Configuration, data, state, caches and logs | `/Users/madda/.local/share/ghostex-3` |
| Local gxserver API | `http://127.0.0.1:58747` |
| Embedded Code HTTP port | `3778` (normal Ghostex keeps `3777`) |
| code-server configuration | `/Users/madda/.local/share/ghostex-3/code-server-runtime-gpui/config.yaml` |
| CEF debugging port | `9337` |
| gxserver launchd label | `com.madda.ghostex-isolated.58747.gxserver` |
| Terminal launchd jobs | `com.madda.ghostex-isolated.58747.zmx.*` |

Ghostex-3 starts with fresh settings and an empty session/project database. Its hook scripts link individually to the existing scripts in `/Users/madda/.local/share/ghostex/hooks`. Hook updates atomically replace the isolated file link, leaving the original script intact. Provider logins remain available through the real user home; `HOME` is not changed.

Use `bun run gx:isolated sessions` for the CLI against this instance. The isolated app skips automatic repair of the global `ghostex` and `gx` wrappers. Skill installation and startup skill refresh use the isolated provider configuration root. Both the app and server use port 58747; shutdown targets only that instance's launchd namespace. The separate app does not register the production URL scheme.

The ordinary `bun run start` command retains its production behavior. Use `start:isolated` when testing this checkout alongside your main app.

The full bundle build also caught and corrected the Node checksum pins: the packager downloads `.tar.xz`, so the hashes now match the `.tar.xz` entries in [Node's official checksums](https://nodejs.org/dist/v24.18.1/SHASUMS256.txt). The missing copied `@tanstack/react-virtual` package was restored with `bun install --frozen-lockfile`; the dependency versions and lockfile were unchanged.

Validation: the final `bun run start:isolated` completed, including runtime preparation, desktop and server release builds, frontend packaging, installation, signature verification and launch. Eight existing storage tests and five existing agent-skill tests pass. Ghostex-3 has its own visible window, and its authenticated server health endpoint on port 58747 responds successfully. `bun run gx:isolated sessions` connects and reports no running terminal sessions.

The installed VS Code reports 1.136.1 and its product metadata records the upgraded code-server commit `08e12f53da813bb05af525b52844d6970045e643`. As with the source build, the outer code-server package version remains the upstream development placeholder `0.0.0`.

The original app PID 12837 and server PID 12974 survived the complete build and launch. All six checked production files (settings, launchd definition, hook scripts and public CLI commands) retained their original hashes. The isolated gxserver launchd definition contains its private home and port. The profile's hook scripts remain symlinks to the original scripts.

A fresh profile may show the hook onboarding prompt even though the original global agent registrations remain available. Choose Skip for now to keep those registrations. No production hook installation was performed.

## Editor isolation repair after user testing

The initial isolation check missed the embedded editor's HTTP port and code-server's own configuration. Both apps used port 3777. When Ghostex-3 owned that listener, normal Ghostex loaded its workbench, producing a settings error that referenced the Ghostex-3 user-data path. This was an isolation defect introduced by the separate-launch work, not a missing upstream patch.

The corrected bundle uses port 3778 for Ghostex-3's embedded editor. The port is used consistently for spawn, health/readiness probes, workbench origins and the Resources panel. A new launch refuses an occupied editor port instead of treating another process's health response as its own readiness.

The desktop settings bridge does not link the real VS Code profile when GHOSTEX_HOME selects an isolated instance. The launcher converts the four existing settings/keybindings/snippets/MCP symlinks into private copies, preserving their contents. Crucially, the bundle also persists CODE_SERVER_CONFIG pointing to a private YAML file: the shared ~/.config/code-server/config.yaml contained link-vscode-user-config: true, which would otherwise recreate the links even after the desktop stopped passing the flag.

The fixed bundle was rebuilt, signed, installed and launched successfully. A process inspection confirmed its Code launch used 127.0.0.1:3778 without the settings-link arguments. After the private YAML correction, all four copied configuration entries remained private. Normal Ghostex's window was reopened from the existing /Applications installation, and its own code-server launched on 3777 with the original user-data directory. Its gxserver PID 21502 stayed running. The normal editor's health/readiness endpoint passed. Interactive settings-save verification was left to the user because the UI tool detected ongoing user interaction; no successful settings-save claim is made.

## Resources crash and titlebar view controls

The reproduced Code-stop crash was a GPUI render-context violation:
`run_resource_secondary_action` called `Window::request_animation_frame` from a
mouse handler. That API reads the current rendered view, but no view is on the
render stack during input dispatch, so it panicked and aborted the app. The
handler now uses `cx.notify()`. Both individual and bulk Resources Code-stop
buttons call the same Sleep operation as the titlebar.

Web-based view buttons now offer Reload, Sleep, a separator, and Extensions.
Actions retain the clicked view identity. Reload uses CEF's page reload for an
existing view, or selects and wakes a sleeping view. Browser reload targets its
focused tab; Browser sleep unloads the current project's pages while retaining
its tab models. Code sleep stops its server. Other built-in, extension, and
custom views release their page and wake through the existing activation path.
Project-command views retain their existing command actions too.

The clean integration checkout passed 54 existing release and archive tests
with Vitest, the root, desktop, and web TypeScript checks, and a desktop release
`cargo check`. The isolated full app build passed. No tests were added; existing
source-based checks were retargeted after the required file splits. The first
run with Bun's test runner mixed Vitest mocks between suites; rerunning these
Vitest suites with their intended runner passed all 54 tests.

Required maintenance moved existing panel renderers, workarea constants,
code-server runtime and packaging helpers, and local-start utilities into
smaller modules. The moved function bodies are unchanged. The repository
formatting pass also normalized six existing server files.

# Windows code signing (SignPath)

Ghostex's Windows packages are Authenticode-signed by [SignPath](https://about.signpath.io/) through the SignPath Foundation open-source program, organization **Ghostex [OSS]**. Signing runs inside the Windows release job (`.github/workflows/release-gpui-windows.yml`); no certificate or private key ever reaches GitHub.

## What gets signed, and when

A signed build splits packaging (`tooling/release-gpui/windows.ps1`) into four phases with a SignPath signing request after each of the first three. Each request uploads one GitHub Actions artifact (a zip) and downloads the signed copy.

| Round | Workflow artifact | Files signed | Why at this point |
| --- | --- | --- | --- |
| 1 | `signpath-windows-<arch>-binaries` | Every PE file `vpk pack` signs: `Ghostex.exe`, `ghostex-gpui-runtime.exe`, `ghostex-gpui-cef-helper.exe`, `resources/GhostexEditor/GhostexEditor.exe`, `resources/native/gxserver.exe`, `ghostex.exe`, `wmx.exe` and any other `.exe`/`.dll`/`.node` of the app, plus Velopack's `Squirrel.exe` (the installed `Update.exe`) and the `Ghostex_ExecutionStub.exe` launcher it generates | These files go inside the update package, the delta and the portable zip, so they must be signed before the real `vpk pack`. A first, throwaway pack collects them (`tooling/release-gpui/vpk-signpath-hook.ps1`); the real pack swaps in the signed copies and re-seals `resources/build-identity.json`, whose fingerprint covers the native runtime binaries. |
| 2 | `signpath-windows-<arch>-velopack-setup` | `Ghostex-Velopack-Setup.exe`, the inner Velopack installer | It embeds the finished package, so it only exists after the real pack. |
| 3 | `signpath-windows-<arch>-installer` | `ghostex-<version>-windows-<arch>.exe`, the installer customers download (launcher + signed inner setup + trailer) | Signed last because bundling changes its bytes. |

The portable zip, the `.nupkg` update packages and the feed files are not signed themselves; they contain the files signed in round 1. The native editor under `code-server/` (code-server, VS Code server, Node.js and their native modules) is third-party code and is excluded from signing.

Without signing (no token, or `sign_windows` off) the job runs the single `package` phase and ships unsigned, exactly as before.

## Configuration the workflow uses

| What | Value | Where to change it |
| --- | --- | --- |
| Organization ID | `ba647e4a-b517-4fc1-b133-5817d7de0c68` | Repository variable `SIGNPATH_ORGANIZATION_ID` |
| Project slug | `Ghostex` (assumed; check it in SignPath) | Repository variable `SIGNPATH_PROJECT_SLUG` |
| Signing policy slug | `test-signing` (default) | Repository variable `SIGNPATH_SIGNING_POLICY_SLUG` |
| Artifact configuration slug | `initial` | Repository variable `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` |
| API token | the CI user's token | Repository secret `SIGNPATH_API_TOKEN` |

The variables are optional; the workflow falls back to the values above when a variable is not set. The artifact configuration source is `.signpath/artifact-configuration.xml`; SignPath does not read it from the repository, so paste it again whenever it changes.

When `SIGNPATH_API_TOKEN` is missing (forks, or before setup is finished) the Windows job logs a notice ("Windows build is unsigned") and packages unsigned instead of failing. `bun run release:actions` turns `sign_windows` on automatically once the secret exists (`--windows-signing auto`).

## Manual steps left in SignPath and GitHub

1. **Check the project slug.** In SignPath open *Projects > Ghostex* and read its slug. If it is not exactly `Ghostex`, set it: `gh variable set SIGNPATH_PROJECT_SLUG -R maddada/Ghostex --body '<slug>'`.
2. **Create the artifact configuration.** In *Projects > Ghostex > Artifact configurations*, add a new configuration with slug `initial`, paste the contents of `.signpath/artifact-configuration.xml`, save, and mark it as the project's default. If you pick another slug, set `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` to it.
3. **Let the CI user submit.** In *Projects > Ghostex > Signing policies*, open `test-signing` and `release-signing` and add the user **CI builds** as a *Submitter* on both.
4. **Create the API token and store it.** In *Users > CI builds*, generate an API token, then add it to GitHub: `gh secret set SIGNPATH_API_TOKEN -R maddada/Ghostex` (paste the token when prompted). Never put it in a file or commit.
5. **Link the GitHub.com trusted build system.** In *Trusted build systems*, make sure the predefined **GitHub.com** system is added to the organization and linked to the **Ghostex** project, and that both signing policies accept it (origin verification). Then install the [SignPath GitHub App](https://github.com/apps/signpath) on `maddada/Ghostex`. It is required, not optional: without it every signing step fails before a signing request is created, with "Failed to retrieve GitHub App token. Ensure that the GitHub App 'https://github.com/apps/signpath' is installed and not suspended" (release 10.25.1, run 39019284165). The workflow already grants `actions: read`, which the connector needs to read job details and download the artifact.
6. **Do a test-signed run.** Dispatch a release with `sign_windows` on (`bun run release:actions` does this once the secret exists). Each of the three *Sign … with SignPath* steps links to its signing request. Test-signed files carry a SignPath test certificate that Windows does not trust; the workflow only checks that a signature is present.

## Switching to release-signing

Once SignPath has reviewed the setup and imported the production certificate into the `release-signing` policy:

```bash
gh variable set SIGNPATH_SIGNING_POLICY_SLUG -R maddada/Ghostex --body release-signing
```

From then on the workflow also requires every signature to be fully valid (`Get-AuthenticodeSignature` status `Valid`), so a misconfigured policy fails the release instead of shipping a broken signature. SignPath Foundation's release-signing policy may need a manual approval per request; each signing step waits up to 30 minutes (`wait-for-completion-timeout-in-seconds: 1800`), so approve the three requests of each architecture while the job runs. To go back to test signing, delete the variable or set it to `test-signing`.

## Things to confirm on the first signed run

- The throwaway pack and the real pack must produce byte-identical `Squirrel.exe` and launcher stub files: the real pack finds each signed copy by the SHA-256 of the unsigned bytes and fails with "No SignPath-signed copy of …" if they differ.
- Each round should list its files as signed in the SignPath signing request. The artifact configuration names the two depths the zips use (root for rounds 2 and 3, one folder for round 1) with single-`*` patterns and no `**`, and every include is optional, so a request that matches nothing would not be rejected by SignPath; the release still fails in that case, because `windows.ps1` checks that every returned file carries a signature.

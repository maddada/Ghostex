# Windows code signing

Ghostex's Windows packages are Authenticode-signed inside the Windows release job (`.github/workflows/release-gpui-windows.yml`) by one of two signers:

- **Certum** (preferred): the user's own "Open Source Code Signing in the Cloud" certificate, used whenever the `CERTUM_SIMPLYSIGN_EMAIL` and `CERTUM_SIMPLYSIGN_TOTP` secrets exist.
- **SignPath** (fallback): the SignPath Foundation open-source program, used when only `SIGNPATH_API_TOKEN` exists.

With neither, the Windows job logs a notice ("Windows build is unsigned") and ships unsigned.

## Which builds are signed

`bun run release:actions` decides the `sign_windows` input with `--windows-signing` (`tooling/release-gpui/windows-signing.mjs`):

| Mode | Certum secrets set | Only `SIGNPATH_API_TOKEN` set | Nothing set |
| --- | --- | --- | --- |
| `auto` (default) | signed with Certum | signed only when `SIGNPATH_SIGNING_POLICY_SLUG` is `release-signing` | unsigned |
| `required` | signed with Certum | signed with the configured SignPath policy (test-signing by default) | the dispatch fails |
| `off` | unsigned | unsigned | unsigned |

The dispatcher prints the outcome and its reason, for example `Windows signing: enabled (Certum): the Certum SimplySign secrets are configured`. In the job, the step *Decide who signs this build* picks `certum`, `signpath` or `none` by the same order.

# Certum

## The certificate

| Field | Value |
| --- | --- |
| Product | Certum "Open Source Code Signing in the Cloud" (SimplySign), order ZoZE/026046/EU/07/11/2026 |
| Subject | `CN=Open Source Developer Mohamad Yahia Youssef, O=Open Source Developer, L=Ajman, ST=Ajman, C=AE` |
| Issuer | `Certum Code Signing 2021 CA` (Asseco Data Systems S.A.) |
| Serial | `0436c352e3cb5e9769206a31b40ce463` |
| SHA-1 thumbprint | `E81BEDC91DEA38C8BB2DA3651E161ADA280B6734` |
| Valid | 2026-11-09 to 2027-11-09 (RSA 3072, SHA-256) |
| Timestamp server | `http://time.certum.pl` (RFC 3161, SHA-256) |

The private key exists only in Certum's SimplySign cloud. Certum limits a certificate to 5000 signatures a month; a release signs a few dozen files.

## How the job signs

Certum offers no API or command-line login for this product. The supported way to use the key is **SimplySign Desktop**, which mounts it as a virtual smart card after you log in with the account e-mail and a one-time code from the SimplySign mobile app. The release does the same on GitHub's Windows runners, as other open-source projects do:

1. *Install SimplySign Desktop for Certum signing* downloads SimplySign Desktop 9.4.5.95 from `files.certum.eu`, checks its pinned SHA-256 and that Asseco signed it, and installs it silently (`tooling/release-gpui/certum-simplysign-login.ps1 -Install`).
2. *Log in to SimplySign for Certum signing* computes the current one-time code from the TOTP seed in `CERTUM_SIMPLYSIGN_TOTP` (RFC 6238; SimplySign uses HMAC-SHA256, 6 digits, 30 seconds), types the e-mail and code into the login window, and waits until the certificate with the configured thumbprint and its key appear in `Cert:\CurrentUser\My`. At most two attempts, so a wrong seed cannot lock the account. The seed and every code are masked and never printed. The x64 and ARM64 jobs log in at the same time to the same account, and Certum accepts each code only once, so x64 uses even and ARM64 odd 30-second windows. The script exports the x64 `signtool.exe` (also on ARM64, where SimplySign runs emulated and the ARM64 signtool cannot load its x64 smart-card driver).
3. *Stage and package the installer and update packages* runs `windows.ps1`'s single `package` phase with `GHOSTEX_WINDOWS_SIGNER=certum`, which signs in place with `tooling/release-gpui/certum-sign.ps1` (`signtool sign /sha1 <thumbprint> /fd sha256 /tr http://time.certum.pl /td sha256`):
   - every unsigned `.exe`/`.dll`/`.node` of the staged app (`Ghostex.exe`, `ghostex-gpui-runtime.exe`, `ghostex-gpui-cef-helper.exe`, `GhostexEditor.exe`, `gxserver.exe`, `wmx.exe`, ...) before `vpk pack`, then `resources/build-identity.json` is re-sealed because it fingerprints `gxserver.exe` and `wmx.exe`;
   - during `vpk pack`, through `--signTemplate`, the `Update.exe` (`Squirrel.exe`) and launcher stub Velopack generates;
   - the inner `Ghostex-Velopack-Setup.exe`, after its package is stored uncompressed (rewriting it later would break the signature);
   - the customer-facing `ghostex-<version>-windows-<arch>.exe`, after bundling.
   Every file must come out `Valid`, signed by the configured thumbprint and timestamped, or the release fails. The native editor under `code-server/` is third-party and is not signed, as with SignPath.
4. *Log out of SimplySign* stops SimplySign Desktop at the end of the job, also on failure.

A SimplySign session lasts about 2 hours, so the login runs right before the package step, the only one that signs.

## Secrets and variables

| Name | Kind | What goes in it |
| --- | --- | --- |
| `CERTUM_SIMPLYSIGN_EMAIL` | Repository secret | The SimplySign account e-mail. |
| `CERTUM_SIMPLYSIGN_TOTP` | Repository secret | The full `otpauth://totp/...` URI from the SimplySign activation QR code (preferred: it states the algorithm, digits and period), or just its base32 `secret=` value. |
| `CERTUM_CERT_SHA1` | Repository variable, optional | The certificate's SHA-1 thumbprint. Defaults to `E81BEDC91DEA38C8BB2DA3651E161ADA280B6734`; set it after a renewal or re-issue. |

Set the secrets from a terminal so they never land in a file or the shell history (each command prompts for the value):

```bash
gh secret set CERTUM_SIMPLYSIGN_EMAIL -R maddada/Ghostex
gh secret set CERTUM_SIMPLYSIGN_TOTP -R maddada/Ghostex
```

**The TOTP seed is as powerful as the certificate.** With it and the e-mail, anyone can sign code as Ghostex, without the phone, until a new QR code is issued. Only people and workflows with write access to the repository can read it (forks never get secrets), the same as the Apple and Android signing secrets. If it leaks, issue a new activation QR code at Certum, which replaces the seed (the community SimplySign clients document this as the way to rotate it; confirm with Certum). Hardening option: move both secrets into a GitHub environment limited to `main` and add `environment:` to the Windows build job.

## Getting the TOTP seed

The SimplySign app does not show its seed, so the seed comes from an activation QR code:

1. If you saved the original activation QR code, decode it (any QR reader, or `zbarimg <screenshot>`): the text is an `otpauth://totp/...` URI.
2. Otherwise get a new one through Certum's "regain access" flow: Certum e-mails an activation link and, separately, a "Secret for regaining access" code; open the link (valid 24 hours), enter the code, and the page shows a QR code. If no regain-access e-mail is available, request a new activation from Certum support.
3. Scan that QR code with the SimplySign app on your phone **and** take a screenshot of it, then decode the screenshot. Phone and CI then share the seed and both keep working. (Re-activating most likely replaces the previous seed, so scan the new code on the phone in the same sitting.)
4. Put the URI into `CERTUM_SIMPLYSIGN_TOTP` with `gh secret set` and delete the screenshot.

## Testing Certum signing on one architecture

`release-gpui-windows.yml` has a `sign` input for a standalone dispatch, so one architecture can be signed without a release: `gh workflow run release-gpui-windows.yml -R maddada/Ghostex --ref <branch> -f version=<version> -f arch=x64 -f sign=true`. A standalone run builds the native editor itself (slow) and still publishes its CEF and code-server components to the components repository, like any Windows run; it publishes no Ghostex release. Check the *Log in to SimplySign* step and that the package step reports every file as `Signed ... with Certum`.

## Things to confirm on the first Certum run

- The login automation drives SimplySign Desktop's window with keystrokes. A new SimplySign version or an unexpected dialog can break it; the version is pinned, and a failed login fails the job (it never ships unsigned when Certum is configured). If it fails, re-run the job; the step log lists the certificates it saw.
- Both architectures log in to one account at once. If Certum ends the first session when the second logs in, signing fails on one architecture with an error from signtool; then the jobs need to sign one after the other.
- The ARM64 runner image (`windows-11-vs2026-arm`) has not run SimplySign yet. The script dismisses the first-run dialogs the ARM64 images are known to show.

# SignPath

## What SignPath signs, and when

A signed build splits packaging (`tooling/release-gpui/windows.ps1`) into four phases with a SignPath signing request after each of the first three. Each request uploads one GitHub Actions artifact (a zip) and downloads the signed copy.

| Round | Workflow artifact | Files signed | Why at this point |
| --- | --- | --- | --- |
| 1 | `signpath-windows-<arch>-binaries` | Every PE file `vpk pack` signs: `Ghostex.exe`, `ghostex-gpui-runtime.exe`, `ghostex-gpui-cef-helper.exe`, `resources/GhostexEditor/GhostexEditor.exe`, `resources/native/gxserver.exe`, `ghostex.exe`, `wmx.exe` and any other `.exe`/`.dll`/`.node` of the app, plus Velopack's `Squirrel.exe` (the installed `Update.exe`) and the `Ghostex_ExecutionStub.exe` launcher it generates | These files go inside the update package and the delta, so they must be signed before the real `vpk pack`. A first, throwaway pack collects them (`tooling/release-gpui/vpk-signpath-hook.ps1`); the real pack swaps in the signed copies and re-seals `resources/build-identity.json`, whose fingerprint covers the native runtime binaries. |
| 2 | `signpath-windows-<arch>-velopack-setup` | `Ghostex-Velopack-Setup.exe`, the inner Velopack installer | It embeds the finished package, so it only exists after the real pack. |
| 3 | `signpath-windows-<arch>-installer` | `ghostex-<version>-windows-<arch>.exe`, the installer customers download (launcher + signed inner setup + trailer) | Signed last because bundling changes its bytes. |

The `.nupkg` update packages and the feed files are not signed themselves; they contain the files signed in round 1. The native editor under `code-server/` (code-server, VS Code server, Node.js and their native modules) is third-party code and is excluded from signing.

Without SignPath (no token, Certum configured, or `sign_windows` off) the job runs the single `package` phase instead, signed with Certum or unsigned.

## Configuration the workflow uses

| What | Value | Where to change it |
| --- | --- | --- |
| Organization ID | `ba647e4a-b517-4fc1-b133-5817d7de0c68` | Repository variable `SIGNPATH_ORGANIZATION_ID` |
| Project slug | `Ghostex` (assumed; check it in SignPath) | Repository variable `SIGNPATH_PROJECT_SLUG` |
| Signing policy slug | `test-signing` (default) | Repository variable `SIGNPATH_SIGNING_POLICY_SLUG` |
| Artifact configuration slug | `initial` | Repository variable `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` |
| API token | the CI user's token | Repository secret `SIGNPATH_API_TOKEN` |

The variables are optional; the workflow falls back to the values above when a variable is not set. The artifact configuration source is `.signpath/artifact-configuration.xml`; SignPath does not read it from the repository, so paste it again whenever it changes.

When the Certum secrets exist, Certum signs and the SignPath settings are not used. When `SIGNPATH_API_TOKEN` is missing as well (forks, or before setup is finished) the Windows job logs a notice ("Windows build is unsigned") and packages unsigned instead of failing.

**SignPath signing is off until the production certificate is imported** (user decision 2026-10-19, "Turn signing off for now"). Without the Certum secrets, `bun run release:actions` (`--windows-signing auto`) turns `sign_windows` on only when the SignPath secret exists **and** `SIGNPATH_SIGNING_POLICY_SLUG` is `release-signing`, so with the variable unset every release ships Windows unsigned. Test-signing every release added about 10 minutes to each Windows job for a certificate Windows does not trust. `--windows-signing required` still signs one dispatch with the configured policy (test-signing by default), and `--windows-signing off` never signs.

## Manual steps left in SignPath and GitHub

1. **Check the project slug.** In SignPath open *Projects > Ghostex* and read its slug. If it is not exactly `Ghostex`, set it: `gh variable set SIGNPATH_PROJECT_SLUG -R maddada/Ghostex --body '<slug>'`.
2. **Create the artifact configuration.** In *Projects > Ghostex > Artifact configurations*, add a new configuration with slug `initial`, paste the contents of `.signpath/artifact-configuration.xml`, save, and mark it as the project's default. If you pick another slug, set `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` to it.
3. **Let the CI user submit.** In *Projects > Ghostex > Signing policies*, open `test-signing` and `release-signing` and add the user **CI builds** as a *Submitter* on both.
4. **Create the API token and store it.** In *Users > CI builds*, generate an API token, then add it to GitHub: `gh secret set SIGNPATH_API_TOKEN -R maddada/Ghostex` (paste the token when prompted). Never put it in a file or commit.
5. **Link the GitHub.com trusted build system.** In *Trusted build systems*, make sure the predefined **GitHub.com** system is added to the organization and linked to the **Ghostex** project, and that both signing policies accept it (origin verification). Then install the [SignPath GitHub App](https://github.com/apps/signpath) on `maddada/Ghostex`. It is required, not optional: without it every signing step fails before a signing request is created, with "Failed to retrieve GitHub App token. Ensure that the GitHub App 'https://github.com/apps/signpath' is installed and not suspended" (release 10.25.1, run 39019284165). The workflow already grants `actions: read`, which the connector needs to read job details and download the artifact.
6. **Do a test-signed run.** Dispatch a release with `sign_windows` on (`bun run release:actions <version> --windows-signing required`; `auto` no longer test-signs). Each of the three *Sign … with SignPath* steps links to its signing request. Test-signed files carry a SignPath test certificate that Windows does not trust; the workflow only checks that a signature is present.

## Switching to release-signing

Once SignPath has reviewed the setup and imported the production certificate into the `release-signing` policy:

```bash
gh variable set SIGNPATH_SIGNING_POLICY_SLUG -R maddada/Ghostex --body release-signing
```

That one variable also turns signing back on: from the next `bun run release:actions`, `auto` signs every Windows build again. From then on the workflow also requires every signature to be fully valid (`Get-AuthenticodeSignature` status `Valid`), so a misconfigured policy fails the release instead of shipping a broken signature. SignPath Foundation's release-signing policy may need a manual approval per request; each signing step waits up to 30 minutes (`wait-for-completion-timeout-in-seconds: 1800`), so approve the three requests of each architecture while the job runs. Deleting the variable or setting it to `test-signing` turns `auto` signing off again; test signing is then only `--windows-signing required`.

## Things to confirm on the first SignPath-signed run

- The throwaway pack and the real pack must produce byte-identical `Squirrel.exe` and launcher stub files: the real pack finds each signed copy by the SHA-256 of the unsigned bytes and fails with "No SignPath-signed copy of …" if they differ.
- Each round should list its files as signed in the SignPath signing request. The artifact configuration names the two depths the zips use (root for rounds 2 and 3, one folder for round 1) with single-`*` patterns and no `**`, and every include is optional, so a request that matches nothing would not be rejected by SignPath; the release still fails in that case, because `windows.ps1` checks that every returned file carries a signature.

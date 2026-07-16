# GPUI multi-platform release

The canonical GPUI release is dispatched with:

```bash
bun run release:gpui -- 6.0.1
```

The command starts `.github/workflows/release-gpui.yml`. The workflow validates
that `package.json` and `CHANGELOG.md` already contain the requested version,
then fans out independent macOS, Debian x64, Fedora x64, Windows x64, Windows
ARM64, Android, Linux gxserver x64/ARM64, and Windows WSL bootstrap builds.
Each job uploads a manifest containing the exact filename, byte size, and
SHA-256 of every artifact. The publish job downloads and re-hashes all enabled
artifacts before it creates the tag and GitHub release.

Platforms can be disabled without editing workflow code:

```bash
bun run release:gpui -- 6.0.1 --disable-linux-rpm --disable-windows-arm64
```

Nightly prereleases can include the notarized macOS build without advancing the
production Sparkle feed:

```bash
bun run release:gpui -- 6.0.1 --prerelease --skip-sparkle
```

`--skip-windows-signing` is intended only for explicitly labeled nightlies when
no Authenticode certificate is configured; production releases remain signed
by default.

The platform scripts can also be run directly when debugging a runner:

```bash
scripts/release-gpui/macos.sh 6.0.1
scripts/release-gpui/linux-deb.sh 6.0.1
scripts/release-gpui/linux-rpm.sh 6.0.1
scripts/release-gpui/android.sh 6.0.1
pwsh scripts/release-gpui/windows.ps1 -Version 6.0.1 -Arch x64
```

## Published artifacts

- macOS arm64: `ghostex-<version>-arm64.dmg`, plus the existing three sealed
  on-demand assets. This is GPUI packaged as `Ghostex.app` with bundle ID
  `com.madda.ghostex.host` and the primary `appcast.xml`, so it replaces the
  installed Swift-host app through Sparkle.
- Linux x64: `ghostex_<version>_amd64.deb` and
  `ghostex-<version>-1.x86_64.rpm`.
- Windows x64 and ARM64: an NSIS installer EXE and a portable ZIP per
  architecture. The portable archive is required because CEF cannot operate
  as a standalone executable without its companion DLL and resource files;
  each package also carries the matching Linux gxserver runtime for WSL2.
- Android: signed universal `ghostex-android.apk`.
- Linux gxserver: `gxserver-linux-x64.tar.gz` and
  `gxserver-linux-arm64.tar.gz` static runtime archives.
- Windows WSL bootstraps: `gxserver-wsl-windows-x64.zip` and
  `gxserver-wsl-windows-arm64.zip`, each containing a checksum-pinned Linux
  runtime and PowerShell installer for an initialized WSL2 distribution.

The Sparkle feed is pushed to `main` only after the notarized DMG and all other
enabled assets are live. This prevents installed Ghostex copies from observing
an update whose enclosure is not downloadable yet.

## Required GitHub Actions secrets

macOS:

- `APPLE_DEVELOPER_ID_P12_BASE64`
- `APPLE_DEVELOPER_ID_P12_PASSWORD`
- `APPLE_KEYCHAIN_PASSWORD`
- `APPLE_NOTARY_KEY_BASE64`
- `APPLE_NOTARY_KEY_ID`
- `APPLE_NOTARY_ISSUER_ID`
- `SPARKLE_PRIVATE_KEY`

Instead of the three `APPLE_NOTARY_KEY_*` secrets, notarization can use the
existing Apple ID profile contract:

- `APPLE_NOTARY_APPLE_ID`
- `APPLE_NOTARY_TEAM_ID`
- `APPLE_NOTARY_APP_PASSWORD`

Android:

- `ANDROID_RELEASE_KEYSTORE_BASE64`
- `ANDROID_RELEASE_STORE_PASSWORD`
- `ANDROID_RELEASE_KEY_ALIAS`
- `ANDROID_RELEASE_KEY_PASSWORD`

Windows:

- `WINDOWS_CODE_SIGN_PFX_BASE64`
- `WINDOWS_CODE_SIGN_PFX_PASSWORD`

The P12, App Store Connect `.p8`, Android keystore, and Sparkle private key must
never be committed. Store their base64 forms as repository or environment
secrets. The workflow reconstructs them only under the ephemeral runner's
temporary directory.

## Release preparation

Before dispatching:

1. Set `package.json` to the new semver.
2. Add the matching `CHANGELOG.md` section.
3. Commit and push all release-bound work to `main`.
4. Confirm the tag and GitHub release do not already exist.
5. Configure the required secrets and allow GitHub Actions read/write contents
   permission so the final job can push the release tag and generated appcast.

The old `release:local` path remains historical recovery machinery. New GPUI
desktop releases should use `release:gpui` so every OS build is isolated on its
native GitHub-hosted runner.

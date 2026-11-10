# `ghostex-bin`: the Ghostex AUR package our release pushes

This directory is the upstream source for the [`ghostex-bin`](https://aur.archlinux.org/packages/ghostex-bin)
package on the Arch User Repository. Arch users install Ghostex with:

```sh
yay -S ghostex-bin      # or: paru -S ghostex-bin
```

## What is in here

| File                  | Purpose                                                                                            |
| --------------------- | -------------------------------------------------------------------------------------------------- |
| `PKGBUILD.template`   | The near-final PKGBUILD. `@PKGVER@`, `@SHA256@`, and `@LICENSE_SHA256@` are the only placeholders. |
| `ghostex-bin.install` | pacman scriptlet shown on install/upgrade (the first-launch browser-runtime download note).        |
| `README.md`           | This file.                                                                                         |

`tooling/release-gpui/publish-aur.mjs` renders `PKGBUILD.template` into a final
`PKGBUILD`, then **derives `.SRCINFO` by parsing that rendered PKGBUILD**. There
is no checked-in `.SRCINFO`, and no second copy of the metadata, so the two can
never drift. `makepkg --printsrcinfo` is not available on a macOS workstation or
on an `ubuntu-24.04` GitHub runner, which is why the `.SRCINFO` is generated in
JavaScript instead. The field order and the tab indentation it emits mirror
`srcinfo_write_global()` in pacman's `scripts/libmakepkg/srcinfo.sh.in`.

The published AUR repository therefore holds four files: `PKGBUILD`, `.SRCINFO`,
`ghostex-bin.install`, and `LICENSE`. Arch expects MIT license text to be
installed into `/usr/share/licenses/$pkgname/`, and
the release tarball carries none, so the publisher copies the Ghostex
repository's root `LICENSE` into the AUR repo as a local `source` entry and
hashes it at render time. There is deliberately no second copy of it under
`tooling/packaging/`.

## What the package installs

`ghostex-bin` repackages the official `ghostex-${pkgver}-linux-x64.tar.zst`
release asset. That tarball is a prefix-preserving tree, and the Ghostex
binaries hardcode `/opt/ghostex`, so `package()` copies it into `$pkgdir`
verbatim rather than relocating anything:

```
/opt/ghostex/...                                     the application payload
/usr/bin/ghostex                                     wrapper script
/usr/bin/gx                                          symlink -> ghostex
/usr/share/applications/ghostex.desktop
/usr/share/icons/hicolor/256x256/apps/ghostex.png
```

Ghostex does **not** bundle Chromium. The first GUI launch downloads the ~346 MB
CEF browser runtime into the user's cache directory. That is why the package is
comparatively small and why `ghostex-bin.install` prints a note about it.

## One-time setup (by hand, once, before automation works)

`ghostex-bin` was created on the AUR by Alper Ortac (AUR user `Alp`), who kept
it up to date by hand until 11.2.x. The release pushes it as a co-maintainer
(user decision Q2160, 10 Nov 2026), so these steps are done once:

1. **An AUR account for Ghostex.** Sign in at <https://aur.archlinux.org/login>
   (or use "Forgot password" there to find out whether an account exists), or
   register one at <https://aur.archlinux.org/register>.
2. **A dedicated SSH key, private half straight into the repository secret.**
   This never shows the private key, and nothing is left on disk:

   ```sh
   d="$(mktemp -d)" && ssh-keygen -q -t ed25519 -N "" -C "ghostex-aur-release-bot" -f "$d/key" && gh secret set AUR_SSH_PRIVATE_KEY --repo maddada/Ghostex <"$d/key" && pbcopy <"$d/key.pub" && cat "$d/key.pub"; rm -rf "$d"
   ```

   It prints the public half and copies it to the clipboard.
3. **The public half on the AUR account.** _My Account_ → _SSH Public Key_,
   paste, enter the current password, _Update_.
4. **Co-maintainer access.** Alp adds the Ghostex AUR user on the package page
   (<https://aur.archlinux.org/packages/ghostex-bin>, _Manage Co-Maintainers_).
   Until then every push is refused and the release goes on without it.

As set up on 10 Nov 2026, steps 1 and 4 were skipped: Alp added the public half
(`ghostex-aur-release-bot`) as a second line of the _SSH Public Key_ field of
his own AUR account, which maintains `ghostex-bin`, so the release pushes as
Alp. If he ever removes that line, every release's AUR step is refused again.

## How the automated bump works

`.github/workflows/release-gpui-publish.yml` runs two AUR steps at the end of
the `publish` job of the Linux x64 stage, right after that stage has uploaded
the Linux assets, so the AUR has the release within a minute or two. They are
skipped, with a log line explaining why, unless all of these hold:

- the publish is **not** a prerelease (`inputs.prerelease != true`): the AUR
  tracks stable releases only;
- the published release actually contains `ghostex-${version}-linux-x64.tar.zst`;
- that release is the repository's latest (a recovery of an older release
  must not downgrade Arch users);
- the `AUR_SSH_PRIVATE_KEY` secret is set.

When it runs, it installs the key, pins the `aur.archlinux.org` host keys into
`known_hosts` (no blind `ssh-keyscan`), and runs the publisher with
`--publish`. The publisher clones `ghostex-bin` and reads the version the AUR
has now: if that is this version (a co-maintainer pushed it first) or newer,
it leaves the package alone. Otherwise it writes the rendered files, commits
"Update ghostex-bin to X.Y.Z-1" and pushes.

**It never fails or holds the release.** Both steps continue on error, the
push step stops after 5 minutes, each git call after 2, and SSH gives up on a
dead connection. The outcome is one line in the run's summary ("AUR
ghostex-bin: Pushed ghostex-bin X.Y.Z to the AUR.", "... already at X.Y.Z",
or "... was NOT updated: <reason>"), and a failure also adds a warning
annotation "AUR ghostex-bin not updated" to the run.

## Checking the key without publishing

The **AUR ghostex-bin dry run** workflow (`.github/workflows/release-aur-dry-run.yml`)
runs the release's AUR step with the same secret and SSH setup
(`tooling/release-gpui/aur-ssh-setup.sh`) but never commits or pushes:

```sh
gh workflow run release-aur-dry-run.yml --repo maddada/Ghostex        # latest release
gh workflow run release-aur-dry-run.yml --repo maddada/Ghostex -f version=11.3.0
```

It prints the key's fingerprint, the AUR's `list-repos` for the key's account,
clones `ghostex-bin` over SSH, proves write access with `git push --dry-run` of
the AUR's own tip (this reaches the AUR's maintainer check and sends nothing),
shows the diff a publish would push, and says what a publish would do. Then
Arch's `makepkg` checks that its `.SRCINFO` matches ours and that every
`sha256sums` entry verifies against the real release asset.

## Running the publisher manually

Render only (no network writes, nothing pushed):

```sh
node tooling/release-gpui/publish-aur.mjs --version 7.7.1 --out /tmp/ghostex-aur
```

Everything a publish does except the commit and push (`--dry-run`), then the real push
(both need the AUR SSH key loaded locally):

```sh
node tooling/release-gpui/publish-aur.mjs --version 7.7.1 --dry-run
```


```sh
node tooling/release-gpui/publish-aur.mjs --version 7.7.1 --publish
```

### Where the sha256 comes from

The publisher resolves the tarball checksum in this order and prints which one
it used:

1. `--sha256 <hex>`: explicit override.
2. `--manifest <path>`: the `manifest.json` written by the release pipeline
   into `build/release-gpui/${version}/linux-tar-x64/`.
3. The GitHub release asset's own `sha256:` digest, read with `gh release view`.
4. Downloading the release asset and hashing it.

Options 3 and 4 also assert that the release exists, is not a draft, and
actually carries the Linux tarball.

## Updating the package metadata

Edit `PKGBUILD.template` (or `ghostex-bin.install`) and commit it here. The next
release picks the change up automatically; there is nothing to sync by hand.
Two rules:

- Keep `pkgver=@PKGVER@` and the `sha256sums` entry as `@SHA256@`; the renderer
  errors out on any unrendered `@PLACEHOLDER@` it finds.
- Keep the template to plain `key=value` and `key=(...)` lines above the
  `package()` function. The `.SRCINFO` generator parses those directly and
  refuses any field it does not recognise, rather than silently omitting it.

### Where `depends` comes from

`depends` is the `Depends:` list of the official `.deb`
(`tooling/release-gpui/linux-deb.sh`), translated to the Arch package that owns
each shared library. Keep the two in step when either changes.

| Debian package                                       | Arch package                                                                               |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `libasound2`                                         | `alsa-lib`                                                                                 |
| `libatk-bridge2.0-0`, `libatk1.0-0`, `libatspi2.0-0` | `at-spi2-core` (Arch merged `atk` and `at-spi2-atk` into it)                               |
| `libc6`                                              | `glibc`                                                                                    |
| `libcairo2`                                          | `cairo`                                                                                    |
| `libcups2`                                           | `libcups` (not `cups`)                                                                     |
| `libdbus-1-3`                                        | `dbus` (a short-lived `libdbus` split was folded back in)                                  |
| `libdrm2`                                            | `libdrm`                                                                                   |
| `libexpat1`                                          | `expat`                                                                                    |
| `libfontconfig1`                                     | `fontconfig`                                                                               |
| `libgbm1`                                            | `mesa`                                                                                     |
| `libglib2.0-0`                                       | `glib2`                                                                                    |
| `libgtk-3-0`                                         | `gtk3`                                                                                     |
| `libnspr4`                                           | `nspr`                                                                                     |
| `libnss3`                                            | `nss`                                                                                      |
| `libpango-1.0-0`, `libpangocairo-1.0-0`              | `pango`                                                                                    |
| `libx11-6`, `libx11-xcb1`                            | `libx11` (both sonames are in this one package)                                            |
| `libxcb1`                                            | `libxcb`                                                                                   |
| `libxcomposite1`                                     | `libxcomposite`                                                                            |
| `libxdamage1`                                        | `libxdamage`                                                                               |
| `libxext6`                                           | `libxext`                                                                                  |
| `libxfixes3`                                         | `libxfixes`                                                                                |
| `libxkbcommon0`                                      | `libxkbcommon`                                                                             |
| `libxrandr2`                                         | `libxrandr`                                                                                |
| `libxshmfence1`                                      | `libxshmfence`                                                                             |
| `wmctrl` (Recommends)                                | `wmctrl` as `optdepends` (in `extra`); only used to raise an already running window on X11 |
| (none)                                               | `hicolor-icon-theme` (owns the theme directory this package's icon goes into)              |

## Known caveats

- **`namcap` will report some `depends` as redundant.** `gtk3` transitively
  pulls in `cairo`, `glib2`, `pango`, `libx11`, `at-spi2-core`, and friends. The
  list is kept explicit anyway so a Ghostty or CEF change cannot silently drop a
  direct dependency; that trade-off is deliberate, not an oversight.
- **Package name.** The `-bin` suffix is correct here: the Ghostex source is
  public (MIT), so a source-built `ghostex` counterpart could exist, and every
  comparable package (`visual-studio-code-bin`, `cursor-bin`, `obsidian-bin`)
  follows the same convention. Renaming later means submitting a new AUR
  package and changing `pkgname` here plus `aurRemote` in `publish-aur.mjs`.
- **License identifier.** The repository's root `LICENSE` (the file this
  package installs) is MIT, so the template declares `license=('MIT')`. (The
  rpm spec's `License: Proprietary` label predates this and is stale.)
- **`arch=('x86_64')` only.** The Ghostex Linux desktop release is x64-only
  (`tooling/release-gpui/linux-stage.sh` refuses a non-`x86_64` runner). If an
  `aarch64` tarball ever ships, add the architecture plus `source_x86_64` /
  `source_aarch64` arrays; the `.SRCINFO` generator already emits arch-suffixed
  arrays in makepkg's grouped-per-architecture order.

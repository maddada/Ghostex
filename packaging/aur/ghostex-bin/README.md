# `ghostex-bin` — the vendor-maintained Ghostex AUR package

This directory is the upstream source for the [`ghostex-bin`](https://aur.archlinux.org/packages/ghostex-bin)
package on the Arch User Repository. Arch users install Ghostex with:

```sh
yay -S ghostex-bin      # or: paru -S ghostex-bin
```

## What is in here

| File | Purpose |
| --- | --- |
| `PKGBUILD.template` | The near-final PKGBUILD. `@PKGVER@`, `@SHA256@`, and `@LICENSE_SHA256@` are the only placeholders. |
| `ghostex-bin.install` | pacman scriptlet shown on install/upgrade (the first-launch browser-runtime download note). |
| `README.md` | This file. |

`scripts/release-gpui/publish-aur.mjs` renders `PKGBUILD.template` into a final
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
`packaging/`.

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

## One-time setup (must be done by hand, once, before automation works)

The release pipeline can only *update* an AUR package that already exists.
These four steps are manual and cannot be automated:

1. **Create an AUR account** at <https://aur.archlinux.org/register>.
2. **Register an SSH key on that account.** Generate a dedicated key for CI —
   do not reuse a personal key:

   ```sh
   ssh-keygen -t ed25519 -C "ghostex-aur-release-bot" -f ~/.ssh/ghostex_aur_ed25519 -N ""
   ```

   Paste the contents of `~/.ssh/ghostex_aur_ed25519.pub` into *My Account →
   SSH Public Key* on the AUR.

3. **Submit the initial `ghostex-bin` repository.** The AUR creates a package
   repo on first push; there is no web form.

   ```sh
   # Render the real files for the version you want to seed with.
   node scripts/release-gpui/publish-aur.mjs --version 7.7.1 --out /tmp/ghostex-aur

   git clone ssh://aur@aur.archlinux.org/ghostex-bin.git /tmp/ghostex-bin
   cp /tmp/ghostex-aur/PKGBUILD /tmp/ghostex-aur/.SRCINFO \
      /tmp/ghostex-aur/ghostex-bin.install /tmp/ghostex-aur/LICENSE /tmp/ghostex-bin/
   cd /tmp/ghostex-bin
   git add PKGBUILD .SRCINFO ghostex-bin.install LICENSE
   git commit -m "Initial import of ghostex-bin"
   git push origin HEAD:master
   ```

   The AUR's default branch is `master`, which is why the push is explicit.

   Before that first push, build it once on a real Arch machine to confirm the
   dependency list is complete:

   ```sh
   cd /tmp/ghostex-bin && makepkg -si
   namcap PKGBUILD ghostex-bin-*.pkg.tar.zst   # optional but recommended
   ```

4. **Add the `AUR_SSH_PRIVATE_KEY` GitHub secret.** In the `maddada/Ghostex`
   repository, go to *Settings → Secrets and variables → Actions → New
   repository secret*, name it `AUR_SSH_PRIVATE_KEY`, and paste the **private**
   key (`~/.ssh/ghostex_aur_ed25519`, the file *without* `.pub`), including the
   `-----BEGIN OPENSSH PRIVATE KEY-----` and `-----END …-----` lines and the
   trailing newline.

Until step 4 is done, the release workflow logs `AUR_SSH_PRIVATE_KEY is not
configured; skipping the AUR update` and succeeds — it never fails the release.

## How the automated bump works

`.github/workflows/release-gpui-publish.yml` runs an AUR step at the end of the
`publish` job, after the GitHub release itself is live. It is skipped, with a
log line explaining why, unless all of these hold:

- the publish is **not** a prerelease (`inputs.prerelease != true`) — the AUR
  tracks stable releases only;
- the published release actually contains `ghostex-${version}-linux-x64.tar.zst`;
- the `AUR_SSH_PRIVATE_KEY` secret is set.

When it runs, it installs the key, pins the `aur.archlinux.org` host keys into
`known_hosts` (no blind `ssh-keyscan`), and runs the publisher with
`--publish`. The publisher clones `ghostex-bin`, writes the rendered files,
and pushes. If the AUR is already at that exact version and checksum it prints
a no-op message and exits 0, so re-running a publish is safe.

## Running the publisher manually

Render only (no network writes, nothing pushed):

```sh
node scripts/release-gpui/publish-aur.mjs --version 7.7.1 --out /tmp/ghostex-aur
```

Render and push to the AUR (requires the AUR SSH key to be loaded locally):

```sh
node scripts/release-gpui/publish-aur.mjs --version 7.7.1 --publish
```

### Where the sha256 comes from

The publisher resolves the tarball checksum in this order and prints which one
it used:

1. `--sha256 <hex>` — explicit override.
2. `--manifest <path>` — the `manifest.json` written by the release pipeline
   into `build/release-gpui/${version}/linux-tar-x64/`.
3. The GitHub release asset's own `sha256:` digest, read with `gh release view`.
4. Downloading the release asset and hashing it.

Options 3 and 4 also assert that the release exists, is not a draft, and
actually carries the Linux tarball.

## Updating the package metadata

Edit `PKGBUILD.template` (or `ghostex-bin.install`) and commit it here. The next
release picks the change up automatically — there is nothing to sync by hand.
Two rules:

- Keep `pkgver=@PKGVER@` and the `sha256sums` entry as `@SHA256@`; the renderer
  errors out on any unrendered `@PLACEHOLDER@` it finds.
- Keep the template to plain `key=value` and `key=(...)` lines above the
  `package()` function. The `.SRCINFO` generator parses those directly and
  refuses any field it does not recognise, rather than silently omitting it.

### Where `depends` comes from

`depends` is the `Depends:` list of the official `.deb`
(`scripts/release-gpui/linux-deb.sh`), translated to the Arch package that owns
each shared library. Keep the two in step when either changes.

| Debian package | Arch package |
| --- | --- |
| `libasound2` | `alsa-lib` |
| `libatk-bridge2.0-0`, `libatk1.0-0`, `libatspi2.0-0` | `at-spi2-core` (Arch merged `atk` and `at-spi2-atk` into it) |
| `libc6` | `glibc` |
| `libcairo2` | `cairo` |
| `libcups2` | `libcups` (not `cups`) |
| `libdbus-1-3` | `dbus` (a short-lived `libdbus` split was folded back in) |
| `libdrm2` | `libdrm` |
| `libexpat1` | `expat` |
| `libfontconfig1` | `fontconfig` |
| `libgbm1` | `mesa` |
| `libglib2.0-0` | `glib2` |
| `libgtk-3-0` | `gtk3` |
| `libnspr4` | `nspr` |
| `libnss3` | `nss` |
| `libpango-1.0-0`, `libpangocairo-1.0-0` | `pango` |
| `libx11-6`, `libx11-xcb1` | `libx11` (both sonames are in this one package) |
| `libxcb1` | `libxcb` |
| `libxcomposite1` | `libxcomposite` |
| `libxdamage1` | `libxdamage` |
| `libxext6` | `libxext` |
| `libxfixes3` | `libxfixes` |
| `libxkbcommon0` | `libxkbcommon` |
| `libxrandr2` | `libxrandr` |
| `libxshmfence1` | `libxshmfence` |
| `wmctrl` | `wmctrl` (in `extra`, not AUR-only) |
| — | `hicolor-icon-theme` (owns the theme directory this package's icon goes into) |

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
- **License identifier.** The repository's root `LICENSE` — the file this
  package installs — is MIT, so the template declares `license=('MIT')`. (The
  rpm spec's `License: Proprietary` label predates this and is stale.)
- **`arch=('x86_64')` only.** The Ghostex Linux desktop release is x64-only
  (`scripts/release-gpui/linux-stage.sh` refuses a non-`x86_64` runner). If an
  `aarch64` tarball ever ships, add the architecture plus `source_x86_64` /
  `source_aarch64` arrays; the `.SRCINFO` generator already emits arch-suffixed
  arrays in makepkg's grouped-per-architecture order.

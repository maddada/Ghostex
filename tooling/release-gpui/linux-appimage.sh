#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
LINUX_ARCH="$(release_gpui_linux_arch)"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" "linux-appimage-$LINUX_ARCH")}"
release_gpui_require_version "$VERSION"
release_gpui_require_command mksquashfs
release_gpui_require_command unsquashfs
release_gpui_require_command tar
release_gpui_require_command curl
release_gpui_require_command node
release_gpui_require_command zsyncmake
release_gpui_require_command appstreamcli
release_gpui_require_command patchelf
release_gpui_require_command readelf
release_gpui_require_command ldd
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

# CDXC:Release 2026-10-26 WHY:
# The AppImage runtime (the ELF header that mounts the image) is pinned by
# release and digest instead of fetched through appimagetool's "continuous"
# channel, so every release embeds bytes we have checked. It is the static
# type2 runtime: it needs no libfuse on the user's computer, only the kernel's
# FUSE and fusermount; without them it stops with a message naming
# --appimage-extract-and-run, which works because AppRun copies the payload
# into the user's data folder either way.
APPIMAGE_RUNTIME_RELEASE="20251108"
case "$LINUX_ARCH" in
x64)
	APPIMAGE_RUNTIME_NAME="runtime-x86_64"
	APPIMAGE_RUNTIME_SHA256="2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d"
	;;
arm64)
	APPIMAGE_RUNTIME_NAME="runtime-aarch64"
	APPIMAGE_RUNTIME_SHA256="00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444"
	;;
esac

# CDXC:Release 2026-10-27 WHY:
# AppImage managers find updates through this update information (embedded in
# the runtime's .upd_info section by appimage-update-info.mjs) and the .zsync
# file published next to each image; AppManager showed an empty Update Link
# without them (GitHub issue #320). Each architecture names only its own zsync
# file, so an x64 install never "updates" to the ARM64 image.
APPIMAGE_UPDATE_INFORMATION="gh-releases-zsync|maddada|Ghostex|latest|ghostex-*-linux-${LINUX_ARCH}.AppImage.zsync"
APPIMAGE_HOMEPAGE="https://ghostex.dev"
APPSTREAM_ID="dev.ghostex.ghostex"
RELEASE_DATE="$(awk -v heading="## $VERSION - " 'index($0, heading) == 1 { print substr($0, length(heading) + 1); exit }' "$REPO_ROOT/CHANGELOG.md")"
[[ "$RELEASE_DATE" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || {
	echo "CHANGELOG.md has no '## $VERSION - YYYY-MM-DD' heading for the AppStream release entry" >&2
	exit 1
}

# CDXC:Release 2026-10-26: shared staged root, see linux-deb.sh. The AppImage
# wraps opt/ghostex and the desktop entry and icon; usr/bin is left out because
# the app installs its own `ghostex` and `gx` commands into ~/.local/bin on
# first start (gpui_auto_install_ghostex_cli_wrappers), pointing at the copy
# AppRun keeps in the user's data folder.
PACKAGE_ROOT="${GHOSTEX_LINUX_PACKAGE_ROOT:-$REPO_ROOT/build/release-gpui/linux-appimage-package-root}"
if [[ "${GHOSTEX_LINUX_PACKAGE_ROOT_READY:-0}" != "1" ]]; then
	"$SCRIPT_DIR/linux-stage.sh" "$VERSION" "$PACKAGE_ROOT"
fi
for required in opt/ghostex/Ghostex usr/share/applications/ghostex.desktop usr/share/icons/hicolor/256x256/apps/ghostex.png; do
	[[ -e "$PACKAGE_ROOT/$required" ]] || {
		echo "Linux package root is not staged: $PACKAGE_ROOT/$required is missing" >&2
		exit 1
	}
done

WORK="$REPO_ROOT/build/release-gpui/linux-appimage-work-$LINUX_ARCH"
rm -rf "$WORK"
mkdir -p "$WORK"
trap 'rm -rf "$WORK"' EXIT
APPDIR="$WORK/Ghostex.AppDir"
mkdir -p "$APPDIR/opt" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
	"$APPDIR/usr/share/metainfo"
cp -a "$PACKAGE_ROOT/opt/ghostex" "$APPDIR/opt/ghostex"
# Libraries the app links that a clean host may lack (libxkbcommon-x11 on the
# catalog's Ubuntu 22.04), beside the executables; see the script for the list.
node "$SCRIPT_DIR/appimage-bundle-libraries.mjs" --payload "$APPDIR/opt/ghostex" --arch "$LINUX_ARCH"
cp "$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps/ghostex.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/ghostex.png"
cp "$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps/ghostex.png" "$APPDIR/ghostex.png"
ln -s ghostex.png "$APPDIR/.DirIcon"
# The packaged entry runs /usr/bin/ghostex, which an AppImage does not have.
# Desktop integration tools (AppImageLauncher, Gear Lever, appimaged) replace
# Exec with the AppImage's own path when they install this entry.
sed -e 's|^Exec=.*|Exec=ghostex %U|' \
	"$PACKAGE_ROOT/usr/share/applications/ghostex.desktop" >"$APPDIR/ghostex.desktop"
# X-AppImage-Homepage is what AppManager shows as the Web Page.
printf 'X-AppImage-Version=%s\nX-AppImage-Homepage=%s\n' "$VERSION" "$APPIMAGE_HOMEPAGE" >>"$APPDIR/ghostex.desktop"
cp "$APPDIR/ghostex.desktop" "$APPDIR/usr/share/applications/ghostex.desktop"
# AppStream metadata: software centers and AppImage catalogs read the homepage,
# description and release from it. No vcs-browser URL: the AppStream 0.15
# validator on the Ubuntu 22.04 build runner rejects that type as a warning,
# and the bugtracker URL already leads to the repository.
METAINFO="$APPDIR/usr/share/metainfo/$APPSTREAM_ID.metainfo.xml"
cat >"$METAINFO" <<METAINFO_XML
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>$APPSTREAM_ID</id>
  <name>Ghostex</name>
  <summary>AI development workspaces and terminals</summary>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>MIT</project_license>
  <developer id="dev.ghostex">
    <name>Mohamad Yahia</name>
  </developer>
  <description>
    <p>Ghostex runs your coding agents side by side, each with its own terminal and chat, next to the projects, worktrees, browser and files they work on.</p>
  </description>
  <launchable type="desktop-id">ghostex.desktop</launchable>
  <url type="homepage">$APPIMAGE_HOMEPAGE</url>
  <url type="bugtracker">https://github.com/maddada/Ghostex/issues</url>
  <categories>
    <category>Development</category>
  </categories>
  <provides>
    <binary>ghostex</binary>
  </provides>
  <content_rating type="oars-1.1"/>
  <releases>
    <release version="$VERSION" date="$RELEASE_DATE">
      <url type="details">https://github.com/maddada/Ghostex/releases/tag/v$VERSION</url>
    </release>
  </releases>
</component>
METAINFO_XML
appstreamcli validate --no-net --explain "$METAINFO"
install -m 755 "$REPO_ROOT/tooling/packaging/appimage/AppRun" "$APPDIR/AppRun"

# The payload id AppRun compares with the copy in the user's data folder: the
# version plus a digest of the payload, so a rebuilt image of the same version
# still replaces a stale copy.
PAYLOAD_DIGEST="$(
	cd "$APPDIR/opt/ghostex"
	tar --sort=name --mtime=@946684800 --owner=0 --group=0 --numeric-owner -cf - . | sha256sum | awk '{ print $1 }'
)"
printf '%s-%s\n' "$VERSION" "${PAYLOAD_DIGEST:0:16}" >"$APPDIR/.ghostex-appimage"

RUNTIME="$WORK/$APPIMAGE_RUNTIME_NAME"
if [[ -n "${GHOSTEX_APPIMAGE_RUNTIME_FILE:-}" ]]; then
	cp "$GHOSTEX_APPIMAGE_RUNTIME_FILE" "$RUNTIME"
else
	"$SCRIPT_DIR/retry.sh" 4 5 -- curl -fsSL --output "$RUNTIME" \
		"https://github.com/AppImage/type2-runtime/releases/download/$APPIMAGE_RUNTIME_RELEASE/$APPIMAGE_RUNTIME_NAME"
fi
[[ "$(release_gpui_sha256 "$RUNTIME")" == "$APPIMAGE_RUNTIME_SHA256" ]] || {
	echo "AppImage runtime $APPIMAGE_RUNTIME_RELEASE/$APPIMAGE_RUNTIME_NAME does not match its pinned sha256" >&2
	exit 1
}
node "$SCRIPT_DIR/appimage-update-info.mjs" write "$RUNTIME" "$APPIMAGE_UPDATE_INFORMATION"

# Deterministic image: root-owned entries, one timestamp, sorted by mksquashfs.
# zstd is what the type2 runtime reads fastest; 1 MiB blocks hold the payload
# close to the tarball's size.
SQUASHFS="$WORK/payload.squashfs"
mksquashfs "$APPDIR" "$SQUASHFS" \
	-root-owned -noappend -no-xattrs -no-progress -quiet \
	-comp zstd -Xcompression-level 19 -b 1M \
	-mkfs-time 946684800 -all-time 946684800
APPIMAGE="$OUTPUT/ghostex-${VERSION}-linux-${LINUX_ARCH}.AppImage"
cat "$RUNTIME" "$SQUASHFS" >"$APPIMAGE"
chmod 755 "$APPIMAGE"

# CDXC:Release 2026-10-27 DECISION:
# User, on signing the AppImages (GitHub issue #320): "do it man ... just do it urself". Every release AppImage is signed with the Ghostex AppImage Signing key (RSA 4096, no expiry: AppImageUpdate only accepts an update signed by the same key as the installed image, so an expiring key would force a rotation that breaks updates). The private key and passphrase are the GHOSTEX_APPIMAGE_SIGNING_KEY and GHOSTEX_APPIMAGE_SIGNING_PASSPHRASE repository secrets (the key itself lives in the maintainer's GPG keyring, its passphrase in their login Keychain); the public key is committed beside AppRun and embedded in every image. Without the secrets (a fork, a local build) the image is built unsigned with a log line. Signing comes before the zsync file, which describes the final bytes.
APPIMAGE_SIGNING_FINGERPRINT="093568F45FD8FFFED60AB4F94A5D4C4A8D2E45D3"
APPIMAGE_SIGNING_PUBLIC_KEY="$REPO_ROOT/tooling/packaging/appimage/ghostex-appimage-signing.asc"
if [[ -n "${GHOSTEX_APPIMAGE_SIGNING_KEY:-}" || -n "${GHOSTEX_APPIMAGE_SIGNING_PASSPHRASE:-}" ]]; then
	[[ -n "${GHOSTEX_APPIMAGE_SIGNING_KEY:-}" && -n "${GHOSTEX_APPIMAGE_SIGNING_PASSPHRASE:-}" ]] || {
		echo "AppImage signing needs both GHOSTEX_APPIMAGE_SIGNING_KEY and GHOSTEX_APPIMAGE_SIGNING_PASSPHRASE; only one is set" >&2
		exit 1
	}
	release_gpui_require_command gpg
	# Short keyring paths: gpg-agent's socket lives in the home folder on hosts
	# without /run/user, and a socket path over 107 bytes fails.
	SIGNING_ROOT="$(mktemp -d)"
	trap 'rm -rf "$WORK" "$SIGNING_ROOT"' EXIT
	SIGNING_HOME="$SIGNING_ROOT/sign"
	VERIFY_HOME="$SIGNING_ROOT/verify"
	mkdir -m 700 "$SIGNING_HOME" "$VERIFY_HOME"
	GNUPGHOME="$SIGNING_HOME" gpg --batch --quiet --pinentry-mode loopback --passphrase-fd 3 --import \
		3< <(printf '%s' "$GHOSTEX_APPIMAGE_SIGNING_PASSPHRASE") <<<"$GHOSTEX_APPIMAGE_SIGNING_KEY"
	GNUPGHOME="$SIGNING_HOME" gpg --batch --list-secret-keys --with-colons "$APPIMAGE_SIGNING_FINGERPRINT" >/dev/null || {
		echo "GHOSTEX_APPIMAGE_SIGNING_KEY is not the Ghostex AppImage Signing key $APPIMAGE_SIGNING_FINGERPRINT" >&2
		exit 1
	}
	# The same text appimagetool signs: the hex digest, as canonical text, armored.
	node "$SCRIPT_DIR/appimage-signature.mjs" digest "$APPIMAGE" >"$WORK/signed-digest.txt"
	GNUPGHOME="$SIGNING_HOME" gpg --batch --yes --pinentry-mode loopback --passphrase-fd 3 \
		--local-user "$APPIMAGE_SIGNING_FINGERPRINT!" --digest-algo SHA256 --armor --textmode \
		--detach-sign --output "$WORK/signature.asc" "$WORK/signed-digest.txt" \
		3< <(printf '%s' "$GHOSTEX_APPIMAGE_SIGNING_PASSPHRASE")
	node "$SCRIPT_DIR/appimage-signature.mjs" embed "$APPIMAGE" "$WORK/signature.asc" "$APPIMAGE_SIGNING_PUBLIC_KEY"
	# Check the embedded signature the way a user's tool does: re-hash the final
	# image and verify against the committed public key only.
	node "$SCRIPT_DIR/appimage-signature.mjs" digest "$APPIMAGE" >"$WORK/verify-digest.txt"
	cmp -s "$WORK/signed-digest.txt" "$WORK/verify-digest.txt" || {
		echo "Embedding the signature changed the signed digest" >&2
		exit 1
	}
	GNUPGHOME="$VERIFY_HOME" gpg --batch --quiet --import "$APPIMAGE_SIGNING_PUBLIC_KEY"
	GNUPGHOME="$VERIFY_HOME" gpg --batch --status-fd 1 --verify "$WORK/signature.asc" "$WORK/verify-digest.txt" 2>/dev/null |
		grep -q "^\[GNUPG:\] VALIDSIG $APPIMAGE_SIGNING_FINGERPRINT " || {
		echo "The AppImage signature does not verify against $APPIMAGE_SIGNING_PUBLIC_KEY" >&2
		exit 1
	}
	gpgconf --homedir "$SIGNING_HOME" --kill gpg-agent 2>/dev/null || true
	gpgconf --homedir "$VERIFY_HOME" --kill gpg-agent 2>/dev/null || true
	rm -rf "$SIGNING_ROOT"
	printf 'Signed %s with the Ghostex AppImage Signing key %s\n' "$(basename "$APPIMAGE")" "$APPIMAGE_SIGNING_FINGERPRINT"
else
	printf 'AppImage signing skipped: GHOSTEX_APPIMAGE_SIGNING_KEY and GHOSTEX_APPIMAGE_SIGNING_PASSPHRASE are not set (a fork or a local build); %s is unsigned\n' "$(basename "$APPIMAGE")"
fi

# The zsync file holds the image's SHA-1 and block checksums: managers compare
# that SHA-1 with the installed image to tell whether an update is out, and
# download only the blocks that changed. Its URL is the image's own name beside
# it on the release; one fixed mtime keeps it reproducible.
touch -d @946684800 "$APPIMAGE"
ZSYNC="$APPIMAGE.zsync"
zsyncmake -u "$(basename "$APPIMAGE")" -o "$ZSYNC" "$APPIMAGE"

# Read the image back the way the runtime does: the squashfs starts right after
# the runtime, and AppRun, the payload id, the entry and the icon are inside.
OFFSET="$(stat -c '%s' "$RUNTIME")"
MEMBERS="$(unsquashfs -o "$OFFSET" -lc "$APPIMAGE" | sed 's|^squashfs-root||')"
for required in /AppRun /.ghostex-appimage /ghostex.desktop /ghostex.png /.DirIcon \
	"/usr/share/metainfo/$APPSTREAM_ID.metainfo.xml" \
	/opt/ghostex/Ghostex /opt/ghostex/ghostex-gpui-runtime /opt/ghostex/gxserver/bin/gxserver \
	/opt/ghostex/gxserver/bin/ghostex /opt/ghostex/gxserver/bin/tailcat \
	/opt/ghostex/resources/on-demand-resources.json \
	/opt/ghostex/gxserver/skills/ghostex-cli/SKILL.md /opt/ghostex/libxkbcommon-x11.so.0; do
	grep -qxF "$required" <<<"$MEMBERS" || {
		echo "AppImage is missing $required" >&2
		exit 1
	}
done
if grep -q '^/opt/ghostex/libcef\.so$' <<<"$MEMBERS"; then
	echo "AppImage bundles libcef.so; CEF must stay an on-demand component" >&2
	exit 1
fi
EMBEDDED="$(node "$SCRIPT_DIR/appimage-update-info.mjs" read "$APPIMAGE")"
[[ "$EMBEDDED" == "$APPIMAGE_UPDATE_INFORMATION" ]] || {
	echo "AppImage carries update information '$EMBEDDED', not '$APPIMAGE_UPDATE_INFORMATION'" >&2
	exit 1
}
grep -q "^SHA-1: $(sha1sum "$APPIMAGE" | awk '{ print $1 }')$" "$ZSYNC" || {
	echo "$ZSYNC does not describe $APPIMAGE" >&2
	exit 1
}

release_gpui_write_manifest "$OUTPUT" "linux-appimage-$LINUX_ARCH" "$VERSION" "$APPIMAGE" "$ZSYNC"
printf 'Built Linux %s AppImage in %s\n' "$LINUX_ARCH" "$OUTPUT"

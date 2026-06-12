# Dual-Arch macOS Release Handover

<!--
CDXC:Distribution 2026-05-14-10:00:
Ghostex should support Intel Macs by publishing real x86_64 builds beside arm64 builds, not by relying on Rosetta or fallback launch behavior.
The release pipeline must keep GitHub Releases, Sparkle appcasts, and the Homebrew tap in sync for both architecture-specific DMGs.
-->

This handover is for the release/update agent that owns the ghostex/Ghostex GitHub, Sparkle, and Homebrew publishing flow.

## Goal

Publish native macOS builds for both Apple Silicon and Intel Macs:

- `ghostex-<version>-arm64.dmg`
- `ghostex-<version>-x86_64.dmg`

Keep the existing arm64 Sparkle feed working for installed Apple Silicon users, and add a separate Intel Sparkle feed so Intel clients never depend on an arm64-only appcast item.

## Current State

- The release skill is `.agents/skills/ghostex-release-to-brew/SKILL.md`.
- The public release flow currently builds one signed/notarized DMG, uploads it to GitHub Releases, regenerates `appcast.xml`, and updates `maddada/homebrew-tap`.
- `appcast.xml` currently advertises recent releases with `<sparkle:hardwareRequirements>arm64</sparkle:hardwareRequirements>`.
- `native/macos/ghostexHost/vendor-cef.sh` chooses CEF by `uname -m`, so Apple Silicon builders always vendor `macosarm64`.
- `native/macos/ghostexHost/build-ghostex-host.sh` tells developers to build GhosttyKit with `-Dxcframework-target=native`, which creates only the host architecture slice.
- The local Ghostty build code already has macOS `aarch64` and `x86_64` support through `GhosttyLib.initMacOSUniversal`.

## Required Architecture Model

Do not publish one mixed appcast containing same-version arm64 and x86_64 items until Sparkle selection behavior has been verified for this exact Sparkle version.
Sparkle supports `sparkle:hardwareRequirements` for requiring Apple Silicon, but there is no matching documented `x86_64` requirement to force Intel-only selection.
An x86_64-only app can be considered runnable on Apple Silicon through Rosetta, so a shared feed can accidentally offer the Intel build to Apple Silicon users if ordering or compatibility checks are wrong.

Use this model instead:

- Keep `appcast.xml` as the Apple Silicon feed for existing installs.
- Add `appcast-x86_64.xml` as the Intel feed.
- Build arm64 apps with `SUFeedURL=https://raw.githubusercontent.com/maddada/ghostex/main/appcast.xml`.
- Build x86_64 apps with `SUFeedURL=https://raw.githubusercontent.com/maddada/ghostex/main/appcast-x86_64.xml`.
- GitHub Releases can contain both DMGs under the same version tag.
- Homebrew can use one cask with an `arch` stanza and per-arch SHA256 values.

References:

- Sparkle documents `generate_appcast` as the supported way to generate signed appcasts and notes that edited appcasts/release notes must be regenerated or re-signed: https://sparkle-project.org/documentation/
- Sparkle documents `sparkle:hardwareRequirements` for Apple Silicon requirements: https://sparkle-project.org/documentation/publishing/
- Homebrew documents `arch`, per-architecture `sha256`, and architecture-substituted URLs for casks: https://docs.brew.sh/Cask-Cookbook

## Implementation Checklist

### 1. Make CEF Vendoring Target-Aware

Update `native/macos/ghostexHost/vendor-cef.sh` so release automation controls the target architecture explicitly.

Add:

```bash
GHOSTEX_MACOS_ARCH="${GHOSTEX_MACOS_ARCH:-$(uname -m)}"
case "$GHOSTEX_MACOS_ARCH" in
  arm64|aarch64)
    CEF_ARCH="macosarm64"
    CMAKE_ARCH="arm64"
    CLANG_ARCH="arm64"
    ;;
  x86_64|x64|amd64)
    CEF_ARCH="macosx64"
    CMAKE_ARCH="x86_64"
    CLANG_ARCH="x86_64"
    ;;
  *)
    echo "Unsupported GHOSTEX_MACOS_ARCH: $GHOSTEX_MACOS_ARCH" >&2
    exit 1
    ;;
esac
```

Change the default CEF root from a shared path to an arch-specific path:

```bash
CEF_ROOT="${CEF_ROOT:-$SCRIPT_DIR/Vendor/cef-$CMAKE_ARCH}"
```

Make the CEF cache version include the target architecture:

```bash
EXPECTED_VERSION="${CEF_VERSION}+chromium-${CHROMIUM_VERSION}+${CMAKE_ARCH}"
```

Make the helper output arch-specific:

```bash
local helper_build="$SCRIPT_DIR/build/cef-$CMAKE_ARCH"
local helper="$helper_build/ghostex-cef-helper"
```

Pass the target arch to both helper compile/link invocations:

```bash
xcrun --sdk macosx clang++ -arch "$CLANG_ARCH" ...
xcrun --sdk macosx clang++ -arch "$CLANG_ARCH" ...
```

Keep the CMake `PROJECT_ARCH` argument, and add explicit validation after building:

```bash
lipo -archs "$CEF_ROOT/Release/Chromium Embedded Framework.framework/Chromium Embedded Framework" | grep -Fx "$CMAKE_ARCH"
lipo -archs "$helper" | grep -Fx "$CMAKE_ARCH"
```

If CMake cannot cross-build the CEF wrapper from Apple Silicon to x86_64, use an Intel runner for the x86_64 leg rather than adding runtime fallbacks.

### 2. Make the Native Host Build Target-Aware

Update `native/macos/ghostexHost/build-ghostex-host.sh` to accept:

```bash
GHOSTEX_MACOS_ARCH="${GHOSTEX_MACOS_ARCH:-$(uname -m)}"
```

Validate the value as `arm64` or `x86_64`, then:

- Export `GHOSTEX_MACOS_ARCH` before calling `vendor-cef.sh`.
- Default `DERIVED_DATA` to an arch-specific path such as `$REPO_ROOT/build/$GHOSTEX_MACOS_ARCH`.
- Pass `ARCHS="$GHOSTEX_MACOS_ARCH"` and `ONLY_ACTIVE_ARCH=NO` to every `xcodebuild ... build` and `xcodebuild ... -showBuildSettings` call.
- Keep `CEF_ROOT` arch-specific.
- Write the final built app path to a deterministic file such as `/tmp/ghostex-$VERSION-$GHOSTEX_MACOS_ARCH-app-path` so packaging does not have to guess the product path.

Example build invocation after the script supports targeting:

```bash
env \
  CONFIGURATION=Release \
  GHOSTEX_MACOS_ARCH=arm64 \
  DERIVED_DATA="$PWD/build/arm64" \
  GHOSTEX_CODE_SIGN_TIMESTAMP_FLAG=--timestamp \
  native/macos/ghostexHost/build-ghostex-host.sh

env \
  CONFIGURATION=Release \
  GHOSTEX_MACOS_ARCH=x86_64 \
  DERIVED_DATA="$PWD/build/x86_64" \
  GHOSTEX_SPARKLE_FEED_URL="https://raw.githubusercontent.com/maddada/ghostex/main/appcast-x86_64.xml" \
  GHOSTEX_CODE_SIGN_TIMESTAMP_FLAG=--timestamp \
  native/macos/ghostexHost/build-ghostex-host.sh
```

Validate every Mach-O in the app bundle that can break architecture support:

```bash
APP_PATH="$(cat /tmp/ghostex-$VERSION-$ARCH-app-path)"
file "$APP_PATH/Contents/MacOS/"*
lipo -archs "$APP_PATH/Contents/MacOS/"*
lipo -archs "$APP_PATH/Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework"
find "$APP_PATH/Contents/Frameworks" -path '*/Contents/MacOS/*' -type f -print -exec lipo -archs {} \;
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
```

### 3. Prepare GhosttyKit For Both Architectures

The current developer command is native-only:

```bash
zig build -Demit-xcframework -Dxcframework-target=native -Demit-macos-app=false
```

For release support, first try:

```bash
cd "$GHOSTTY_ROOT"
env \
  DEVELOPER_DIR=/Library/Developer/CommandLineTools \
  SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk \
  GHOSTTY_METAL_DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  zig build -Demit-xcframework -Dxcframework-target=universal -Demit-macos-app=false
```

Then verify the macOS library contains both slices:

```bash
plutil -p "$GHOSTTY_ROOT/macos/GhosttyKit.xcframework/Info.plist"
find "$GHOSTTY_ROOT/macos/GhosttyKit.xcframework" \
  -name 'libghostty-internal-fat.a' \
  -exec lipo -archs {} \;
```

If `-Dxcframework-target=universal` fails because it also attempts iOS/iOS-simulator outputs that ghostex does not need, add a Ghostty build target such as `macos-universal` or `macos-x86_64` that only packages the macOS `aarch64`/`x86_64` static library.
Do not keep using `native` for public releases once Intel support is enabled.

### 4. Build, Sign, Notarize, And Package Both DMGs

Use separate temp directories per architecture:

```bash
VERSION=<version>
for ARCH in arm64 x86_64; do
  APP_PATH="$(cat "/tmp/ghostex-$VERSION-$ARCH-app-path")"
  FINAL_DIR="$(mktemp -d)"
  STAGING_DIR="$(mktemp -d)"
  FINAL_DMG="$FINAL_DIR/ghostex-$VERSION-$ARCH.dmg"

  cp -R "$APP_PATH" "$STAGING_DIR/ghostex.app"
  ln -s /Applications "$STAGING_DIR/Applications"
  hdiutil create -volname "ghostex" -srcfolder "$STAGING_DIR" -format UDZO "$FINAL_DMG"

  xcrun notarytool submit "$FINAL_DMG" --keychain-profile notarytool-profile --wait
  xcrun stapler staple "$FINAL_DMG"
  xcrun stapler validate "$FINAL_DMG"
  shasum -a 256 "$FINAL_DMG"
  printf '%s\n' "$FINAL_DMG" > "/tmp/ghostex-${VERSION//./}-$ARCH-final-dmg"
done
```

Keep the app bundle name inside both DMGs identical.
If the public cask still installs `app "ghostex.app"`, stage the app as `ghostex.app` even if the build product is branded `Ghostex.app`.
Change the cask app stanza only as part of a deliberate bundle-name migration.

Validate mounted DMGs for both architectures:

```bash
for ARCH in arm64 x86_64; do
  FINAL_DMG="$(cat "/tmp/ghostex-${VERSION//./}-$ARCH-final-dmg")"
  ATTACH_OUTPUT="$(hdiutil attach -nobrowse -readonly "$FINAL_DMG")"
  MOUNT_POINT="$(printf '%s\n' "$ATTACH_OUTPUT" | awk 'END {print $3}')"
  spctl --assess --type execute --verbose "$MOUNT_POINT/ghostex.app"
  codesign --verify --deep --strict --verbose=2 "$MOUNT_POINT/ghostex.app"
  lipo -archs "$MOUNT_POINT/ghostex.app/Contents/MacOS/"*
  plutil -p "$MOUNT_POINT/ghostex.app/Contents/Info.plist" | rg 'CFBundleShortVersionString|CFBundleVersion|CFBundleIdentifier|SUFeedURL|SUPublicEDKey'
  hdiutil detach "$MOUNT_POINT"
done
```

An actual Intel Mac smoke test is required before announcing Intel support.
At minimum, launch the app, open one terminal pane, open one Chromium browser pane, and verify the process architecture in Activity Monitor or with `file`/`lipo`.

### 5. Generate Sparkle Feeds

Keep two feeds:

- `appcast.xml`: Apple Silicon feed.
- `appcast-x86_64.xml`: Intel feed.

Generate/sign the arm64 feed from the arm64 DMG:

```bash
VERSION=<version>
BUILD_VERSION="$(perl -E 'my ($M,$m,$p)=split /\./, shift; say $M*10000+$m*100+$p' "$VERSION")"
SPARKLE_BIN_DIR="$(find "$PWD/build/SourcePackages/artifacts/sparkle" /tmp/ghostex-xcodebuild/SourcePackages/artifacts/sparkle "$HOME/Library/Developer/Xcode/DerivedData" -path '*/Sparkle/bin/generate_appcast' -print -quit 2>/dev/null | xargs dirname)"

ARM_DMG="$(cat "/tmp/ghostex-${VERSION//./}-arm64-final-dmg")"
ARM_WORK_DIR="$(mktemp -d)"
cp appcast.xml "$ARM_WORK_DIR/appcast.xml"
cp "$ARM_DMG" "$ARM_WORK_DIR/ghostex-$VERSION-arm64.dmg"
cat > "$ARM_WORK_DIR/ghostex-$VERSION-arm64.md" <<EOF
# ghostex $VERSION arm64

See https://github.com/maddada/ghostex/releases/tag/v$VERSION for release notes.
EOF
"$SPARKLE_BIN_DIR/generate_appcast" \
  --download-url-prefix "https://github.com/maddada/ghostex/releases/download/v$VERSION/" \
  --full-release-notes-url "https://github.com/maddada/ghostex/releases/tag/v$VERSION" \
  --maximum-versions 6 \
  -o "$ARM_WORK_DIR/appcast.xml" \
  "$ARM_WORK_DIR"
cp "$ARM_WORK_DIR/appcast.xml" appcast.xml
cp "$ARM_WORK_DIR/ghostex-$VERSION-arm64.md" .
"$SPARKLE_BIN_DIR/sign_update" --verify appcast.xml
xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])" appcast.xml | grep -Fx "$BUILD_VERSION"
xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='shortVersionString'])[1])" appcast.xml | grep -Fx "$VERSION"
rg "ghostex-$VERSION-arm64.dmg|hardwareRequirements|sparkle-signatures" appcast.xml --glob '!node_modules/**'
```

Generate/sign the x86_64 feed from the Intel DMG:

```bash
INTEL_DMG="$(cat "/tmp/ghostex-${VERSION//./}-x86_64-final-dmg")"
INTEL_WORK_DIR="$(mktemp -d)"
if [[ -f appcast-x86_64.xml ]]; then
  cp appcast-x86_64.xml "$INTEL_WORK_DIR/appcast-x86_64.xml"
fi
cp "$INTEL_DMG" "$INTEL_WORK_DIR/ghostex-$VERSION-x86_64.dmg"
cat > "$INTEL_WORK_DIR/ghostex-$VERSION-x86_64.md" <<EOF
# ghostex $VERSION x86_64

See https://github.com/maddada/ghostex/releases/tag/v$VERSION for release notes.
EOF
"$SPARKLE_BIN_DIR/generate_appcast" \
  --download-url-prefix "https://github.com/maddada/ghostex/releases/download/v$VERSION/" \
  --full-release-notes-url "https://github.com/maddada/ghostex/releases/tag/v$VERSION" \
  --maximum-versions 6 \
  -o "$INTEL_WORK_DIR/appcast-x86_64.xml" \
  "$INTEL_WORK_DIR"
cp "$INTEL_WORK_DIR/appcast-x86_64.xml" appcast-x86_64.xml
cp "$INTEL_WORK_DIR/ghostex-$VERSION-x86_64.md" .
"$SPARKLE_BIN_DIR/sign_update" --verify appcast-x86_64.xml
xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])" appcast-x86_64.xml | grep -Fx "$BUILD_VERSION"
xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='shortVersionString'])[1])" appcast-x86_64.xml | grep -Fx "$VERSION"
rg "ghostex-$VERSION-x86_64.dmg|hardwareRequirements|sparkle-signatures" appcast-x86_64.xml --glob '!node_modules/**'
! rg "sparkle:hardwareRequirements>arm64" appcast-x86_64.xml --glob '!node_modules/**'
```

Expected feed differences:

- `appcast.xml` latest item should point at `ghostex-<version>-arm64.dmg` and include `sparkle:hardwareRequirements>arm64`.
- `appcast-x86_64.xml` latest item should point at `ghostex-<version>-x86_64.dmg` and should not include `sparkle:hardwareRequirements>arm64`.
- Both feeds should have the same `sparkle:version`, `sparkle:shortVersionString`, release URL, and EdDSA-signed enclosure metadata.

Do not hand-edit either feed after signing.
If generated metadata is wrong, fix the build artifact and regenerate.

### 6. Commit Release Metadata

Commit source/config release metadata, not DMGs:

```bash
git add \
  package.json \
  native/macos/ghostexHost/project.yml \
  appcast.xml \
  appcast-x86_64.xml \
  "ghostex-$VERSION-arm64.md" \
  "ghostex-$VERSION-x86_64.md"
git commit -m "chore: bump version to $VERSION"
git tag "v$VERSION"
```

Push only when the release workflow has been authorized:

```bash
git push origin main
git push origin "v$VERSION"
```

### 7. Publish GitHub Release

Upload both stapled DMGs to the same tag:

```bash
ARM_DMG="$(cat "/tmp/ghostex-${VERSION//./}-arm64-final-dmg")"
INTEL_DMG="$(cat "/tmp/ghostex-${VERSION//./}-x86_64-final-dmg")"
ARM_SHA="$(shasum -a 256 "$ARM_DMG" | awk '{print $1}')"
INTEL_SHA="$(shasum -a 256 "$INTEL_DMG" | awk '{print $1}')"

gh release create "v$VERSION" \
  "$ARM_DMG" \
  "$INTEL_DMG" \
  --repo maddada/ghostex \
  --title "ghostex $VERSION" \
  --notes "Changes: <fill from CHANGELOG>

Apple Silicon DMG: ghostex-$VERSION-arm64.dmg
Apple Silicon SHA256: $ARM_SHA

Intel DMG: ghostex-$VERSION-x86_64.dmg
Intel SHA256: $INTEL_SHA

Install with Homebrew:
brew install --cask maddada/tap/ghostex"
```

Validate both assets:

```bash
curl -I -L --fail "https://github.com/maddada/ghostex/releases/download/v$VERSION/ghostex-$VERSION-arm64.dmg"
curl -I -L --fail "https://github.com/maddada/ghostex/releases/download/v$VERSION/ghostex-$VERSION-x86_64.dmg"
```

### 8. Update Homebrew Tap

Use one cask that selects the DMG by CPU architecture.

Example `Casks/ghostex.rb` shape:

```ruby
cask "ghostex" do
  arch arm: "arm64", intel: "x86_64"

  version "<version>"
  sha256 arm:   "<arm64-sha256>",
         intel: "<x86_64-sha256>"

  url "https://github.com/maddada/ghostex/releases/download/v#{version}/ghostex-#{version}-#{arch}.dmg"
  name "ghostex"
  desc "Workspace and session UI for agent terminals"
  homepage "https://github.com/maddada/ghostex"

  depends_on macos: :ventura

  app "ghostex.app"
  binary "#{appdir}/ghostex.app/Contents/Resources/CLI/ghostex"
  # CDXC:CliInstall 2026-06-07-13:53: Homebrew links the same app-owned CLI launchers that direct DMG installs auto-link on app startup.
  preflight do
    gx_candidates = [HOMEBREW_PREFIX/"bin/gx"]
    ENV.fetch("PATH", "").split(File::PATH_SEPARATOR).each do |entry|
      gx_candidates << Pathname(entry)/"gx" unless entry.empty?
    end

    gx_candidates.uniq.each do |gx_path|
      next unless gx_path.exist? || gx_path.symlink?

      gx_target = gx_path.symlink? ? gx_path.readlink.to_s : gx_path.to_s
      next if gx_target.include?("ghostex.app/Contents/Resources/CLI/gx")

      raise "Ghostex cannot install the gx CLI because #{gx_path} already exists. " \
            "Remove or rename the existing gx command, then reinstall Ghostex."
    end
  end
  binary "#{appdir}/ghostex.app/Contents/Resources/CLI/gx"

  zap trash: [
    "~/Library/Application Support/com.madda.ghostex.host",
    "~/Library/Preferences/com.madda.ghostex.host.plist",
    "~/Library/Saved Application State/com.madda.ghostex.host.savedState",
  ]
end
```

Validate:

```bash
ruby -c Casks/ghostex.rb
brew style Casks/ghostex.rb
brew fetch --cask ./Casks/ghostex.rb
brew fetch --cask --arch=arm64 ./Casks/ghostex.rb
brew fetch --cask --arch=x86_64 ./Casks/ghostex.rb
```

If `brew fetch --cask --arch=...` is not available in the local Homebrew version, validate on real arm64 and Intel machines.
If the tap also contains a `ghostex` cask or alias, update it in the same commit so both public install commands resolve to the same architecture-specific release.
Keep both `binary` stanzas in the cask so Homebrew installs the same `ghostex` command and `gx` short alias from `Contents/Resources/CLI` that direct DMG installs auto-link on app startup. Keep the `gx` preflight check so Homebrew fails clearly instead of taking over an existing non-Ghostex `gx` command.

Commit and push the tap only after both SHA values match the stapled DMGs:

```bash
git add Casks/ghostex.rb
git commit -m "Update ghostex cask to $VERSION"
git push origin main
```

### 9. Validate Live Sparkle Feeds

After GitHub assets and appcast commits are live:

```bash
curl -fsSL "https://raw.githubusercontent.com/maddada/ghostex/main/appcast.xml" -o /tmp/ghostex-appcast-arm64.xml
curl -fsSL "https://raw.githubusercontent.com/maddada/ghostex/main/appcast-x86_64.xml" -o /tmp/ghostex-appcast-x86_64.xml
xmllint --noout /tmp/ghostex-appcast-arm64.xml
xmllint --noout /tmp/ghostex-appcast-x86_64.xml
"$SPARKLE_BIN_DIR/sign_update" --verify /tmp/ghostex-appcast-arm64.xml
"$SPARKLE_BIN_DIR/sign_update" --verify /tmp/ghostex-appcast-x86_64.xml
rg "ghostex-$VERSION-arm64.dmg|sparkle:version|sparkle:shortVersionString|hardwareRequirements|sparkle-signatures" /tmp/ghostex-appcast-arm64.xml --glob '!node_modules/**'
rg "ghostex-$VERSION-x86_64.dmg|sparkle:version|sparkle:shortVersionString|hardwareRequirements|sparkle-signatures" /tmp/ghostex-appcast-x86_64.xml --glob '!node_modules/**'
```

Then validate enclosure URLs:

```bash
curl -I -L --fail "https://github.com/maddada/ghostex/releases/download/v$VERSION/ghostex-$VERSION-arm64.dmg"
curl -I -L --fail "https://github.com/maddada/ghostex/releases/download/v$VERSION/ghostex-$VERSION-x86_64.dmg"
```

### 10. Required Final Report

The release agent should report:

- App repo commit hashes and subjects.
- GitHub release URL.
- arm64 DMG SHA256.
- x86_64 DMG SHA256.
- Notary submission IDs and statuses for both DMGs.
- Sparkle feed commit SHA.
- Live Sparkle validation for `appcast.xml` and `appcast-x86_64.xml`.
- Homebrew tap commit SHA.
- `brew fetch` validation for both architectures.
- Intel Mac smoke-test result.
- Any known test limitation, especially the existing `vite-plus/test` limitation if it appears.

## Open Risks

- The x86_64 CEF wrapper may not cross-build cleanly on Apple Silicon. If it fails, add an Intel build host rather than adding runtime fallbacks.
- `GhosttyKit.xcframework` universal mode may include iOS outputs that ghostex does not need. If those fail, add a macOS-only universal or x86_64 GhosttyKit build target.
- The Intel build must be launched on real Intel hardware before public support is announced. CEF browser panes and embedded Ghostty rendering are the two highest-risk areas.
- Existing Apple Silicon installs use `appcast.xml`, so keep that feed valid and arm64-compatible during the migration.
- After implementation, update `.agents/skills/ghostex-release-to-brew/SKILL.md` so future release agents do not accidentally run the old single-architecture workflow.

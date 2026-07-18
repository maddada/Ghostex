<!--
CDXC:AndroidHandover 2026-05-18-06:55:
This handover records the current Ghostex Android implementation, SSHJ migration, package identity, build flow, and the next investigation target around spinner-driven terminal auto-scroll so another agent can continue without reconstructing the session history.
-->

# Ghostex Android Handover

## Current State

Ghostex Android is a Termux-based Android app that connects to persistent Ghostex sessions running on a macOS machine. The first release supports ZMX only. The Android app does not manage local Termux terminals as the product workflow; it uses Termux as the terminal surface and asks the Mac-side Ghostex CLI for session inventory and attach/action commands.

The Android runtime package id is now `io.ghostex`. This replaced the earlier `com.ghostx` debug id so Android package-visible surfaces spell Ghostex correctly. `io.ghostex` is intentionally the same byte length as `com.termux`, which keeps the current bootstrap binary-prefix patching valid.

The latest APK I built in this session was:

```text
http://100.77.81.4:8765/ghostex-debug.apk
package: io.ghostex
versionName: 0.1.0
versionCode: 144
targetSdkVersion: 35
compileSdkVersion: 36
```

Because the package id changed from `com.ghostx` to `io.ghostex`, a phone may show both apps until the old `com.ghostx` build is manually uninstalled.

## What Changed

- Created and evolved a separate Android submodule under `android/termux-app`.
- Kept the Termux Java namespace mostly intact for upstream sync, while adding Ghostex-specific modules under `com.termux.app.ghostex`.
- Built the Ghostex sidebar/drawer experience over Termux's terminal surface.
- Added saved-machine management, first-run tutorial, setup panel, Tailscale action, password handling, reconnect, and recovery flows.
- Added a warm session pool that keeps the last 7 tapped remote sessions alive per machine/session id.
- Added SSHJ as the app-owned SSH transport.
- Removed the phone-side OpenSSH/sshpass installer and preflight path once SSHJ attach was stable.
- Added a single shareable Android log file at `Downloads/ghostex/ghostex-android.log`.
- Changed the package id spelling from `com.ghostx` to `io.ghostex`.

## Main Architecture

### Controller and UI

- `android/termux-app/app/src/main/java/com/termux/app/ghostex/GhostexAndroidController.java`
  - Central coordinator for onboarding, saved machines, reconnect, drawer rendering, session attach, remote actions, file upload, tutorial/setup panels, and warm-session management.
  - Holds the warm session map: 7 remote attach terminals keyed by machine id plus Ghostex session id.
  - Uses generation counters to ignore stale async reconnect/action/check callbacks after machine switches or edits.

- `android/termux-app/app/src/main/java/com/termux/app/ghostex/GhostexRemoteSessionAdapter.java`
  - Renders project headers, session rows, state cards, and project action controls in the drawer.

- `android/termux-app/app/src/main/res/layout/view_ghostex_drawer.xml`
  - The actual Ghostex sidebar drawer layout.

- `android/termux-app/app/src/main/res/layout/activity_termux.xml`
  - TermuxActivity layout with Ghostex terminal surface controls, including keyboard/file/refresh floating buttons.

### SSH and Remote Commands

- `android/termux-app/app/src/main/java/com/termux/app/ghostex/GhostexSshTransport.java`
  - App-owned SSHJ transport for reconnect, inventory, readiness checks, remote actions, create/rename, and SFTP upload.
  - Uses SSHJ `AndroidConfig`, removes Curve25519/X25519 KEX factories, and replaces Android's platform `BC` provider with bundled BouncyCastle to avoid `X25519` and `EC` provider errors.
  - Persists accepted host-key fingerprints in app-owned SharedPreferences.
  - Host-key reset clears this SSHJ store, not Termux `known_hosts`.

- `android/termux-app/app/src/main/java/com/termux/app/ghostex/GhostexSshAttachProcess.java`
  - Bridges a remote SSHJ interactive PTY into Termux's `TerminalSession.ExternalTerminalProcess`.
  - Allocates a PTY, starts an SSH shell channel, then runs `ghostex attach --session-id ...` through the remote account's configured login shell (`$SHELL -lc`), preserving zsh behavior on macOS without requiring zsh on Linux.
  - Handles stdout/stdin logging and remote PTY resize.
  - Important previous bug fixed here: SSHJ `window-change` writes to the socket, and Android resize/zoom can happen on the UI thread. Resize is now queued to a single background executor and coalesced.

- `android/termux-app/app/src/main/java/com/termux/app/ghostex/GhostexSshCommandBuilder.java`
  - Builds Mac-side Ghostex CLI command strings.
  - Android itself uses SSHJ now. The copyable attach command remains plain `ssh -tt ...` for human support/debugging only.

- `android/termux-app/app/src/main/java/com/termux/app/ghostex/GhostexSessionInventoryClient.java`
  - Calls `ghostex sessions --json` and `ghostex android-check --json`.
  - Parses CLI JSON even if shell/profile output wraps it.

### Files and Upload

- `android/termux-app/app/src/main/java/com/termux/app/ghostex/GhostexFileUploadClient.java`
  - SFTP upload path for images/logs/files from Android to the Mac.
  - Pastes a markdown reference into the active terminal after upload.

### Logging

- `android/termux-app/app/src/main/java/com/termux/app/ghostex/GhostexFileLogger.java`
  - Single log file: `Downloads/ghostex/ghostex-android.log`.
  - Lines include timestamp, tag, and ZMX/session metadata.
  - It also cleans up older duplicate `ghostex-android*.log`/`.txt` MediaStore entries so the phone exposes one file.

### Package Identity

- `android/termux-app/app/build.gradle`
  - Default runtime package id: `io.ghostex`.
  - Builds patched bootstrap archives where `/data/data/com.termux/files/usr` is rewritten to `/data/data/io.ghostex/files/usr`.

- `android/termux-app/app/src/main/res/values/strings.xml`
  - Static package entity now uses `io.ghostex`.

- `android/termux-app/termux-shared/src/main/res/values/strings.xml`
  - Shared package/prefix strings now use `io.ghostex`.

- `android/termux-app/termux-shared/src/main/java/com/termux/shared/termux/TermuxConstants.java`
  - Shared runtime package constant now uses `io.ghostex`.

- `android/termux-app/tools/ghostex-android-adb.sh`
  - Device helper defaults to `io.ghostex`.

## Build and Verification

From `android/termux-app`:

```sh
GHOSTEX_ANDROID_VERSION_NAME=0.1.0 \
GHOSTEX_ANDROID_VERSION_CODE=145 \
TERMUX_SPLIT_APKS_FOR_DEBUG_BUILDS=0 \
./gradlew :app:assembleDebug
```

Copy the universal debug APK to the currently served URL:

```sh
cp -f app/build/outputs/apk/debug/ghostex-android_v0.1.0+145-apt-android-7-debug_universal.apk \
  app/build/outputs/apk/debug/ghostex-debug.apk
```

Useful verification:

```sh
./gradlew :app:compileDebugJavaWithJavac :app:testDebugUnitTest :app:compileDebugAndroidTestJavaWithJavac --continue
"$ANDROID_HOME/build-tools/36.0.0/aapt" dump badging app/build/outputs/apk/debug/ghostex-debug.apk | head -n 5
curl -I --max-time 3 http://127.0.0.1:8765/ghostex-debug.apk
```

## Current Known Issue: Spinner Output Forces Scroll to Bottom

The next issue to work on is:

> If the CLI tool running inside the attached terminal has an active spinner, Termux keeps scrolling down to the bottom of the screen.

The likely current path is in Termux's terminal view, not in SSHJ:

- `android/termux-app/terminal-view/src/main/java/com/termux/view/TerminalView.java`
  - `onScreenUpdated(boolean skipScrolling)` snaps `mTopRow` back to `0` whenever `skipScrolling` is false and `mTopRow != 0`.
  - Spinner/progress output creates frequent terminal updates, so any manual scrollback is repeatedly pulled back to bottom.
  - `updateSize()` also resets `mTopRow = 0`, which is correct for some resize cases but can be hostile during user-driven zoom/resize while reading scrollback.

The relevant code shape is:

```java
if (!skipScrolling && mTopRow != 0) {
    mTopRow = 0;
}
```

There is an existing emulator-level auto-scroll toggle:

- `android/termux-app/terminal-emulator/src/main/java/com/termux/terminal/TerminalEmulator.java`
  - `mAutoScrollDisabled`
  - `isAutoScrollDisabled()`
  - `toggleAutoScrollDisabled()`

- `android/termux-app/app/src/main/java/com/termux/app/terminal/io/TermuxTerminalExtraKeys.java`
  - Extra key `"SCROLL"` toggles auto-scroll disabled.

However, Ghostex's current floating controls/sidebar do not expose this as a clear product behavior, and the terminal view still defaults to auto-following output. A good fix should probably be automatic: if the user manually scrolls up, output should not snap to bottom until the user scrolls back to bottom or taps an explicit "jump to bottom" control.

Suggested next implementation:

1. Track user-initiated scrollback in `TerminalView`.
   - When `mTopRow < 0` because of touch/fling/mouse wheel scroll, mark a "user pinned scrollback" state.
   - While pinned, `onScreenUpdated(false)` should preserve `mTopRow` using the existing scroll counter logic instead of setting it to `0`.
   - Clear the pinned state when the user scrolls to bottom, attaches a new session, or explicitly taps a new jump-to-bottom control.

2. Be careful with alternate screen tools.
   - Many full-screen TUIs use the alternate screen and mouse tracking. Confirm whether spinner-heavy tools are in main buffer or alternate buffer before applying a global policy.
   - The existing `mAutoScrollDisabled` path already preserves view position with `mEmulator.getScrollCounter()`. Reusing that behavior may be safer than inventing a second preservation algorithm.

3. Add logs only if needed.
   - If diagnosing on-device, log `mTopRow`, `rowsInHistory`, `rowShift`, `isAutoScrollDisabled`, and whether the update came after user scroll or output.
   - Do not log terminal content.

4. Test manually with:
   - A long-running spinner command in the remote attached session.
   - Scroll up in history and wait while spinner keeps updating.
   - Verify it does not snap to bottom.
   - Scroll/tap back to bottom and verify normal follow-output resumes.
   - Repeat inside a full-screen CLI/TUI and after pinch zoom.

## Important Constraints

- Do not bring back phone-side OpenSSH/sshpass install/preflight code. SSHJ is the current transport.
- Do not change `io.ghostex` to a longer package id until Ghostex has rebuilt bootstrap/package artifacts for that prefix.
- Preserve upstream Termux files as much as possible. Prefer Ghostex wrapper modules and narrow, documented changes.
- Keep CDXC comments updated near behavior that encodes user requirements.
- Do not use broad git restore/reset commands. This repo has unrelated user/agent work in the tree.

## Useful User-Facing Behaviors to Preserve

- First launch shows a one-page scrollable tutorial, then asks for machine credentials.
- Multiple saved SSH machines are supported.
- Saved passwords use Android secure storage; unchecked passwords remain process-only.
- Reopen/resume tries the last selected machine, except normal resume does not reconnect while a warm Ghostex attach terminal is foregrounded.
- Sidebar mirrors macOS Ghostex session/project organization.
- Session long-press opens actions; project header has explicit action controls.
- Last 7 tapped remote sessions stay warm in the background.
- Floating keyboard button remains always visible.
- File/image upload goes through SFTP and pastes markdown into the current terminal.

# accesskit_macos 0.26.3, patched for Ghostex

A copy of the crates.io release `accesskit_macos` 0.26.3 (MIT OR Apache-2.0, https://github.com/AccessKit/accesskit), used by the desktop app through `[patch.crates-io]` in `apps/desktop/Cargo.toml`.

The one change (`src/node.rs`, marked "Ghostex patch"): a node that supports `Action::ShowContextMenu` lists AppKit's `AXShowMenu` in `accessibilityActionNames`, and performing `AXShowMenu` sends `ShowContextMenu` to the app. Upstream 0.26.3 maps only Press, Increment, Decrement, Pick, Focus, SetValue and ScrollToVisible, so a GPUI element's right-click menu could not be opened by VoiceOver or automation.

When bumping gpui's `accesskit_macos`, copy the new release here and re-apply the change, or drop this copy once upstream maps `ShowContextMenu`.

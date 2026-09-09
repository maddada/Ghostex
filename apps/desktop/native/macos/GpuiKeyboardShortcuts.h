#pragma once

#import <AppKit/AppKit.h>

/*
 CDXC:Hotkeys 2026-09-10 DECISION:
 User: Command-V and the other shortcuts must keep working when the keyboard is set to Arabic or any other language, on macOS, Linux and Windows, as the best long-term solution rather than a per-language patch.

 CDXC:Hotkeys 2026-09-10 WHY:
 charactersIgnoringModifiers is the character a key types, so with an Arabic layout Command-V arrives as "د" and every comparison against "v" fails.
 A Shift chord can even arrive as ASCII punctuation ("{" for Shift+V on Arabic PC), so "use it whenever it is ASCII" is wrong as well.
 The keyboard layout itself declares the Latin identity of every key in its Command layer: each Apple non-Latin layout and "Dvorak - QWERTY ⌘" map Command+key to the QWERTY character, which is what AppKit matches menu key equivalents against and what GPUI uses for its own bindings (gpui_macos parse_keystroke).
 A custom layout without a Command layer falls back to the user's ASCII-capable layout, the same source AppKit consults, so no key table is maintained here.
 Plain and Shift-only keys are text, not shortcuts, and are returned unchanged so typing in any language is unaffected.
 The first fix rewrote every NSEvent at sendEvent: from a hard-coded kVK_ANSI table; that changed what Chromium, Ghostty and GPUI received and assumed a QWERTY base, so it was replaced by this read-only resolver that each matcher calls.
 SEE-ALSO: packages/shared/keyboard-shortcut-key.ts is the same definition for the web layer; apps/desktop/src/app/hotkeys.rs gpui_native_hotkey_text consumes the result through GhostexGpuiKeyboardRouteNativeEvent.
 */

/// The layout-independent shortcut characters of a key event.
/// For a Command, Control or Option chord this is the Latin character the
/// current keyboard layout assigns to the key (Shift applied, like
/// charactersIgnoringModifiers); for every other key it is the event's own
/// charactersIgnoringModifiers. Returns @"" for non-key events, never nil.
NSString *GhostexGpuiShortcutCharactersForEvent(NSEvent *event);

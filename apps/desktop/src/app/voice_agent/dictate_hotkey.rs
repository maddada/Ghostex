//! The Dictate hotkey's timing (Ctrl+Alt+Space by default): what a press and its release do in
//! each mode, `dictateHotkeyMode` in Settings › Voice. The chat box does the recording
//! (`native_chat/dictation.rs`); this file only decides when the hotkey starts or stops it.
//!
//! CDXC:VoiceMode 2026-11-05 DECISION: User (Q1261) picked Auto: a tap toggles (tap to start, tap
//! again to stop) and a hold of 300 ms or more records until the key comes up. Toggle and Hold
//! stay for people who want only one of the two. Q1262/Q1271: the key is Ctrl+Alt+Space; when
//! macOS already uses it to switch input sources, Settings › Hotkeys says so and offers Ctrl+Alt+D
//! (`macos_input_source_conflict`). It stays rebindable like every other hotkey.
//! SEE-ALSO: packages/settings-catalog/src/hotkey_definitions.rs (`toggleDictation`),
//! apps/desktop/src/app/render/root.rs (the key-up), hold_to_talk.rs (the voice agent's hold).

use std::time::{Duration, Instant};

/// A press held at least this long records only until the key comes up (Auto).
pub(crate) const HOLD_THRESHOLD: Duration = Duration::from_millis(300);
/// A press with no key repeat for this long lost its key-up (the key came up somewhere the window
/// never saw); the next press starts afresh instead of being taken for a repeat. Longer than the
/// slowest macOS "Delay until repeat" (1.8 s).
const STALE_PRESS: Duration = Duration::from_millis(2_000);

pub(crate) const HOTKEY_ID: &str = "toggleDictation";
pub(crate) const MODE_SETTING: &str = "dictateHotkeyMode";
/// The default chord (Settings › Hotkeys offers Ctrl+Alt+D when macOS takes it).
pub(crate) const DEFAULT_CHORD: &str = "ctrl+alt+space";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HotkeyMode {
    Auto,
    Toggle,
    Hold,
}

impl HotkeyMode {
    pub(crate) fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("toggle") => Self::Toggle,
            Some("hold") => Self::Hold,
            _ => Self::Auto,
        }
    }

    /// The saved mode.
    pub(crate) fn current() -> Self {
        Self::from_setting(
            crate::shared_settings::shared_sidebar_settings_snapshot()
                .object()
                .get(MODE_SETTING)
                .and_then(serde_json::Value::as_str),
        )
    }
}

/// What the chat box should do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HotkeyStep {
    Start,
    Stop,
    Nothing,
}

/// The press the hotkey's release will end.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Press {
    at: Instant,
    /// The press or its latest key repeat.
    last_seen: Instant,
    /// This press started the recording (a release may stop it).
    started: bool,
    /// The chat session whose chat box the press went to (`TerminalSessionId`).
    session: u64,
    /// The chord's key in GPUI's naming ("space"), whose key-up ends the press.
    key: String,
}

/// The hotkey's press waiting for its release.
#[derive(Default)]
pub(crate) struct DictateHotkey {
    press: Option<Press>,
}

impl DictateHotkey {
    /// The hotkey went down. `recording` is whether that chat box is recording (or starting);
    /// a key repeat while the press is held changes nothing.
    pub(crate) fn press(
        &mut self,
        mode: HotkeyMode,
        recording: bool,
        session: u64,
        key: &str,
        now: Instant,
    ) -> HotkeyStep {
        if let Some(press) = self.press.as_mut() {
            if now.saturating_duration_since(press.last_seen) < STALE_PRESS {
                press.last_seen = now;
                return HotkeyStep::Nothing;
            }
            self.press = None;
        }
        let step = if recording {
            HotkeyStep::Stop
        } else {
            HotkeyStep::Start
        };
        // A press that stops has nothing for its release to do, but it is kept so the key's
        // repeats while held are swallowed.
        self.press = Some(Press {
            at: now,
            last_seen: now,
            started: step == HotkeyStep::Start && mode != HotkeyMode::Toggle,
            session,
            key: key.to_string(),
        });
        step
    }

    /// A key came up: the session to stop, when it is the press's key and the press should end
    /// the recording. `None` also when no press waits or it is another key.
    pub(crate) fn release(&mut self, mode: HotkeyMode, key: &str, now: Instant) -> Option<u64> {
        if self.press.as_ref().is_none_or(|press| press.key != key) {
            return None;
        }
        self.end(mode, now)
    }

    /// The window stopped being active: the key-up goes to another app, so the press ends now.
    pub(crate) fn window_deactivated(&mut self, mode: HotkeyMode, now: Instant) -> Option<u64> {
        self.end(mode, now)
    }

    /// Whether a press is waiting for its release (its key-up is the hotkey's).
    pub(crate) fn pressed(&self) -> bool {
        self.press.is_some()
    }

    /// The key whose key-up ends the waiting press.
    pub(crate) fn pressed_key(&self) -> Option<&str> {
        self.press.as_ref().map(|press| press.key.as_str())
    }

    fn end(&mut self, mode: HotkeyMode, now: Instant) -> Option<u64> {
        let press = self.press.take()?;
        let held = now.saturating_duration_since(press.at);
        let stops = press.started
            && match mode {
                HotkeyMode::Hold => true,
                HotkeyMode::Auto => held >= HOLD_THRESHOLD,
                HotkeyMode::Toggle => false,
            };
        stops.then_some(press.session)
    }
}

impl crate::GhostexGpuiApp {
    /// The Dictate hotkey went down for the chat of `session` (its repeats come here too).
    /// Answers false where that chat box draws no microphone, so the chord keeps its meaning.
    pub(crate) fn dictate_hotkey_pressed(
        &mut self,
        session: crate::TerminalSessionId,
        view: gpui::Entity<crate::app::native_chat::state::NativeChatView>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !view.read(cx).dictation_hotkey_available() {
            return false;
        }
        let Some(key) = hotkey_key() else {
            return false;
        };
        let recording = view.read(cx).dictating(cx);
        let step = self.dictate_hotkey.press(
            HotkeyMode::current(),
            recording,
            session.0,
            &key,
            Instant::now(),
        );
        if step != HotkeyStep::Nothing {
            view.update(cx, |view, cx| {
                view.dictation_hotkey_step(step == HotkeyStep::Start, window, cx)
            });
        }
        true
    }

    /// A key came up at the window's root: ends a Dictate hotkey press whose key it is. True when
    /// it was that key, so nothing else sees it.
    pub(crate) fn dictate_hotkey_key_up(
        &mut self,
        event: &gpui::KeyUpEvent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.dictate_hotkey.pressed() {
            return false;
        }
        let ended = self.dictate_hotkey.pressed_key() == Some(event.keystroke.key.as_str());
        if let Some(session) =
            self.dictate_hotkey
                .release(HotkeyMode::current(), &event.keystroke.key, Instant::now())
        {
            self.stop_dictation_for(session, window, cx);
        }
        ended
    }

    /// The window stopped being active mid-press: a hold ends as if released.
    pub(crate) fn dictate_hotkey_window_deactivated(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(session) = self
            .dictate_hotkey
            .window_deactivated(HotkeyMode::current(), Instant::now())
        {
            self.stop_dictation_for(session, window, cx);
        }
    }

    fn stop_dictation_for(
        &mut self,
        session: u64,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = self
            .native_chat_views
            .iter()
            .find(|(id, _)| id.0 == session)
            .map(|(_, view)| view.clone());
        if let Some(view) = view {
            view.update(cx, |view, cx| view.dictation_hotkey_step(false, window, cx));
        }
    }
}

/// The Dictate hotkey's key in GPUI's naming ("space" for Ctrl+Alt+Space), when one is set.
fn hotkey_key() -> Option<String> {
    let chord = crate::gpui_configured_hotkey_key(HOTKEY_ID)?;
    let keystroke = crate::gpui_keystroke_from_shared_hotkey(&chord)?;
    let key = keystroke.rsplit('-').next().unwrap_or_default();
    Some(if key.is_empty() {
        "-".to_string()
    } else {
        key.to_string()
    })
}

/// Whether macOS switches input sources with Ctrl+Option+Space ("Select next source in Input
/// menu", or "Select the previous input source" set to it) and has more than one keyboard input
/// source to switch between, so the system takes the chord before Ghostex sees it.
/// Read again at most once a minute, since Settings pages ask while they draw.
pub(crate) fn macos_input_source_conflict() -> bool {
    #[cfg(target_os = "macos")]
    {
        static CHECKED: std::sync::Mutex<Option<(Instant, bool)>> = std::sync::Mutex::new(None);
        let mut checked = CHECKED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((at, conflict)) = *checked
            && at.elapsed() < Duration::from_secs(60)
        {
            return conflict;
        }
        let conflict = mac::input_source_conflict();
        *checked = Some((Instant::now(), conflict));
        conflict
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Ctrl+Option+Space in the symbolic hot keys' terms: the Space key code and the Control and
/// Option flags.
const SPACE_KEY_CODE: i64 = 49;
const CONTROL_OPTION_FLAGS: i64 = 0x40000 | 0x80000;

/// Whether symbolic hot key `entry` (60 or 61 of `AppleSymbolicHotKeys`) is an enabled
/// Ctrl+Option+Space. `None` (never changed) is the system default: 61 is Ctrl+Option+Space and on.
fn symbolic_hotkey_is_ctrl_option_space(id: u32, entry: Option<&serde_json::Value>) -> bool {
    let Some(entry) = entry else {
        return id == 61;
    };
    let enabled =
        entry["enabled"].as_bool().unwrap_or(false) || entry["enabled"].as_i64() == Some(1);
    let parameters = &entry["value"]["parameters"];
    enabled
        && parameters[1].as_i64() == Some(SPACE_KEY_CODE)
        && parameters[2].as_i64() == Some(CONTROL_OPTION_FLAGS)
}

/// How many keyboard layouts and input modes are turned on (`AppleEnabledInputSources`).
fn keyboard_input_sources(sources: &serde_json::Value) -> usize {
    sources
        .as_array()
        .into_iter()
        .flatten()
        .filter(|source| {
            matches!(
                source["InputSourceKind"].as_str(),
                Some("Keyboard Layout" | "Input Mode")
            )
        })
        .count()
}

#[cfg(target_os = "macos")]
mod mac {
    use std::process::Command;

    /// A preferences domain as JSON (`defaults export` piped through `plutil`), or `None`.
    fn domain(name: &str) -> Option<serde_json::Value> {
        let exported = Command::new("/usr/bin/defaults")
            .args(["export", name, "-"])
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        let mut child = Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        {
            use std::io::Write as _;
            child.stdin.take()?.write_all(&exported.stdout).ok()?;
        }
        let output = child.wait_with_output().ok()?;
        serde_json::from_slice(&output.stdout).ok()
    }

    pub(super) fn input_source_conflict() -> bool {
        // The system's key: enabled and set to Ctrl+Option+Space.
        let hotkeys = domain("com.apple.symbolichotkeys");
        let table = hotkeys
            .as_ref()
            .and_then(|hotkeys| hotkeys.get("AppleSymbolicHotKeys"));
        let taken = [60u32, 61].into_iter().any(|id| {
            super::symbolic_hotkey_is_ctrl_option_space(
                id,
                table.and_then(|table| table.get(id.to_string())),
            )
        });
        if !taken {
            return false;
        }
        // HIToolbox can hold dates and data `plutil` can't write as JSON; then it is assumed
        // there are sources to switch between, as the key is on.
        match domain("com.apple.HIToolbox") {
            Some(toolbox) => {
                super::keyboard_input_sources(&toolbox["AppleEnabledInputSources"]) > 1
            }
            None => true,
        }
    }
}

/// CDXC:VoiceMode 2026-11-05 WHY: Settings › Hotkeys saves the whole table, so anyone who ever
/// changed a hotkey has Dictate's old default (Option+Ctrl+D, Ctrl+Alt+Shift+D on Windows and
/// Linux) saved and would never get Ctrl+Alt+Space. It is not a retired key, because Ctrl+Alt+D
/// is the chord offered when macOS takes Ctrl+Alt+Space, so the move happens once, here, with a
/// marker file, and leaves Ctrl+Alt+D on a Mac where the system has the new chord.
pub(crate) fn move_old_default_chord_once() {
    let paths = ghostex_paths::GhostexPaths::resolve();
    let marker = paths.state_dir.join("dictate-hotkey-moved");
    if marker.exists() {
        return;
    }
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let mut object = snapshot.object().clone();
    let saved = object
        .get("hotkeys")
        .and_then(|hotkeys| hotkeys.get(HOTKEY_ID))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let outcome = match saved {
        Some(saved) if old_default_chord(&saved) && !macos_input_source_conflict() => {
            if let Some(hotkeys) = object
                .get_mut("hotkeys")
                .and_then(serde_json::Value::as_object_mut)
            {
                hotkeys.insert(HOTKEY_ID.to_string(), DEFAULT_CHORD.into());
            }
            match crate::shared_settings::write_shared_sidebar_settings_object(object) {
                Ok(_) => "moved to ctrl+alt+space",
                Err(_) => return,
            }
        }
        _ => "nothing to move",
    };
    let _ = std::fs::create_dir_all(&paths.state_dir);
    let _ = std::fs::write(&marker, format!("{outcome}\n"));
}

/// Dictate's default before 2026-11-05, on any system.
fn old_default_chord(chord: &str) -> bool {
    let chord = chord.trim().to_ascii_lowercase();
    chord == "ctrl+alt+d" || chord == "cmd+alt+shift+d"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn auto_a_tap_starts_and_its_release_keeps_recording() {
        let start = Instant::now();
        let mut hotkey = DictateHotkey::default();
        assert_eq!(
            hotkey.press(HotkeyMode::Auto, false, 1, "space", start),
            HotkeyStep::Start
        );
        assert_eq!(
            hotkey.release(HotkeyMode::Auto, "space", at(start, 120)),
            None
        );
        assert!(!hotkey.pressed());
        // The second tap stops it, and its release does nothing more.
        assert_eq!(
            hotkey.press(HotkeyMode::Auto, true, 1, "space", at(start, 2_000)),
            HotkeyStep::Stop
        );
        assert_eq!(
            hotkey.release(HotkeyMode::Auto, "space", at(start, 2_080)),
            None
        );
    }

    #[test]
    fn auto_a_hold_of_300_ms_or_more_stops_on_release() {
        let start = Instant::now();
        let mut hotkey = DictateHotkey::default();
        hotkey.press(HotkeyMode::Auto, false, 1, "space", start);
        assert_eq!(
            hotkey.release(HotkeyMode::Auto, "space", at(start, 300)),
            Some(1)
        );
        hotkey.press(HotkeyMode::Auto, false, 1, "space", at(start, 1_000));
        assert_eq!(
            hotkey.release(HotkeyMode::Auto, "space", at(start, 1_299)),
            None
        );
    }

    #[test]
    fn key_repeats_while_held_change_nothing() {
        let start = Instant::now();
        let mut hotkey = DictateHotkey::default();
        hotkey.press(HotkeyMode::Auto, false, 1, "space", start);
        for ms in [500, 530, 560] {
            assert_eq!(
                hotkey.press(HotkeyMode::Auto, true, 1, "space", at(start, ms)),
                HotkeyStep::Nothing
            );
        }
        assert_eq!(
            hotkey.release(HotkeyMode::Auto, "space", at(start, 900)),
            Some(1)
        );
    }

    #[test]
    fn a_press_whose_key_up_was_lost_does_not_swallow_the_next_one() {
        let start = Instant::now();
        let mut hotkey = DictateHotkey::default();
        hotkey.press(HotkeyMode::Auto, false, 1, "space", start);
        // No release ever came; seconds later the key goes down again.
        assert_eq!(
            hotkey.press(HotkeyMode::Auto, true, 1, "space", at(start, 5_000)),
            HotkeyStep::Stop
        );
        // Repeats every few dozen milliseconds keep a long hold alive.
        let mut hotkey = DictateHotkey::default();
        hotkey.press(HotkeyMode::Auto, false, 1, "space", start);
        for ms in (400..4_000).step_by(40) {
            assert_eq!(
                hotkey.press(HotkeyMode::Auto, true, 1, "space", at(start, ms)),
                HotkeyStep::Nothing
            );
        }
        assert_eq!(
            hotkey.release(HotkeyMode::Auto, "space", at(start, 4_010)),
            Some(1)
        );
    }

    #[test]
    fn another_key_coming_up_does_not_end_the_press() {
        let start = Instant::now();
        let mut hotkey = DictateHotkey::default();
        hotkey.press(HotkeyMode::Auto, false, 1, "space", start);
        assert_eq!(hotkey.release(HotkeyMode::Auto, "a", at(start, 800)), None);
        assert!(hotkey.pressed());
        assert_eq!(
            hotkey.release(HotkeyMode::Auto, "space", at(start, 900)),
            Some(1)
        );
    }

    #[test]
    fn toggle_never_stops_on_release_and_hold_always_does() {
        let start = Instant::now();
        let mut hotkey = DictateHotkey::default();
        hotkey.press(HotkeyMode::Toggle, false, 1, "space", start);
        assert_eq!(
            hotkey.release(HotkeyMode::Toggle, "space", at(start, 5_000)),
            None
        );
        hotkey.press(HotkeyMode::Hold, false, 2, "space", at(start, 6_000));
        assert_eq!(
            hotkey.release(HotkeyMode::Hold, "space", at(start, 6_050)),
            Some(2)
        );
    }

    #[test]
    fn leaving_the_window_mid_hold_ends_it_like_a_release() {
        let start = Instant::now();
        let mut hotkey = DictateHotkey::default();
        hotkey.press(HotkeyMode::Auto, false, 1, "space", start);
        assert_eq!(
            hotkey.window_deactivated(HotkeyMode::Auto, at(start, 700)),
            Some(1)
        );
        assert!(!hotkey.pressed());
    }

    #[test]
    fn the_mode_setting_defaults_to_auto() {
        assert_eq!(HotkeyMode::from_setting(None), HotkeyMode::Auto);
        assert_eq!(HotkeyMode::from_setting(Some("auto")), HotkeyMode::Auto);
        assert_eq!(HotkeyMode::from_setting(Some("toggle")), HotkeyMode::Toggle);
        assert_eq!(HotkeyMode::from_setting(Some("hold")), HotkeyMode::Hold);
        assert_eq!(HotkeyMode::from_setting(Some("odd")), HotkeyMode::Auto);
    }

    #[test]
    fn the_input_source_key_counts_only_when_enabled_on_ctrl_option_space() {
        let on =
            json!({"enabled": true, "value": {"parameters": [32, 49, 786432], "type": "standard"}});
        let off = json!({"enabled": false, "value": {"parameters": [32, 49, 786432]}});
        let ctrl_space = json!({"enabled": true, "value": {"parameters": [32, 49, 262144]}});
        assert!(symbolic_hotkey_is_ctrl_option_space(61, Some(&on)));
        assert!(symbolic_hotkey_is_ctrl_option_space(60, Some(&on)));
        assert!(!symbolic_hotkey_is_ctrl_option_space(61, Some(&off)));
        assert!(!symbolic_hotkey_is_ctrl_option_space(61, Some(&ctrl_space)));
        // Never changed: the system default.
        assert!(symbolic_hotkey_is_ctrl_option_space(61, None));
        assert!(!symbolic_hotkey_is_ctrl_option_space(60, None));
    }

    #[test]
    fn only_keyboard_layouts_and_input_modes_count_as_sources() {
        let sources = json!([
            {"InputSourceKind": "Keyboard Layout", "KeyboardLayout Name": "ABC"},
            {"InputSourceKind": "Non Keyboard Input Method", "Bundle ID": "com.apple.PressAndHold"},
        ]);
        assert_eq!(keyboard_input_sources(&sources), 1);
        let two = json!([
            {"InputSourceKind": "Keyboard Layout"},
            {"InputSourceKind": "Keyboard Input Method"},
            {"InputSourceKind": "Input Mode"},
        ]);
        assert_eq!(keyboard_input_sources(&two), 2);
    }

    #[test]
    fn only_the_old_defaults_move() {
        assert!(old_default_chord("ctrl+alt+d"));
        assert!(old_default_chord("cmd+alt+shift+d"));
        assert!(!old_default_chord("ctrl+alt+space"));
        assert!(!old_default_chord("cmd+shift+d"));
    }
}

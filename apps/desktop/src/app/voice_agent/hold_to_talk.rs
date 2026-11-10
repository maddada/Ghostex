//! Holding to talk on the computer: the panel's hold button, Space while no text field has the
//! keyboard, and the Hold to Talk hotkey. gxserver decides what a hold does to the turn
//! (server/src/voice_agent/talk.rs); this file decides when the computer starts and ends one.
//!
//! CDXC:VoiceMode 2026-10-24 DECISION: User, asked "When the computer window loses focus while Space is held (⌘-Tab, a dialog opening), what happens to the turn?", picked "Send what was said up to that moment" (not the recommended cancel). A Space or hotkey hold ends as a release when the window stops being active, since its key-up goes to another app.
//! CDXC:VoiceMode 2026-10-24 WHY: Space holds Talk only while the keyboard is parked on the window itself (no focused element, nothing the root would route keys to: no chat box, terminal, web page or IME composition). Opening a session hands the keyboard to its chat box or terminal, which is why the Hold to Talk hotkey exists (CDXC:VoiceMode 2026-10-24 DECISION in packages/settings-catalog/src/hotkey_definitions.rs).
//! SEE-ALSO: server/src/voice_agent/talk.rs, panel_controls.rs, apps/mobile/app/src/voice/conversation.ts.

use gpui::{KeyDownEvent, KeyUpEvent, Keystroke, SharedString, Window};

use super::conversation::VoicePhase;
use super::panel::controls::HoldLook;
use crate::*;

/// How long a hint stays in the strip above the panel's buttons.
const HINT_FOR: std::time::Duration = std::time::Duration::from_millis(2_500);
pub(super) const HINT_TOO_SHORT: &str = "Hold to talk, then let go to send";
pub(super) const HINT_UNMUTE: &str = "Unmute to talk";
pub(super) const HINT_ALREADY_HEARD: &str = "Mute first, then hold to talk";
pub(super) const HINT_CANCELLED: &str = "Talk cancelled";
pub(super) const HOLD_TO_TALK_HOTKEY: &str = "holdToTalk";

/// What started the hold, which says which release ends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HoldSource {
    Button,
    Space,
    /// The Hold to Talk hotkey; its key (`gpui` name) coming up ends the hold.
    Hotkey(String),
}

/// What holding would do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoldMode {
    /// A push-to-talk call: talk, and send on release.
    Talk,
    /// A muted Regular call: unmute for as long as the hold lasts.
    Unmute,
    /// A muted push-to-talk call: Mute wins.
    MutedLock,
    /// A Regular call that already listens.
    AlreadyHeard,
    /// No live call.
    None,
}

pub(super) fn plain_space(keystroke: &Keystroke) -> bool {
    let modifiers = &keystroke.modifiers;
    keystroke.key == "space"
        && !modifiers.platform
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.shift
        && !modifiers.function
}

/// The Hold to Talk hotkey's key in GPUI's naming ("t" for Ctrl+Alt+T), when one is set.
fn hold_hotkey_key() -> Option<String> {
    let chord = gpui_configured_hotkey_key(HOLD_TO_TALK_HOTKEY)?;
    let keystroke = gpui_keystroke_from_shared_hotkey(&chord)?;
    let key = keystroke.rsplit('-').next().unwrap_or_default();
    Some(if key.is_empty() {
        "-".to_string()
    } else {
        key.to_string()
    })
}

impl GhostexGpuiApp {
    pub(crate) fn voice_agent_hold_mode(&self) -> HoldMode {
        let agent = &self.voice_agent;
        if agent.phase != VoicePhase::Live {
            return HoldMode::None;
        }
        match (agent.push_to_talk, agent.muted) {
            (true, false) => HoldMode::Talk,
            (true, true) => HoldMode::MutedLock,
            (false, true) => HoldMode::Unmute,
            (false, false) => HoldMode::AlreadyHeard,
        }
    }

    pub(super) fn voice_agent_hold_look(&self) -> HoldLook {
        if self.voice_agent.hold.is_some() {
            return HoldLook::Held;
        }
        match self.voice_agent_hold_mode() {
            HoldMode::Talk | HoldMode::Unmute => HoldLook::Ready,
            _ => HoldLook::Unavailable,
        }
    }

    /// The hold button's tooltip: what holding does, then its keys.
    pub(super) fn voice_agent_hold_tooltip(&self) -> SharedString {
        let what = match self.voice_agent_hold_mode() {
            HoldMode::Talk | HoldMode::None => "Hold to talk",
            HoldMode::Unmute => "Hold to unmute",
            HoldMode::MutedLock => HINT_UNMUTE,
            HoldMode::AlreadyHeard => "Hold to talk while muted",
        };
        let keys = match gpui_configured_hotkey_label(HOLD_TO_TALK_HOTKEY) {
            Some(hotkey) if !hotkey.is_empty() => format!(" (Space or {hotkey})"),
            _ => " (or hold Space)".to_string(),
        };
        format!("{what}{keys}").into()
    }

    /// The header's Space keycap: shown while Space would hold Talk now, lit while a keyboard hold
    /// is on.
    pub(super) fn voice_agent_space_keycap(&self) -> Option<bool> {
        let held = matches!(
            self.voice_agent.hold,
            Some(HoldSource::Space | HoldSource::Hotkey(_))
        );
        if held {
            return Some(true);
        }
        ((self.voice_agent.space_free || self.voice_agent.panel_space_free)
            && matches!(
                self.voice_agent_hold_mode(),
                HoldMode::Talk | HoldMode::Unmute
            ))
        .then_some(false)
    }

    /// Starts a hold; false when holding means nothing now (no call, or a Regular call that already
    /// listens), so Space keeps its usual meaning.
    pub(crate) fn start_voice_agent_hold(
        &mut self,
        source: HoldSource,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.voice_agent.hold.is_some() {
            return true;
        }
        match self.voice_agent_hold_mode() {
            HoldMode::Talk | HoldMode::Unmute => {
                self.voice_agent.hold = Some(source);
                self.voice_agent.hint = None;
                self.set_voice_agent_talking(true, cx);
                true
            }
            HoldMode::MutedLock => {
                self.flash_voice_agent_hint(HINT_UNMUTE, cx);
                true
            }
            HoldMode::AlreadyHeard => {
                if source == HoldSource::Button {
                    self.flash_voice_agent_hint(HINT_ALREADY_HEARD, cx);
                }
                false
            }
            HoldMode::None => false,
        }
    }

    pub(crate) fn end_voice_agent_hold(&mut self, cx: &mut gpui::Context<Self>) {
        if self.voice_agent.hold.take().is_some() {
            self.set_voice_agent_talking(false, cx);
        }
    }

    /// The panel's hold button, pressed or released (anywhere in the panel).
    pub(super) fn voice_agent_button_hold(&mut self, down: bool, cx: &mut gpui::Context<Self>) {
        if down {
            self.start_voice_agent_hold(HoldSource::Button, cx);
        } else if self.voice_agent.hold == Some(HoldSource::Button) {
            self.end_voice_agent_hold(cx);
        }
    }

    /// Space at the window's root, before anything else sees it. True when it was the hold's (the
    /// start, or a key repeat while held), so nothing types it or wakes a session with it.
    pub(crate) fn voice_agent_space_down(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !plain_space(&event.keystroke) {
            return false;
        }
        if self.voice_agent.hold == Some(HoldSource::Space) {
            return true;
        }
        if event.is_held
            || self.voice_agent.hold.is_some()
            || !self.voice_agent_space_free(window, cx)
        {
            return false;
        }
        self.start_voice_agent_hold(HoldSource::Space, cx)
    }

    /// A key coming up at the window's root: ends a Space or hotkey hold whose key it is.
    pub(crate) fn voice_agent_key_up(
        &mut self,
        event: &KeyUpEvent,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let ends = match &self.voice_agent.hold {
            Some(HoldSource::Space) => event.keystroke.key == "space",
            Some(HoldSource::Hotkey(key)) => event.keystroke.key == *key,
            _ => false,
        };
        if ends {
            self.end_voice_agent_hold(cx);
        }
        ends
    }

    /// The Hold to Talk hotkey went down (its repeats come here too and change nothing).
    pub(crate) fn hold_to_talk_hotkey_pressed(&mut self, cx: &mut gpui::Context<Self>) {
        if self.voice_agent.hold.is_some() {
            return;
        }
        let Some(key) = hold_hotkey_key() else {
            return;
        };
        self.start_voice_agent_hold(HoldSource::Hotkey(key), cx);
        if self.voice_agent.hold.is_some() {
            // The key-up has to reach this window, not a web page that held the keyboard.
            self.reclaim_gpui_root_for_chrome_input_focus();
        }
    }

    /// Space in the voice panel while it has the keyboard and its text box does not
    /// (`panel_input.rs`): down starts a Space hold (true when Space was the hold's), up ends it.
    pub(super) fn voice_agent_panel_space(
        &mut self,
        down: bool,
        repeat: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !down {
            if self.voice_agent.hold == Some(HoldSource::Space) {
                self.end_voice_agent_hold(cx);
            }
            return true;
        }
        if self.voice_agent.hold == Some(HoldSource::Space) {
            return true;
        }
        if repeat || self.voice_agent.hold.is_some() {
            return false;
        }
        self.start_voice_agent_hold(HoldSource::Space, cx)
    }

    /// The voice panel noted whether Space would hold Talk there, for its keycap.
    pub(super) fn note_voice_agent_panel_space_free(
        &mut self,
        free: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if free != self.voice_agent.panel_space_free {
            self.voice_agent.panel_space_free = free;
            self.sync_voice_agent_panel(cx);
        }
    }

    /// The window stopped being active: a keyboard hold's key-up goes to another app, so the hold
    /// ends here and what was said is sent.
    pub(crate) fn voice_agent_window_deactivated(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(
            self.voice_agent.hold,
            Some(HoldSource::Space | HoldSource::Hotkey(_))
        ) {
            self.end_voice_agent_hold(cx);
        }
    }

    /// Whether Space would hold Talk: the window is active and its keyboard is parked on the
    /// window itself, with nothing the root would hand the key to.
    pub(crate) fn voice_agent_space_free(&self, window: &Window, cx: &gpui::App) -> bool {
        window.is_window_active()
            && window
                .focused(cx)
                .is_none_or(|focused| focused == self.root_action_focus_handle)
            && matches!(
                self.shell_keyboard_owner(),
                ShellKeyboardOwner::Nothing | ShellKeyboardOwner::GpuiViewPanelSurface
            )
            && self.titlebar_popup_menu.is_none()
            && !super::typing::composing_text()
    }

    /// Notes from the main window's render whether Space would hold Talk, so the panel's keycap
    /// follows the keyboard.
    pub(crate) fn note_voice_agent_space_free(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.voice_agent.phase != VoicePhase::Live {
            return;
        }
        let free = self.voice_agent_space_free(window, cx);
        if free != self.voice_agent.space_free {
            self.voice_agent.space_free = free;
            cx.defer_in(window, |this, _, cx| this.sync_voice_agent_panel(cx));
        }
    }

    /// Shows a short note in the strip above the panel's buttons.
    pub(super) fn flash_voice_agent_hint(
        &mut self,
        text: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.voice_agent.hint = Some(text.to_string());
        self.voice_agent.hint_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HINT_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.voice_agent.hint = None;
                this.voice_agent.hint_timer = None;
                this.sync_voice_agent_panel(cx);
            });
        }));
        self.sync_voice_agent_panel(cx);
    }
}

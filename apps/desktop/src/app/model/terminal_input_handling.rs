// C1 wave-3 re-cluster: terminal body mouse-position, text-input, IME, mouse-modifier, and scroll forwarding functions, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalBodyMousePosition {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

pub(crate) fn terminal_body_relative_mouse_position(
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
) -> Option<TerminalBodyMousePosition> {
    if !bounds.contains(&position) {
        return None;
    }

    Some(TerminalBodyMousePosition {
        x: f64::from(position.x.as_f32() - bounds.origin.x.as_f32()),
        y: f64::from(position.y.as_f32() - bounds.origin.y.as_f32()),
    })
}

pub(crate) fn terminal_body_relative_mouse_position_for_slot<
    MountSlotId: Copy + Eq + std::hash::Hash,
>(
    bounds_by_slot: &HashMap<MountSlotId, Bounds<Pixels>>,
    slot_id: MountSlotId,
    position: Point<Pixels>,
) -> Option<TerminalBodyMousePosition> {
    terminal_body_relative_mouse_position(*bounds_by_slot.get(&slot_id)?, position)
}

/*
CDXC:Terminal 2026-06-23-10:13:
Phase 2 terminal input parity forwards only committed GPUI `key_char` text. Do not synthesize from `key`, do not map physical keys without a native keycode, reject Cmd/Super and Control modified events so shortcuts and control-key terminal bindings can use a later Ghostty key-event bridge, and keep Option-generated characters when GPUI has already committed them as text.

CDXC:Terminal 2026-06-23-10:18:
Focused text delivery must choose an explicit terminal target before touching a surface: Agents requires Agents mode plus an Agents-pane focus target, command requires command-pane focus, and Browser/project-editor focus must no-op instead of falling through to a terminal helper.

CDXC:Terminal 2026-06-23-10:45:
Terminal IME/preedit delivery is a text-service path, not a keyboard fallback. GPUI may register input handling only for the exact focused mounted terminal body, then send committed text/preedit bytes to the matching Ghostty owner while retaining only sanitized UTF-16 marked ranges and no raw typed or terminal content.

CDXC:Terminal 2026-06-23-11:50:
The local GPUI app event API exposes `KeyDownEvent { keystroke, is_held, prefer_character_input }`; `Keystroke` exposes only modifiers, layout-derived `key`, and committed `key_char`. The macOS backend uses `NSEvent.keyCode()` while constructing `Keystroke` but drops that native keycode before app listeners run, so Ghostty physical-key forwarding must remain blocked until GPUI exposes a stable native keycode or UIEvents-code physical key identity.

CDXC:Terminal 2026-06-23-14:23:
Committed `key_char` text is a text-input signal, not evidence for physical-key or key-binding parity. Layout `key` values, Control shortcuts, and Cmd/Super shortcuts must stay rejected by this helper until GPUI can pass a stable native keycode or UIEvents-code identity to Ghostty without guessing.
*/
pub(crate) fn committed_terminal_text_from_key_down_event(event: &KeyDownEvent) -> Option<&str> {
    committed_terminal_text_from_keystroke(&event.keystroke)
}

pub(crate) fn committed_terminal_text_from_keystroke(keystroke: &Keystroke) -> Option<&str> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.control {
        return None;
    }

    let text = keystroke.key_char.as_deref()?;
    if text.is_empty() { None } else { Some(text) }
}

pub(crate) fn command_pane_sleeping_placeholder_keystroke_requests_wake(
    keystroke: &Keystroke,
) -> bool {
    /*
    CDXC:SessionSleep 2026-06-25-14:49:
    Sleeping command placeholders wake on plain alphanumeric key-downs like native AppKit. Reject Cmd, Control, and Option/Alt modified keys, and use GPUI's layout key only as a wake-affordance identity for shifted digits rather than as terminal input.

    CDXC:SessionSleep 2026-06-25-19:02:
    Native AppKit rejects Function-modified wake keys before inspecting alphanumeric text. GPUI must keep Function inert too, while still allowing plain and Shift-only letters/digits that AppKit treats as `charactersIgnoringModifiers`.
    */
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.control || modifiers.alt || modifiers.function {
        return false;
    }

    command_pane_sleeping_placeholder_wake_text_is_alphanumeric(keystroke.key_char.as_deref())
        || command_pane_sleeping_placeholder_wake_text_is_alphanumeric(Some(&keystroke.key))
}

pub(crate) fn command_pane_sleeping_placeholder_wake_text_is_alphanumeric(
    text: Option<&str>,
) -> bool {
    let Some(text) = text else {
        return false;
    };
    let mut chars = text.chars();
    let Some(character) = chars.next() else {
        return false;
    };
    chars.next().is_none() && character.is_ascii_alphanumeric()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedTerminalTextTarget {
    Agents,
    Command,
    ProjectEditorCompanion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedTerminalTextMountTarget {
    Agents(AgentsTerminalBodyMountSlotId),
    Command(CommandTerminalBodyMountSlotId),
    ProjectEditorCompanion(ProjectEditorCompanionTerminalBodyMountSlotId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalTextMarkedRange {
    pub(crate) target: FocusedTerminalTextMountTarget,
    pub(crate) range: Range<usize>,
}

pub(crate) fn focused_terminal_text_target(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
) -> Option<FocusedTerminalTextTarget> {
    match shell_focus {
        ShellFocusTarget::AgentsPane(_) if active_mode == TitlebarMode::Agents => {
            Some(FocusedTerminalTextTarget::Agents)
        }
        ShellFocusTarget::CommandPane => Some(FocusedTerminalTextTarget::Command),
        ShellFocusTarget::ProjectEditorCompanion(mode)
            if active_mode == mode && mode.is_project_editor_mode() =>
        {
            Some(FocusedTerminalTextTarget::ProjectEditorCompanion)
        }
        ShellFocusTarget::AgentsPane(_)
        | ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_)
        | ShellFocusTarget::ProjectEditorCompanion(_) => None,
    }
}

pub(crate) fn terminal_text_marked_range_for_preedit(
    replacement_range: Option<Range<usize>>,
    new_text: &str,
) -> Option<Range<usize>> {
    let marked_len_utf16 = new_text.encode_utf16().count();
    if marked_len_utf16 == 0 {
        return None;
    }

    let marked_start = replacement_range.map_or(0, |range| range.start);
    Some(marked_start..marked_start.saturating_add(marked_len_utf16))
}

pub(crate) fn terminal_ime_bounds_from_ghostty_point(
    body_bounds: Bounds<Pixels>,
    ime_point: terminal_ghostty_surface::GhosttySurfaceImePoint,
) -> Option<Bounds<Pixels>> {
    let x = ime_point.x as f32;
    let y = ime_point.y as f32;
    let width = ime_point.width as f32;
    let height = ime_point.height as f32;

    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || width < 0.0
        || height < 0.0
    {
        return None;
    }

    Some(Bounds::new(
        gpui::point(body_bounds.origin.x + px(x), body_bounds.origin.y + px(y)),
        size(px(width), px(height)),
    ))
}

/*
CDXC:Terminal 2026-06-23-09:45:
Mounted Agents and command terminal bodies must translate GPUI mouse-event keyboard modifiers into Ghostty input.Mods bits for pointer position and button events. Map shift, control, alt, and platform to Ghostty shift, ctrl, alt, and super while intentionally ignoring function; scroll events keep Ghostty ScrollMods precision-only and forward keyboard modifiers only through the preceding pointer position update.
*/
pub(crate) fn ghostty_mouse_mods_from_gpui_modifiers(
    modifiers: Modifiers,
) -> ghostty_kit::ffi::ghostty_input_mods_e {
    let mut mods = GHOSTTY_MOUSE_ZERO_MODS;

    if modifiers.shift {
        mods |= GHOSTTY_MOUSE_SHIFT_MOD;
    }
    if modifiers.control {
        mods |= GHOSTTY_MOUSE_CTRL_MOD;
    }
    if modifiers.alt {
        mods |= GHOSTTY_MOUSE_ALT_MOD;
    }
    if modifiers.platform {
        mods |= GHOSTTY_MOUSE_SUPER_MOD;
    }

    mods
}

/*
CDXC:Terminal 2026-06-23-10:23:
Mounted Agents and command terminal bodies must forward Ghostty's exact left, right, and middle mouse button values while rejecting GPUI navigation buttons. Keep the mapper pure so button parity does not store raw input state, coordinates, modifiers, terminal content, command text, paths, URLs, or titles.

CDXC:Terminal 2026-06-23-12:42:
The bounded non-left parity audit keeps the existing implementation: mounted Agents and command terminal bodies use this shared mapper for right/middle press, in-body release, and capture-gated body-level mouse-up-out while preserving current-slot, recorded-bounds, exact-surface/runtime, and mapped-modifier gates.
*/
pub(crate) fn ghostty_mouse_button_from_gpui_button(
    button: MouseButton,
) -> Option<ghostty_kit::ffi::ghostty_input_mouse_button_e> {
    match button {
        MouseButton::Left => Some(ghostty_kit::ffi::GHOSTTY_MOUSE_LEFT),
        MouseButton::Right => Some(ghostty_kit::ffi::GHOSTTY_MOUSE_RIGHT),
        MouseButton::Middle => Some(ghostty_kit::ffi::GHOSTTY_MOUSE_MIDDLE),
        MouseButton::Navigate(_) => None,
    }
}

/*
CDXC:Terminal 2026-06-23-09:51:
Mounted Agents and command terminal pressure events must preserve the GPUI stage contract when crossing into Ghostty. Map Zero to none, Normal to normal, and Force to deep without clamping, fallback stages, logging, persistence, or raw input storage.
*/
pub(crate) fn ghostty_mouse_pressure_stage_from_gpui_stage(stage: PressureStage) -> u32 {
    match stage {
        PressureStage::Zero => GHOSTTY_MOUSE_PRESSURE_STAGE_NONE,
        PressureStage::Normal => GHOSTTY_MOUSE_PRESSURE_STAGE_NORMAL,
        PressureStage::Force => GHOSTTY_MOUSE_PRESSURE_STAGE_DEEP,
    }
}

/*
CDXC:Terminal 2026-06-23-09:32:
Mounted Running Agents terminal bodies must forward wheel deltas to Ghostty without inventing fallback behavior.

CDXC:Terminal 2026-06-23-09:41:
Agents and command terminal bodies share the same wheel-delta conversion: pixel deltas use raw GPUI pixels and set Ghostty ScrollMods precision bit 0, line deltas use raw GPUI lines with zero scroll mods, and keyboard modifiers are forwarded through mouse position input instead of encoded into ScrollMods.
*/
pub(crate) fn terminal_ghostty_scroll_delta(
    delta: ScrollDelta,
) -> (f64, f64, ghostty_kit::ffi::ghostty_input_scroll_mods_t) {
    match delta {
        ScrollDelta::Pixels(delta) => (
            f64::from(delta.x.as_f32()),
            f64::from(delta.y.as_f32()),
            GHOSTTY_SCROLL_PRECISION_MOD,
        ),
        ScrollDelta::Lines(delta) => (f64::from(delta.x), f64::from(delta.y), 0),
    }
}

pub(crate) fn agents_terminal_body_relative_mouse_position_for_slot(
    bounds_by_slot: &HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    slot_id: AgentsTerminalBodyMountSlotId,
    position: Point<Pixels>,
) -> Option<TerminalBodyMousePosition> {
    terminal_body_relative_mouse_position_for_slot(bounds_by_slot, slot_id, position)
}

pub(crate) fn command_terminal_body_relative_mouse_position_for_slot(
    bounds_by_slot: &HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    slot_id: CommandTerminalBodyMountSlotId,
    position: Point<Pixels>,
) -> Option<TerminalBodyMousePosition> {
    terminal_body_relative_mouse_position_for_slot(bounds_by_slot, slot_id, position)
}

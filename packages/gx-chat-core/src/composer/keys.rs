//! What a keystroke on the chat background means for the composer.
//!
//! Port of `packages/shared/session-chat-presentation/native-composer-keys.ts` together with the
//! two shared rule files it asked, `packages/core-ui/chat/session-chat-caret-navigation.ts` and
//! `session-chat-edit-shortcuts.ts`.
//!
//! CDXC:SessionChat 2026-09-18 WHY:
//! `detectghostexHotkeyPlatform` read `navigator`, which the QuickJS chat runtime did not have, so
//! the shared editing rules always answered as macOS there. On Windows and Linux the primary
//! modifier is Control, which under the macOS answer is also the terminal-chord modifier: ask
//! twice, once as Control (Ctrl+U/K/Y/A/E) and once as the primary chord, instead of forking the
//! rules themselves.

use serde::{Deserialize, Serialize};

/// A GPUI keystroke, as `gpui::Keystroke` names its key and modifiers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerKeyEvent {
    pub alt: bool,
    pub control: bool,
    pub key: String,
    pub platform: bool,
    pub shift: bool,
}

/// Which way the caret moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaretDirection {
    Left,
    Right,
    Up,
    Down,
}

/// How far one caret step goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaretUnit {
    Character,
    Word,
    Line,
    LineBoundary,
    Paragraph,
    Document,
}

/// One caret move, with whether it extends the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaretMovement {
    pub direction: CaretDirection,
    pub select: bool,
    pub unit: CaretUnit,
}

/// An editing command the composer performs on its own text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextEditCommand {
    Undo,
    Redo,
    DeleteAllLeft,
    DeleteAllRight,
    DeleteWordLeft,
    DeleteWordRight,
    KillLineLeft,
    KillLineRight,
    Yank,
    LineStart,
    LineEnd,
    SelectAll,
    Copy,
    Cut,
    Paste,
}

/// What the renderer should do with a key it caught on the chat background.
///
/// Serialized as the TypeScript union: `{kind: "caret", direction, select, unit}` or
/// `{command, kind: "edit"}`, in that key order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposerKeyIntent {
    Caret {
        kind: CaretTag,
        direction: CaretDirection,
        select: bool,
        unit: CaretUnit,
    },
    Edit {
        command: TextEditCommand,
        kind: EditTag,
    },
}

/// The literal `"caret"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaretTag {
    #[default]
    Caret,
}

/// The literal `"edit"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditTag {
    #[default]
    Edit,
}

/// Which primary modifier the renderer's platform uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyPlatform {
    Linux,
    #[default]
    Mac,
    Windows,
}

impl KeyPlatform {
    /// The spelling the renderer sends; anything else answers as macOS, the way a missing argument
    /// falls back to the default parameter.
    pub fn from_wire(value: &str) -> Self {
        match value {
            "linux" => Self::Linux,
            "windows" => Self::Windows,
            _ => Self::Mac,
        }
    }
}

/// A DOM keyboard event as the shared rules read one.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DomEvent {
    alt_key: bool,
    ctrl_key: bool,
    key: String,
    /// `undefined` unless the GPUI key is a single lowercase letter.
    key_code: Option<u32>,
    meta_key: bool,
    shift_key: bool,
}

/// GPUI key names for the keys the shared rules match by their DOM name.
fn dom_key_name(key: &str) -> &str {
    match key {
        "backspace" => "Backspace",
        "delete" => "Delete",
        "down" => "ArrowDown",
        "end" => "End",
        "home" => "Home",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "up" => "ArrowUp",
        other => other,
    }
}

fn dom_event(event: &ComposerKeyEvent, ctrl_key: bool, meta_key: bool) -> DomEvent {
    let mut characters = event.key.chars();
    let single_lowercase = match (characters.next(), characters.next()) {
        (Some(character), None) if character.is_ascii_lowercase() => Some(character),
        _ => None,
    };
    DomEvent {
        alt_key: event.alt,
        ctrl_key,
        key: dom_key_name(&event.key).to_string(),
        key_code: single_lowercase.map(|character| character.to_ascii_uppercase() as u32),
        meta_key,
        shift_key: event.shift,
    }
}

/// `shortcutKeyFromKeyboardEvent` for the synthetic events this file builds.
///
/// The events carry no `code`, so the physical-key fallbacks in
/// `packages/shared/keyboard-shortcut-key.ts` cannot fire and the answer is the Latin letter, the
/// printed ASCII character, or the key name lowercased.
fn shortcut_key(event: &DomEvent) -> String {
    if let Some(code) = event.key_code {
        if (65..=90).contains(&code) {
            return char::from_u32(code + 32)
                .map(String::from)
                .unwrap_or_default();
        }
    }
    let mut characters = event.key.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        if character.is_ascii_alphabetic() {
            return character.to_lowercase().to_string();
        }
        if character.is_ascii_digit() {
            return character.to_string();
        }
        let code = character as u32;
        if code > 0x20 && code < 0x7f {
            return character.to_lowercase().to_string();
        }
    }
    event.key.to_lowercase()
}

/// CDXC:SessionChat 2026-09-06 DECISION:
/// User: background Shift+Enter inserts a newline at the saved caret; arrows, including Option and
/// Cmd arrows, restore input focus and move the caret unless a picker owns the key.
fn caret_movement(event: &DomEvent) -> Option<CaretMovement> {
    if u8::from(event.alt_key) + u8::from(event.ctrl_key) + u8::from(event.meta_key) > 1 {
        return None;
    }
    let direction = match event.key.as_str() {
        "ArrowLeft" => CaretDirection::Left,
        "ArrowRight" => CaretDirection::Right,
        "ArrowUp" => CaretDirection::Up,
        "ArrowDown" => CaretDirection::Down,
        _ => return None,
    };
    let horizontal = matches!(direction, CaretDirection::Left | CaretDirection::Right);
    let unit = if event.meta_key {
        if horizontal {
            CaretUnit::LineBoundary
        } else {
            CaretUnit::Document
        }
    } else if event.alt_key || event.ctrl_key {
        if horizontal {
            CaretUnit::Word
        } else {
            CaretUnit::Paragraph
        }
    } else if horizontal {
        CaretUnit::Character
    } else {
        CaretUnit::Line
    };
    Some(CaretMovement {
        direction,
        select: event.shift_key,
        unit,
    })
}

/// CDXC:SessionChat 2026-09-08 DECISION:
/// User: Ctrl+U, Ctrl+K, Ctrl+Y, Ctrl+E, and Ctrl+A behave like the terminal in the chat input.
/// Use logical line boundaries, retain killed text for yank, and keep Cmd+A as select-all.
fn terminal_shortcut(event: &DomEvent) -> Option<TextEditCommand> {
    if !event.ctrl_key || event.meta_key || event.alt_key || event.shift_key {
        return None;
    }
    match shortcut_key(event).as_str() {
        "u" => Some(TextEditCommand::KillLineLeft),
        "k" => Some(TextEditCommand::KillLineRight),
        "y" => Some(TextEditCommand::Yank),
        "a" => Some(TextEditCommand::LineStart),
        "e" => Some(TextEditCommand::LineEnd),
        _ => None,
    }
}

/// CDXC:SessionChat 2026-09-07 DECISION:
/// User: Cmd+A and other text-editing shortcuts, including undo and redo, act on the composer even
/// when only the chat background is focused. Match editing chords explicitly so app shortcuts and
/// other controls keep their own keyboard ownership.
///
/// The rules answer as macOS here, because the chat runtime has no `navigator` to detect a platform
/// from; `composer_key_intent` is what makes Windows and Linux reach the primary chords.
fn editing_shortcut(event: &DomEvent) -> Option<TextEditCommand> {
    if let Some(terminal) = terminal_shortcut(event) {
        return Some(terminal);
    }
    let primary = event.meta_key && !event.ctrl_key;
    if (event.key == "Backspace" || event.key == "Delete") && !event.shift_key {
        let backward = event.key == "Backspace";
        if primary && !event.alt_key {
            return Some(if backward {
                TextEditCommand::DeleteAllLeft
            } else {
                TextEditCommand::DeleteAllRight
            });
        }
        if event.alt_key && !event.meta_key && !event.ctrl_key {
            return Some(if backward {
                TextEditCommand::DeleteWordLeft
            } else {
                TextEditCommand::DeleteWordRight
            });
        }
    }
    if !primary || event.alt_key {
        return None;
    }
    let key = shortcut_key(event);
    if key == "z" {
        return Some(if event.shift_key {
            TextEditCommand::Redo
        } else {
            TextEditCommand::Undo
        });
    }
    if key == "v" {
        return Some(TextEditCommand::Paste);
    }
    if event.shift_key {
        return None;
    }
    match key.as_str() {
        "a" => Some(TextEditCommand::SelectAll),
        "c" => Some(TextEditCommand::Copy),
        "x" => Some(TextEditCommand::Cut),
        "y" => Some(TextEditCommand::Redo),
        _ => None,
    }
}

/// What a background keystroke means, or `None` when the composer should not claim it.
pub fn composer_key_intent(
    event: &ComposerKeyEvent,
    platform: KeyPlatform,
) -> Option<ComposerKeyIntent> {
    if let Some(caret) = caret_movement(&dom_event(event, event.control, event.platform)) {
        return Some(ComposerKeyIntent::Caret {
            kind: CaretTag::Caret,
            direction: caret.direction,
            select: caret.select,
            unit: caret.unit,
        });
    }
    let passes: Vec<DomEvent> = if platform == KeyPlatform::Mac {
        vec![dom_event(event, event.control, event.platform)]
    } else {
        vec![
            dom_event(event, event.control, false),
            dom_event(event, false, event.control),
        ]
    };
    for (index, pass) in passes.iter().enumerate() {
        // The second pass only exists for the primary chords; it must not re-read a terminal chord.
        if index > 0 && terminal_shortcut(&passes[0]).is_some() {
            break;
        }
        if let Some(command) = editing_shortcut(pass) {
            return Some(ComposerKeyIntent::Edit {
                command,
                kind: EditTag::Edit,
            });
        }
    }
    None
}

/// The chords this renderer answers itself, with the kill buffer it keeps.
pub fn composer_terminal_command(event: &ComposerKeyEvent) -> Option<TextEditCommand> {
    match terminal_shortcut(&dom_event(event, event.control, false)) {
        Some(command @ TextEditCommand::KillLineLeft)
        | Some(command @ TextEditCommand::KillLineRight)
        | Some(command @ TextEditCommand::Yank) => Some(command),
        _ => None,
    }
}

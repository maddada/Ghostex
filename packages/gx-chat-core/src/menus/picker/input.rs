//! Keys and the wheel in the quick picker.
//!
//! Port of `packages/shared/session-chat-presentation/model-picker-input.ts`. The wheel
//! accumulator kept its own clock there; here every call takes `now` from
//! [`crate::ChatContext`], so the same inputs always produce the same steps.

use serde::{Deserialize, Serialize};

/// A move in the picker's grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PickerDirection {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

/// A key the picker's footer lights up.
///
/// `EnterAlternate` is Shift+Enter: commit the same choice to this session only where the agent
/// can; Enter saves the default. See [`super::model_pick_scope`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PickerControl {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Enter,
    EnterAlternate,
    Escape,
}

impl PickerControl {
    /// The wire spelling the document carries in `pressed`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
            Self::Enter => "Enter",
            Self::EnterAlternate => "EnterAlternate",
            Self::Escape => "Escape",
        }
    }

    /// The control with this spelling, for an action the renderer sends already resolved
    /// (`modelPickerControl`).
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "ArrowUp" => Some(Self::ArrowUp),
            "ArrowDown" => Some(Self::ArrowDown),
            "ArrowLeft" => Some(Self::ArrowLeft),
            "ArrowRight" => Some(Self::ArrowRight),
            "Enter" => Some(Self::Enter),
            "EnterAlternate" => Some(Self::EnterAlternate),
            "Escape" => Some(Self::Escape),
            _ => None,
        }
    }
}

impl From<PickerDirection> for PickerControl {
    fn from(value: PickerDirection) -> Self {
        match value {
            PickerDirection::ArrowUp => Self::ArrowUp,
            PickerDirection::ArrowDown => Self::ArrowDown,
            PickerDirection::ArrowLeft => Self::ArrowLeft,
            PickerDirection::ArrowRight => Self::ArrowRight,
        }
    }
}

/// One key press as the renderer reports it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerKeyInput {
    pub key: String,
    /// The physical key, which is what the held-key map is keyed by when it is there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default)]
    pub is_composing: bool,
    #[serde(default)]
    pub alt_key: bool,
    #[serde(default)]
    pub ctrl_key: bool,
    #[serde(default)]
    pub meta_key: bool,
    #[serde(default)]
    pub shift_key: bool,
}

/// The `controls` table, in its own order so a lookup is the same both ways.
fn control_for_name(key: &str) -> Option<PickerControl> {
    match key {
        "ArrowUp" => Some(PickerControl::ArrowUp),
        "ArrowDown" => Some(PickerControl::ArrowDown),
        "ArrowLeft" => Some(PickerControl::ArrowLeft),
        "ArrowRight" => Some(PickerControl::ArrowRight),
        "Enter" => Some(PickerControl::Enter),
        "Escape" => Some(PickerControl::Escape),
        "h" => Some(PickerControl::ArrowLeft),
        "j" => Some(PickerControl::ArrowDown),
        "k" => Some(PickerControl::ArrowUp),
        "l" => Some(PickerControl::ArrowRight),
        "," => Some(PickerControl::ArrowLeft),
        "." => Some(PickerControl::ArrowRight),
        _ => None,
    }
}

/// `modelPickerControlForKey`.
///
/// CDXC:SessionChat 2026-09-05 DECISION:
/// User: support H/J/K/L alongside arrows and light up the matching footer key when used.
/// Additional effort shortcuts remain unadvertised. Modifier chords belong to the application's
/// configured hotkeys.
pub fn model_picker_control_for_key(event: &PickerKeyInput) -> Option<PickerControl> {
    if event.is_composing || event.alt_key || event.ctrl_key || event.meta_key {
        return None;
    }
    if event.key == "Enter" && event.shift_key {
        return Some(PickerControl::EnterAlternate);
    }
    control_for_name(&event.key).or_else(|| control_for_name(&event.key.to_lowercase()))
}

/// One wheel event as the renderer reports it, with the clock the host read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPickerWheelInput {
    pub delta_x: f64,
    pub delta_y: f64,
    #[serde(default)]
    pub delta_mode: Option<i64>,
    #[serde(default)]
    pub shift_key: bool,
    #[serde(default)]
    pub ctrl_key: bool,
    #[serde(default)]
    pub meta_key: bool,
    pub height: f64,
    pub now: f64,
}

/// The wheel accumulator: 40 px of travel is one step, and steps are at least 100/0.7 ms apart.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelPickerWheelNavigation {
    last_event: f64,
    last_step: f64,
    distance: f64,
    direction: Option<PickerDirection>,
}

impl Default for ModelPickerWheelNavigation {
    fn default() -> Self {
        Self {
            last_event: 0.0,
            last_step: f64::NEG_INFINITY,
            distance: 0.0,
            direction: None,
        }
    }
}

impl ModelPickerWheelNavigation {
    /// `update`: the direction to move now, or `None` while the gesture is still accumulating.
    pub fn update(&mut self, event: &ModelPickerWheelInput) -> Option<PickerDirection> {
        if event.ctrl_key || event.meta_key {
            return None;
        }
        let unit = match event.delta_mode {
            Some(1) => 20.0,
            Some(2) => event.height,
            _ => 1.0,
        };
        // `!value` in JavaScript is true for `0` and for `NaN` alike.
        let falsy = |value: f64| value == 0.0 || value.is_nan();
        let dx = if event.shift_key && falsy(event.delta_x) {
            event.delta_y
        } else {
            event.delta_x
        } * unit;
        let dy = if event.shift_key { 0.0 } else { event.delta_y } * unit;
        if falsy(dx) && falsy(dy) {
            return None;
        }
        let now = event.now;
        let horizontal = dx.abs() > dy.abs();
        let delta = if horizontal { dx } else { dy };
        let next_direction = if horizontal {
            if delta > 0.0 {
                PickerDirection::ArrowRight
            } else {
                PickerDirection::ArrowLeft
            }
        } else if delta > 0.0 {
            PickerDirection::ArrowDown
        } else {
            PickerDirection::ArrowUp
        };
        if now - self.last_event > 180.0 || Some(next_direction) != self.direction {
            self.distance = 0.0;
            self.last_step = f64::NEG_INFINITY;
        }
        self.last_event = now;
        self.direction = Some(next_direction);
        self.distance += delta.abs();
        if self.distance < 40.0 || now - self.last_step < 100.0 / 0.7 {
            return None;
        }
        self.distance = 0.0;
        self.last_step = now;
        self.direction
    }
}

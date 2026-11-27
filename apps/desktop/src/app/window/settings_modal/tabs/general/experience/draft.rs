//! What the Experience page is trying out: the settings a level card, an area's level or a Custom
//! switch changed but Apply has not saved yet, and how each part of the preview that change showed
//! or hid is moving.
use super::super::super::super::store::SettingsValues;
use crate::fold_motion::{FoldMotion, bezier};
use ghostex_settings_catalog::experience::{self, AREAS};
use ghostex_settings_catalog::power_user_flags;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use web_time::Instant;

/// How long a part that appears takes to grow in.
const GROW: Duration = Duration::from_millis(450);
/// How long the highlight around a part that just appeared lasts.
const GLOW: Duration = Duration::from_millis(1600);
/// How long a part that goes away stays outlined before it folds out.
const HOLD: Duration = Duration::from_millis(550);
/// How long it then takes to fold out.
const SHRINK: Duration = Duration::from_millis(400);

/// A part of the preview one or more of the area settings shows or hides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Part {
    ModePicker,
    Maximize,
    TerminalButton,
    /// The folded line between the first message and its reply.
    FoldedLine,
    /// That line's tool call count (Calm Quiet View names only reports).
    FoldedToolCount,
    /// The work between them, open (no Quiet View, or Detailed view).
    OpenWork,
    /// The running turn as one line with its time (Calm Quiet View).
    RunningCalm,
    /// The running turn folded into one line with its count.
    RunningFolded,
    /// The running turn open, its tool calls in view.
    RunningOpen,
    FileEditPreview,
    ProjectIcons,
    DiffStats,
    FileCount,
    LastActive,
    /// Agent icons only on hover (`hideSessionAgentIconUntilHover`).
    AgentIconsOnHover,
    /// Browser favicons only on hover.
    FaviconsOnHover,
}

/// Every part and whether `value` shows it.
fn parts(value: &dyn Fn(&str) -> Value) -> Vec<(Part, bool)> {
    let on = |key: &str| value(key).as_bool() == Some(true);
    let quiet = on("chatQuietViewCoordinators");
    let calm = on(experience::QUIET_VIEW_CALM_KEY);
    let last_open = on("chatQuietViewExpandLast");
    let detailed = on("sessionChatDetailedView");
    let running_calm = quiet && calm;
    let running_folded = quiet && !calm && !last_open && !detailed;
    vec![
        (Part::ModePicker, on(experience::COMPOSER_MODE_PICKER_KEY)),
        (Part::Maximize, on(experience::COMPOSER_MAXIMIZE_KEY)),
        (
            Part::TerminalButton,
            on(experience::COMPOSER_TERMINAL_BUTTON_KEY),
        ),
        (Part::FoldedLine, quiet),
        (Part::FoldedToolCount, !calm),
        (Part::OpenWork, !quiet || detailed),
        (Part::RunningCalm, running_calm),
        (Part::RunningFolded, running_folded),
        (Part::RunningOpen, !running_calm && !running_folded),
        (Part::FileEditPreview, on("sessionChatFileEditPreviews")),
        (Part::ProjectIcons, on("showProjectIcons")),
        (Part::DiffStats, !on("hideProjectHeaderDiffStats")),
        (Part::FileCount, on("showProjectEditorDiffFileCount")),
        (Part::LastActive, !on("hideLastActiveTimeOnSessionCards")),
        (
            Part::AgentIconsOnHover,
            on("hideSessionAgentIconUntilHover"),
        ),
        (Part::FaviconsOnHover, on("hideBrowserFaviconUntilHover")),
    ]
}

/// How a part is drawn this frame.
#[derive(Clone, Copy)]
pub(super) struct Look {
    /// How much of its size it takes, 0 (gone) to 1.
    pub(super) size: f32,
    /// The strength of the highlight around a part that just appeared, 0 to 1.
    pub(super) glow: f32,
    /// Outlined as going away.
    pub(super) leaving: bool,
}

#[derive(Default)]
pub(crate) struct ExperienceDraft {
    /// The tried settings that differ from the saved ones.
    pending: HashMap<&'static str, Value>,
    /// Each area's Custom switches, open or folding.
    pub(super) custom: HashMap<&'static str, FoldMotion>,
    /// The area the pointer is on, lit in the preview.
    pub(super) hovered: Option<&'static str>,
    /// When each part last appeared (`true`) or went away.
    changes: HashMap<Part, (bool, Instant)>,
    /// Apply just saved what was tried.
    pub(super) saved: bool,
}

impl ExperienceDraft {
    /// A setting as tried: the pending value, else the saved one.
    pub(super) fn value(&self, saved: &SettingsValues, key: &str) -> Value {
        self.pending
            .get(key)
            .cloned()
            .unwrap_or_else(|| saved.value(key))
    }

    /// The tried settings that differ from the saved ones, in the page's order, then the level
    /// picked for the whole app (`power_user_flags::LEVEL_KEY`, CDXC:Settings 2026-11-27 in the
    /// settings catalog's experience.rs `writes_all`).
    pub(super) fn changed(&self, saved: &SettingsValues) -> Vec<(&'static str, Value)> {
        AREAS
            .iter()
            .flat_map(|area| area.controls.iter().map(|control| control.key))
            .chain(std::iter::once(power_user_flags::LEVEL_KEY))
            .filter_map(|key| {
                let value = self.pending.get(key)?;
                (*value != saved.value(key)).then(|| (key, value.clone()))
            })
            .collect()
    }

    /// Tries `writes` on top of what is tried already; `animate` moves the preview's parts.
    pub(super) fn try_writes(
        &mut self,
        saved: &SettingsValues,
        writes: Vec<(&'static str, Value)>,
        animate: bool,
    ) {
        let before = parts(&|key| self.value(saved, key));
        for (key, value) in writes {
            if value == saved.value(key) {
                self.pending.remove(key);
            } else {
                self.pending.insert(key, value);
            }
        }
        self.saved = false;
        self.moved(before, saved, animate);
    }

    /// Drops what was tried; the preview moves back to the saved settings.
    pub(super) fn cancel(&mut self, saved: &SettingsValues, animate: bool) {
        let before = parts(&|key| self.value(saved, key));
        self.pending.clear();
        self.saved = false;
        self.moved(before, saved, animate);
    }

    /// What Apply writes; the draft then holds nothing (the preview already shows it).
    pub(super) fn take_for_apply(&mut self, saved: &SettingsValues) -> Vec<(&'static str, Value)> {
        let writes = self.changed(saved);
        self.pending.clear();
        self.saved = !writes.is_empty();
        writes
    }

    fn moved(&mut self, before: Vec<(Part, bool)>, saved: &SettingsValues, animate: bool) {
        let after = parts(&|key| self.value(saved, key));
        let now = Instant::now();
        for ((part, was), (_, is)) in before.into_iter().zip(after) {
            if was != is {
                if animate {
                    self.changes.insert(part, (is, now));
                } else {
                    self.changes.remove(&part);
                }
            }
        }
    }

    /// How `part` is drawn now that the tried settings `show` it or not.
    pub(super) fn look(&self, part: Part, show: bool) -> Look {
        let settled = Look {
            size: if show { 1.0 } else { 0.0 },
            glow: 0.0,
            leaving: false,
        };
        let Some((appeared, at)) = self.changes.get(&part).copied() else {
            return settled;
        };
        if appeared != show {
            return settled;
        }
        let elapsed = at.elapsed();
        let fraction = |from: Duration, length: Duration| {
            (elapsed.saturating_sub(from).as_secs_f32() / length.as_secs_f32()).min(1.0)
        };
        if appeared {
            Look {
                size: bezier(fraction(Duration::ZERO, GROW), 0.22, 1.0, 0.36, 1.0),
                glow: 1.0 - fraction(Duration::ZERO, GLOW),
                leaving: false,
            }
        } else if elapsed < HOLD + SHRINK {
            Look {
                size: 1.0 - bezier(fraction(HOLD, SHRINK), 0.22, 1.0, 0.36, 1.0),
                glow: 0.0,
                leaving: true,
            }
        } else {
            settled
        }
    }

    /// Whether any part is still moving, so the page asks for another frame.
    pub(super) fn animating(&self) -> bool {
        let longest = GLOW.max(HOLD + SHRINK);
        self.changes.values().any(|(_, at)| at.elapsed() < longest)
    }

    /// Every part and whether the tried settings show it.
    pub(super) fn parts(&self, saved: &SettingsValues) -> HashMap<Part, bool> {
        parts(&|key| self.value(saved, key)).into_iter().collect()
    }
}

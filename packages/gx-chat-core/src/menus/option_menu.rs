//! What the options dropdown shows for one descriptor, and which descriptors it shows at all.
//!
//! Port of `packages/shared/session-chat-presentation/option-menu.ts`.

use crate::menus::option_catalog::{
    OptionCategory, OptionDescriptor, OptionDispatch, SessionOptionCatalog, CODEX_PERMISSIONS_ID,
};
use crate::menus::option_values::{
    option_choice_sections, option_tracks_value, ChoiceSection, OptionState,
};

/// What the menu can do about the current session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OptionCaps {
    /// The session can be sent a raw key.
    pub can_send_key: bool,
    /// The daemon can apply a model choice, so the picker rows are offered.
    pub can_pick_model: bool,
    /// The agent accepts queued option changes (Claude and Codex), so Plan and Fast stay live
    /// while the agent is working.
    pub queued_controls: bool,
}

/// `isShiftTabModeCycler`: the descriptor the composer's Mode button owns instead of the options
/// pill. That is the Shift+Tab mode cycle of Claude, Cursor, Grok and Antigravity, and Codex's
/// permission mode (`/permissions`).
pub fn is_mode_button(descriptor: &OptionDescriptor) -> bool {
    descriptor.category == OptionCategory::Mode
        && (matches!(&descriptor.dispatch, OptionDispatch::CyclicKeySteps { key } if key == "shift-tab")
            || descriptor.id == CODEX_PERMISSIONS_ID)
}

/// `isCodexPlanModeToggle`.
pub fn is_codex_plan_mode_toggle(descriptor: &OptionDescriptor) -> bool {
    descriptor.id == "mode" && matches!(descriptor.dispatch, OptionDispatch::ToggleCommand { .. })
}

/// One run of descriptors that share a heading.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionSection {
    pub label: String,
    pub description: Option<String>,
    pub descriptors: Vec<OptionDescriptor>,
}

/// `optionMenuSections`: consecutive descriptors with the same label merge into one section.
pub fn option_menu_sections(descriptors: &[OptionDescriptor]) -> Vec<OptionSection> {
    let mut sections: Vec<OptionSection> = Vec::new();
    for descriptor in descriptors {
        match sections.last_mut() {
            Some(last) if last.label == descriptor.label => {
                last.descriptors.push(descriptor.clone());
            }
            _ => sections.push(OptionSection {
                label: descriptor.label.clone(),
                description: descriptor.description.clone(),
                descriptors: vec![descriptor.clone()],
            }),
        }
    }
    sections
}

/// `visibleSessionChatOptions`: the descriptors this session can actually act on.
pub fn visible_options(
    descriptors: &[OptionDescriptor],
    caps: OptionCaps,
) -> Vec<OptionDescriptor> {
    descriptors
        .iter()
        .filter(|descriptor| {
            let needs_key = matches!(
                descriptor.dispatch,
                OptionDispatch::Key { .. }
                    | OptionDispatch::BoundedKeySteps { .. }
                    | OptionDispatch::CyclicKeySteps { .. }
            );
            let key_ok = !needs_key
                || caps.can_send_key
                || (caps.queued_controls && descriptor.id == "mode");
            let picker_ok =
                !matches!(descriptor.dispatch, OptionDispatch::ModelPicker) || caps.can_pick_model;
            key_ok && picker_ok
        })
        .cloned()
        .collect()
}

/// `sessionChatOptionsMayResolve`: whether any model in the lineup offers an option this session
/// could show, which is what keeps the pill reserved while the model is still unknown.
pub fn options_may_resolve(catalog: &SessionOptionCatalog, can_send_key: bool) -> bool {
    catalog.model.choice_list().iter().any(|choice| {
        catalog
            .options_for_model(&choice.value)
            .iter()
            .any(|descriptor| {
                !is_mode_button(descriptor)
                    && (can_send_key
                        || !matches!(
                            descriptor.dispatch,
                            OptionDispatch::Key { .. }
                                | OptionDispatch::BoundedKeySteps { .. }
                                | OptionDispatch::CyclicKeySteps { .. }
                        ))
            })
    })
}

/// What one descriptor draws as.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionRows {
    /// A single row that runs something.
    Action { label: String },
    /// A switch row that keeps the menu open.
    Toggle {
        label: String,
        checked: bool,
        /// The value the row sends, which is the opposite of what it shows.
        value: String,
        disabled: bool,
        /// Leaving Plan mode is a Shift+Tab, not the `/plan` command again.
        exit_plan: bool,
    },
    /// A choice list, possibly with submenus.
    Choices {
        sections: Vec<ChoiceSection>,
        current: Option<String>,
    },
}

/// `sessionChatOptionRows`.
pub fn option_rows(
    descriptor: &OptionDescriptor,
    state: &OptionState,
    caps: OptionCaps,
) -> OptionRows {
    let label = descriptor
        .action_label
        .clone()
        .unwrap_or_else(|| descriptor.label.clone());
    if matches!(descriptor.dispatch, OptionDispatch::ModelPicker) && !caps.can_pick_model {
        return OptionRows::Action {
            label: descriptor
                .action_label
                .clone()
                .unwrap_or_else(|| "Open the CLI's model picker".to_string()),
        };
    }
    if matches!(descriptor.dispatch, OptionDispatch::ToggleCommand { .. })
        && descriptor.id == "fastMode"
    {
        let checked = state.get("fastMode").map(|entry| entry.value.as_str()) == Some("on");
        return OptionRows::Toggle {
            label,
            checked,
            value: if checked { "off" } else { "on" }.to_string(),
            disabled: false,
            exit_plan: false,
        };
    }
    if is_codex_plan_mode_toggle(descriptor) {
        let checked = state.get("mode").map(|entry| entry.value.as_str()) == Some("plan");
        return OptionRows::Toggle {
            label,
            checked,
            value: if checked { "default" } else { "plan" }.to_string(),
            disabled: !caps.queued_controls && checked && !caps.can_send_key,
            exit_plan: checked,
        };
    }
    if option_tracks_value(descriptor) {
        return OptionRows::Choices {
            sections: option_choice_sections(descriptor),
            current: state.get(&descriptor.id).map(|entry| entry.value.clone()),
        };
    }
    OptionRows::Action { label }
}

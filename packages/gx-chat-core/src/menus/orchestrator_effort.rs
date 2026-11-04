//! The effort levels an orchestrator is offered: in the New Orchestrator dialog (desktop and
//! phone) and in the effort pickers of an orchestrator's chat composer.
//!
//! CDXC:Coordinators 2026-10-17 DECISION:
//! User: "raise the effort level to xhigh for the orchestrator by default please, and allow just high or xhigh (no low/max/ultracode here in this pop up or in the effort level selection inside the orchestrator chat composer please)." An orchestrator is offered High and XHigh and starts on XHigh; an agent without XHigh is offered its two highest levels below Max (Antigravity: Medium and High) and starts on the higher one. Supersedes the 2026-09-30 decision that started a coordinator at medium effort.
//!
//! CDXC:Coordinators 2026-10-29 DECISION:
//! User: "allow picking medium here but default to high". An orchestrator is offered Medium, High and XHigh and starts on High; an agent without XHigh is offered its levels from Medium up to its highest below Max (Antigravity: Medium and High) and starts on High. Supersedes the 2026-10-17 rule above (High and XHigh, starting on XHigh), which stays as the record of that decision.
//! CDXC:Coordinators 2026-10-31 DECISION:
//! User: "please allow setting low as the orchestrator effort level". An orchestrator is offered Low as well: Low, Medium, High and XHigh, starting on High as before; an agent without XHigh is offered its levels from Low up to its highest below Max (Antigravity: Low, Medium and High). An agent or model without a Low level is offered from Medium up, as before. Supersedes the 2026-10-29 list above (it started at Medium).
//! SEE-ALSO: apps/desktop/src/app/window/new_coordinator_modal.rs (the dialog), server/src/ghostex_cli/coordinator/command.rs (`ghostex orchestrator options` and `create`, which the phone's sheet uses), server/src/coordinators/role.rs (`default_orchestrator_effort`).

/// Levels above XHigh that an orchestrator is never offered.
const BEYOND_ORCHESTRATOR: [&str; 3] = ["max", "ultra", "ultracode"];

/// The levels Ghostex offers an orchestrator out of `efforts` (an agent's levels, lowest first): Low
/// up to the agent's highest level below Max (Low, Medium, High and XHigh where it has them). An
/// agent without a Low level starts at Medium, and one without a Medium level is offered its three
/// highest levels below Max.
fn recommended_orchestrator_efforts<T: AsRef<str>>(efforts: &[T]) -> Vec<String> {
    let offered: Vec<&str> = efforts
        .iter()
        .map(AsRef::as_ref)
        .filter(|effort| !BEYOND_ORCHESTRATOR.contains(effort))
        .collect();
    let start = ["low", "medium"]
        .into_iter()
        .find_map(|level| offered.iter().position(|effort| *effort == level))
        .unwrap_or_else(|| offered.len().saturating_sub(3));
    offered[start..]
        .iter()
        .map(|effort| effort.to_string())
        .collect()
}

/// The levels an orchestrator's effort pickers list out of `efforts` (an agent's levels, lowest
/// first). `show_all` is the Show all effort levels setting (`orchestratorAllEfforts`): every level
/// the agent has. Otherwise the recommended levels, plus `current` (the level the session or form is
/// on) when it is one of the agent's levels, so a level picked earlier still shows as chosen.
///
/// CDXC:Coordinators 2026-10-20 DECISION:
/// User: "Please add a setting under the orchestration settings where the user can toggle on showing more effort levels. By default, we just show high and X high, but if he enables this, he can see all of the effort levels. But we need to mention there in the tooltip that it's not recommended to use anything lower than high ... make the settings advanced." Off by default, so the 2026-10-17 High/XHigh rule above stands; on, the pickers list every level the agent offers. The level an orchestrator starts on stays the recommended one either way.
/// SEE-ALSO: packages/settings-catalog/src/data/defaults.rs (`orchestratorAllEfforts`), server/src/session_chat_read.rs (the chat read carries it to every chat, the phone's too), apps/desktop/src/app/new_coordinator_modal_lifecycle.rs (the dialog), server/src/ghostex_cli/coordinator/command.rs (`ghostex orchestrator options`).
pub fn orchestrator_efforts<T: AsRef<str>>(
    efforts: &[T],
    show_all: bool,
    current: Option<&str>,
) -> Vec<String> {
    if show_all {
        return efforts
            .iter()
            .map(|effort| effort.as_ref().to_string())
            .collect();
    }
    let recommended = recommended_orchestrator_efforts(efforts);
    efforts
        .iter()
        .map(AsRef::as_ref)
        .filter(|effort| recommended.iter().any(|kept| kept == effort) || Some(*effort) == current)
        .map(str::to_string)
        .collect()
}

/// The level an orchestrator starts on: High, or the agent's highest recommended level at or below
/// High when it has no High (Medium, else its highest recommended level).
pub fn default_orchestrator_effort<T: AsRef<str>>(efforts: &[T]) -> Option<String> {
    let recommended = recommended_orchestrator_efforts(efforts);
    ["high", "medium"]
        .into_iter()
        .find(|level| recommended.iter().any(|effort| effort == level))
        .map(str::to_string)
        .or_else(|| recommended.last().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: [&str; 6] = ["low", "medium", "high", "xhigh", "max", "ultracode"];
    const CODEX: [&str; 5] = ["low", "medium", "high", "xhigh", "ultra"];
    const ANTIGRAVITY: [&str; 3] = ["low", "medium", "high"];

    #[test]
    fn offers_low_through_xhigh() {
        let offered = |efforts: &[&str]| orchestrator_efforts(efforts, false, None);
        assert_eq!(offered(&CLAUDE), ["low", "medium", "high", "xhigh"]);
        assert_eq!(offered(&CODEX), ["low", "medium", "high", "xhigh"]);
        assert_eq!(offered(&ANTIGRAVITY), ["low", "medium", "high"]);
        // A model without Low starts at Medium; Pi's extra levels below Low stay out.
        assert_eq!(
            offered(&["medium", "high", "xhigh", "max"]),
            ["medium", "high", "xhigh"]
        );
        assert_eq!(
            offered(&["off", "minimal", "low", "medium", "high", "xhigh", "max"]),
            ["low", "medium", "high", "xhigh"]
        );
    }

    #[test]
    fn starts_on_high_or_the_highest_level_at_or_below_it() {
        assert_eq!(
            default_orchestrator_effort(&CLAUDE).as_deref(),
            Some("high")
        );
        assert_eq!(default_orchestrator_effort(&CODEX).as_deref(), Some("high"));
        assert_eq!(
            default_orchestrator_effort(&ANTIGRAVITY).as_deref(),
            Some("high")
        );
        // No High: Medium; no Medium or High: the highest level offered.
        assert_eq!(
            default_orchestrator_effort(&["low", "medium", "xhigh"]).as_deref(),
            Some("medium")
        );
        assert_eq!(
            default_orchestrator_effort(&["xhigh", "max"]).as_deref(),
            Some("xhigh")
        );
        assert_eq!(default_orchestrator_effort::<&str>(&[]), None);
    }

    #[test]
    fn show_all_lists_everything_and_a_current_level_stays_listed() {
        assert_eq!(orchestrator_efforts(&CLAUDE, true, None), CLAUDE);
        assert_eq!(
            orchestrator_efforts(&CLAUDE, false, Some("max")),
            ["low", "medium", "high", "xhigh", "max"]
        );
        // The start level is the recommended one even while every level is listed.
        assert_eq!(
            default_orchestrator_effort(&CLAUDE).as_deref(),
            Some("high")
        );
    }
}

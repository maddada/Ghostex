//! The Ghostex note: the few sentences every agent session Ghostex starts is told about Ghostex,
//! and the browser preference it and `ghostex agent-guide` state.

use serde_json::Value;

use crate::paths::GxserverPaths;

/// CDXC:AgentGuide 2026-11-02 DECISION:
/// User (Q1065): inject it the way Marked handovers is injected, "but want it to be very short and point to `ghostex agent-guide`", mention "inline HTML and inline questions and inline charts & graphs in Ghostex", and that Ghostex skills (ghostex-visuals, the embedded browser one…) can be installed for the user when they need them. Agents use it only when asked, so the note says not to bring it up otherwise; everything else lives in `ghostex agent-guide` (ghostex_cli/agent_guide.rs). Issues #262 (an agent supervised helpers by polling instead of finding coordinators) and #263 (an agent opened an outside browser) are why other agents and the browser are named.
///
/// CDXC:AgentGuide 2026-11-06 DECISION:
/// User: "i hate saying 'inline html' and 'inline q' and inline graphs. can we have different name for those?", then "Let's say ghostex page / gx page, ghostex form / gx form, ghostex visuals / gx visuals", everywhere. The note names the three by those words, so "give me a gx page" or "ask me in a gx form" reaches `ghostex agent-guide`. Supersedes the 2026-11-02 "inline HTML and inline questions and inline charts & graphs" wording.
/// SEE-ALSO: server/src/agents/marked_handovers.rs (the launch channel it shares), skills/ghostex-agent-guide/SKILL.md (the same text for agents without a launch flag), server/src/ghostex_cli/agent_guide.rs.
pub const GHOSTEX_NOTE: &str = "You are running inside Ghostex. Ghostex can show Ghostex pages (gx page: HTML), Ghostex forms (gx form: questions) and Ghostex visuals (gx visuals: charts, graphs) in this chat, open pages you build in its own browser, run other agents, and install Ghostex skills for the user (such as ghostex-visuals or ghostex-embedded-browser-use). When the user asks for one of these, or about Ghostex, run `ghostex agent-guide` first; otherwise don't bring it up.";

const NOTE_SETTING_KEY: &str = "agentGhostexNote";
pub(crate) const PREVIEW_BROWSER_SETTING_KEY: &str = "agentPreviewBrowser";

/// Where the user wants agents to open a page they built (`agentPreviewBrowser`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewBrowser {
    Ask,
    Ghostex,
    System,
}

impl PreviewBrowser {
    pub(crate) fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("internal-browser") => Self::Ghostex,
            Some("system-default-browser") => Self::System,
            _ => Self::Ask,
        }
    }

    /// The sentence the note adds once the user has picked; nothing while agents still ask.
    pub(crate) fn note_sentence(self) -> Option<&'static str> {
        match self {
            Self::Ask => None,
            Self::Ghostex => Some("The user wants pages you build opened in the Ghostex browser."),
            Self::System => Some("The user wants pages you build opened in their default browser."),
        }
    }
}

/// Whether Tell agents about Ghostex is on (on unless the user turned it off), read fresh from the
/// shared settings file so a change reaches the next launch or resume without a gxserver restart.
pub(crate) fn ghostex_note_enabled(paths: &GxserverPaths) -> bool {
    // Unit tests assert exact commands, which must not depend on the user's settings.
    if cfg!(test) {
        return false;
    }
    crate::shared_settings_file::read_shared_settings(paths)
        .and_then(|settings| settings.get(NOTE_SETTING_KEY).and_then(Value::as_bool))
        .unwrap_or(true)
}

/// The user's Agents open pages in choice, read fresh like [`ghostex_note_enabled`].
pub(crate) fn agent_preview_browser(paths: &GxserverPaths) -> PreviewBrowser {
    if cfg!(test) {
        return PreviewBrowser::Ask;
    }
    let settings = crate::shared_settings_file::read_shared_settings(paths);
    PreviewBrowser::from_setting(
        settings
            .as_ref()
            .and_then(|settings| settings.get(PREVIEW_BROWSER_SETTING_KEY))
            .and_then(Value::as_str),
    )
}

/// The note with the browser sentence the user's choice adds.
pub(crate) fn ghostex_note_text(browser: PreviewBrowser) -> String {
    match browser.note_sentence() {
        Some(sentence) => format!("{GHOSTEX_NOTE} {sentence}"),
        None => GHOSTEX_NOTE.to_string(),
    }
}

/// What a session's launch adds to its agent's prompt: the Ghostex note while it is on, then the
/// Marked handovers note while that is Every session, as one text, since Claude keeps only its last
/// `--append-system-prompt-file` and Codex reads one `developer_instructions`.
pub(crate) fn launch_note_text(
    ghostex_note: Option<&str>,
    marked_handovers: bool,
) -> Option<String> {
    let parts = [
        ghostex_note,
        marked_handovers.then_some(super::MARKED_HANDOVERS_NOTE),
    ];
    let text = parts.into_iter().flatten().collect::<Vec<_>>().join("\n\n");
    (!text.is_empty()).then_some(text)
}

/// [`launch_note_text`] for the current settings.
pub(crate) fn current_launch_note_text(paths: &GxserverPaths) -> Option<String> {
    let note = ghostex_note_enabled(paths).then(|| ghostex_note_text(agent_preview_browser(paths)));
    launch_note_text(note.as_deref(), super::marked_handovers_enabled(paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_and_marked_handovers_join_into_one_text() {
        let note = ghostex_note_text(PreviewBrowser::Ask);
        assert_eq!(launch_note_text(None, false), None);
        assert_eq!(
            launch_note_text(Some(&note), false).as_deref(),
            Some(GHOSTEX_NOTE)
        );
        assert_eq!(
            launch_note_text(None, true).as_deref(),
            Some(super::super::MARKED_HANDOVERS_NOTE)
        );
        let both = launch_note_text(Some(&note), true).unwrap();
        assert!(both.starts_with(GHOSTEX_NOTE));
        assert!(both.ends_with(super::super::MARKED_HANDOVERS_NOTE));
    }

    #[test]
    fn a_picked_browser_is_stated_and_ask_adds_nothing() {
        assert_eq!(PreviewBrowser::from_setting(None), PreviewBrowser::Ask);
        assert_eq!(
            PreviewBrowser::from_setting(Some("ask")),
            PreviewBrowser::Ask
        );
        assert_eq!(ghostex_note_text(PreviewBrowser::Ask), GHOSTEX_NOTE);
        assert!(
            ghostex_note_text(PreviewBrowser::from_setting(Some("internal-browser")))
                .ends_with("opened in the Ghostex browser.")
        );
        assert!(
            ghostex_note_text(PreviewBrowser::from_setting(Some("system-default-browser")))
                .ends_with("opened in their default browser.")
        );
    }

    #[test]
    fn the_note_stays_short() {
        // About 105 tokens (characters / 4); the guide carries everything else.
        assert!(GHOSTEX_NOTE.len() < 480, "{}", GHOSTEX_NOTE.len());
    }
}

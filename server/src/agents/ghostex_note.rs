//! The Ghostex note: the few sentences every agent session Ghostex starts is told about Ghostex,
//! and the browser preference it and `ghostex agent-guide` state.

use serde_json::Value;

use crate::paths::GxserverPaths;

/// CDXC:AgentGuide 2026-11-02 DECISION:
/// User (Q1065): inject it the way Marked handovers is injected, "but want it to be very short and point to `ghostex agent-guide`", mention "inline HTML and inline questions and inline charts & graphs in Ghostex", and that Ghostex skills (ghostex-visuals, the embedded browser one…) can be installed for the user when they need them. Agents use it only when asked, so the note says not to bring it up otherwise; everything else lives in `ghostex agent-guide` (ghostex_cli/agent_guide.rs). Issues #262 (an agent supervised helpers by polling instead of finding coordinators) and #263 (an agent opened an outside browser) are why other agents and the browser are named.
///
/// CDXC:AgentGuide 2026-11-06 DECISION:
/// User: "i hate saying 'inline html' and 'inline q' and inline graphs. can we have different name for those?", then "Let's say ghostex page / gx page, ghostex form / gx form, ghostex visuals / gx visuals", everywhere. The note names the three by those words, so "give me a gx page" or "ask me in a gx form" reaches `ghostex agent-guide`. Supersedes the 2026-11-02 "inline HTML and inline questions and inline charts & graphs" wording.
/// CDXC:AgentGuide 2026-11-19 DECISION:
/// User picked "Live page" (Q4207, Q4208) for what the 2026-11-06 decision named Ghostex page / gx page, superseding that part of it. The note says Live pages and keeps the old words beside it, so "give me a gx page" or "inline HTML" still reaches `ghostex agent-guide`.
/// CDXC:AgentSkills 2026-11-20 DECISION:
/// User (Q4262 "B"): Ghostex's own skills are retired for CLI help, superseding the "Ghostex skills … can be installed for the user" part of Q1065 above: the note no longer offers skills to install, and `ghostex agent-guide` names the guide to read for each job.
/// SEE-ALSO: server/src/agents/marked_handovers.rs (the launch channel it shares), skills/ghostex/SKILL.md (the one Ghostex skill, which sends agents without a launch flag to the same guide), server/src/ghostex_cli/agent_guide.rs.
pub const GHOSTEX_NOTE: &str = "You are running inside Ghostex. Ghostex can show Live pages (HTML; also called gx page or inline HTML), Ghostex forms (gx form: questions) and Ghostex visuals (gx visuals: charts, graphs) in this chat, open pages you build in its own browser, run other agents and coordinators, and explain or change its own settings. When the user asks for one of these, or about Ghostex, run `ghostex agent-guide` first; otherwise don't bring it up.";

/// The paragraph after the Ghostex note that has agents keep a to-do for each step only the user
/// can do (server/src/session_needs_you/).
///
/// CDXC:AgentGuide 2026-11-16 DECISION:
/// User (Q60 "always"): agents add a Needs you to-do without being asked whenever a step only the
/// user can do blocks their work, matching "always, in this kind of case, put a needs you work
/// item for me so I can do it". One short paragraph in the note every session's prompt carries,
/// and the same in `ghostex agent-guide`; it is not behind a setting.
pub const NEEDS_YOU_NOTE: &str = "When a step can only be done by the user (a sudo command, a tap on their phone, a sign-in, a check by eye), add it with `ghostex needs-you add \"<what to do>\" [--command \"…\"]` and also list it under Needs you in your reply. Their answer arrives as \"Q<n>: Done\". If you see it done yourself, run `ghostex needs-you done Q<n>`.";

/// The paragraph after the Needs you one that has agents ask for a secret through Ghostex.
///
/// CDXC:AgentSkills 2026-11-17 DECISION:
/// User (Q1 "C"): `ghostex-secrets` no longer loads on its own (`disable-model-invocation: true`,
/// `allow_implicit_invocation: false`), so no session outside Ghostex reaches for it. A session
/// Ghostex starts learns the verb here instead; threads, which take no note, from their brief
/// (coordinators/brief.rs `thread_brief`).
pub const SECRETS_NOTE: &str = "When a task needs a secret only the user has (a password, a sudo password, an API key or token), run `ghostex secret request --label <what> --reason <why>`: the user types it on a private card and you get a private file holding it. Never ask for it in chat.";

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

/// The note with the browser sentence the user's choice adds, then the Needs you paragraph.
pub(crate) fn ghostex_note_text(browser: PreviewBrowser) -> String {
    match browser.note_sentence() {
        Some(sentence) => {
            format!("{GHOSTEX_NOTE} {sentence}\n\n{NEEDS_YOU_NOTE}\n\n{SECRETS_NOTE}")
        }
        None => format!("{GHOSTEX_NOTE}\n\n{NEEDS_YOU_NOTE}\n\n{SECRETS_NOTE}"),
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

/// [`launch_note_text`] for the current settings, followed, `with_extensions`, by the line of each
/// installed extension that may add one (extensions/agent_notes.rs) and, while Grok Bot is
/// connected, by the line that tells agents about `ghostex grokbot notify` (grokbot/mod.rs),
/// which coordinators get too.
pub(crate) fn current_launch_note_text(
    paths: &GxserverPaths,
    with_extensions: bool,
) -> Option<String> {
    note_text_with(
        paths,
        super::marked_handovers_enabled(paths),
        with_extensions,
    )
}

/// [`current_launch_note_text`] with the Marked handovers note decided by the caller: the text a
/// SessionStart hook hands an agent whose launch cannot carry it (activity.rs
/// `session_start_note_context`), where a thread always marks its handovers and a coordinator
/// follows its own setting.
pub(crate) fn note_text_with(
    paths: &GxserverPaths,
    marked_handovers: bool,
    with_extensions: bool,
) -> Option<String> {
    let note = ghostex_note_enabled(paths).then(|| ghostex_note_text(agent_preview_browser(paths)));
    let text = launch_note_text(note.as_deref(), marked_handovers);
    // Unit tests assert exact commands, which must not depend on the user's extensions.
    if cfg!(test) {
        return text;
    }
    let mut notes = if with_extensions {
        crate::extensions::agent_notes::installed_agent_notes(paths)
    } else {
        Vec::new()
    };
    notes.extend(crate::grokbot::agent_note(paths));
    with_extension_notes(text, &notes)
}

/// `text` with each extension's line after it, as one text.
fn with_extension_notes(text: Option<String>, notes: &[String]) -> Option<String> {
    let parts = text
        .into_iter()
        .chain(notes.iter().cloned())
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
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
            Some(note.as_str())
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
        assert_eq!(
            ghostex_note_text(PreviewBrowser::Ask),
            format!("{GHOSTEX_NOTE}\n\n{NEEDS_YOU_NOTE}\n\n{SECRETS_NOTE}")
        );
        assert!(
            ghostex_note_text(PreviewBrowser::from_setting(Some("internal-browser")))
                .contains("opened in the Ghostex browser.\n\n")
        );
        assert!(
            ghostex_note_text(PreviewBrowser::from_setting(Some("system-default-browser")))
                .contains("opened in their default browser.\n\n")
        );
    }

    #[test]
    fn the_note_stays_short() {
        // About 105 tokens (characters / 4); the guide carries everything else.
        assert!(GHOSTEX_NOTE.len() < 480, "{}", GHOSTEX_NOTE.len());
    }
}

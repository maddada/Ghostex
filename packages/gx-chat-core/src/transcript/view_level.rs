//! The chat's view level: exactly one of Summary, Normal and Detailed.
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User, on the Simple vs Detailed page: "ok i think let's remove verbose, and make detailed a different option. so u can only pick 1 between summary, normal (simple renamed to this), detailed. but when i turn off summary we always go back to normal", and for chats saved on Detailed: "keep them on detailed, i changed my mind no need for verbose". More actions > View offers one choice of Summary, Normal (Simple renamed) and Detailed, and picking one turns the others off. Turning Summary off by any route (the menu, the strip's chip, the hotkey) goes to Normal, never back to the level before it. Verbose is gone as a level: a chat saved on Verbose reads as Detailed, a chat saved on Detailed stays Detailed, and a chat saved with Summary keeps Summary. The per-chat records stay the same three (`summary`, `simple`, `verbose`), read here into the one choice, so nothing resets. Supersedes 2026-10-29's Simple, Detailed or Verbose level plus a separate Summary switch (cleanup Q11).
//!
//! CDXC:SessionChat 2026-11-02 SEE-ALSO:
//! The renderers draw from `viewLevel` only: desktop apps/desktop/src/app/native_chat/appearance.rs and actions.rs (View menu), phone apps/mobile/app/src/chat/native/composer/menus.ts and transcript/Transcript.tsx.

use crate::state::TranscriptViewState;

pub const SUMMARY: &str = "summary";
pub const NORMAL: &str = "normal";
pub const DETAILED: &str = "detailed";

/// The chat's one view level from its saved records and the Default to Detailed view setting.
pub fn view_level(view: &TranscriptViewState) -> &'static str {
    if view.summary_mode {
        return SUMMARY;
    }
    // A chat saved on the retired Verbose is Detailed.
    if view.verbose_override == Some(true) {
        return DETAILED;
    }
    match view.simple_override {
        Some(true) => NORMAL,
        Some(false) => DETAILED,
        None if view.detailed_default => DETAILED,
        None => NORMAL,
    }
}

/// The level a `setDetailLevel` names, with the retired spellings: `simple` is Normal and
/// `verbose` is Detailed.
pub fn parse(level: &str) -> &'static str {
    match level {
        SUMMARY => SUMMARY,
        DETAILED | "verbose" => DETAILED,
        _ => NORMAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(
        summary: bool,
        simple: Option<bool>,
        verbose: Option<bool>,
        detailed: bool,
    ) -> &'static str {
        let mut view = TranscriptViewState::default();
        view.summary_mode = summary;
        view.simple_override = simple;
        view.verbose_override = verbose;
        view.detailed_default = detailed;
        view_level(&view)
    }

    #[test]
    fn saved_records_migrate_into_one_level() {
        assert_eq!(
            level(true, Some(false), Some(true), false),
            SUMMARY,
            "Summary keeps Summary"
        );
        assert_eq!(
            level(false, Some(false), Some(true), false),
            DETAILED,
            "Verbose becomes Detailed"
        );
        assert_eq!(
            level(false, Some(false), Some(false), false),
            DETAILED,
            "Detailed stays Detailed"
        );
        assert_eq!(
            level(false, Some(true), Some(false), true),
            NORMAL,
            "Simple is Normal"
        );
        assert_eq!(
            level(false, None, None, false),
            NORMAL,
            "a new chat opens Normal"
        );
        assert_eq!(
            level(false, None, None, true),
            DETAILED,
            "unless Default to Detailed view is on"
        );
    }

    #[test]
    fn retired_level_names_still_parse() {
        assert_eq!(parse("simple"), NORMAL);
        assert_eq!(parse("verbose"), DETAILED);
        assert_eq!(parse("summary"), SUMMARY);
    }
}

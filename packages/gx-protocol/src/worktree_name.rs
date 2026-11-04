//! The name a new worktree's branch and folder take: Add Worktree's slug of the first prompt, and
//! the new chat's worktree tray, which puts a ticket, issue or PR its first message names in front.
//!
//! One rule for both, here, because the dialog computes its name hint on the client
//! (`packages/gx-core/src/git_menu/worktree.rs` re-exports it) while the tray's name is made by
//! gxserver when the message is sent (`server/src/worktree_branch_name.rs`).

/// At most this many words of the prompt make the name.
pub const WORKTREE_NAME_MAX_WORDS: usize = 6;
/// The name is cut to this many characters.
pub const WORKTREE_NAME_MAX_CHARS: usize = 48;
/// The name of a prompt with no usable words (only emoji, punctuation or a script other than
/// ASCII letters and digits).
pub const WORKTREE_NAME_FALLBACK: &str = "worktree";

/// `prompt`'s words as the slug keeps them: lowercase ASCII letters and digits, quotes dropped,
/// anything else a separator.
fn slug_words(prompt: &str) -> Vec<String> {
    let lowered: String = prompt
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !matches!(ch, '`' | '\'' | '"'))
        .collect();
    let mut dashed = String::new();
    let mut last_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            dashed.push(ch);
            last_dash = false;
        } else if !last_dash {
            dashed.push('-');
            last_dash = true;
        }
    }
    dashed
        .trim_matches('-')
        .split('-')
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Cuts a joined name to [`WORKTREE_NAME_MAX_CHARS`], or `None` when nothing is left.
fn clip(joined: &str) -> Option<String> {
    let clipped: String = joined.chars().take(WORKTREE_NAME_MAX_CHARS).collect();
    let clipped = clipped.trim_end_matches('-').to_string();
    (!clipped.is_empty()).then_some(clipped)
}

/// `gpuiWorktreeSlugFromPrompt`: the name hint gxserver derives the branch and folder from. The
/// first six words of the prompt, cut to 48 characters; `worktree` when it has none.
pub fn worktree_slug_from_prompt(prompt: &str) -> String {
    let words = slug_words(prompt);
    let joined = words
        .iter()
        .take(WORKTREE_NAME_MAX_WORDS)
        .cloned()
        .collect::<Vec<_>>()
        .join("-");
    clip(&joined).unwrap_or_else(|| WORKTREE_NAME_FALLBACK.to_string())
}

/// A ticket, issue or PR the first message names, as the worktree's name starts with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeNameReference<'a> {
    /// What leads the name: `spx-123` for a Linear issue, `218` for a GitHub issue or PR (the
    /// shapes Work mode reads back out of a branch, `server/src/work_mode/branch.rs`).
    pub key: &'a str,
    /// The text in the message that named it (`SPX-123`, `#218`, a URL), left out of the words.
    pub matched: &'a str,
    /// Its title, when one is already known; its words follow the key instead of the message's.
    pub title: Option<&'a str>,
}

/// The name of a new chat's worktree.
///
/// CDXC:Worktrees 2026-10-31 DECISION:
/// User (Q830): the branch name is "automatic and not shown in the ui basically. also if any ticket or pr or issue title is mentioned then use that in the name please in the beginning (like spx-123-xyz-abc)". Without a reference this is Add Worktree's slug of the message. With one, the key leads (`spx-123`, `218`), followed by the words of its title when gxserver already knows it, else the message's own words without the reference, the same six words and 48 characters. A message with no usable words and no reference is named `worktree`, as Add Worktree names one.
pub fn worktree_name_for_message(
    message: &str,
    reference: Option<&WorktreeNameReference>,
) -> String {
    let Some(reference) = reference else {
        return worktree_slug_from_prompt(message);
    };
    let key_words = slug_words(reference.key);
    if key_words.is_empty() {
        return worktree_slug_from_prompt(message);
    }
    let title_words = reference.title.map(slug_words).unwrap_or_default();
    let words = if title_words.is_empty() {
        slug_words(&message.replacen(reference.matched, " ", 1))
    } else {
        title_words
    };
    // A title or message that repeats the key (`SPX-123 SPX-123 fix`) does not repeat it in the name.
    let mut rest: &[String] = &words;
    while rest.len() >= key_words.len() && rest[..key_words.len()] == key_words[..] {
        rest = &rest[key_words.len()..];
    }
    let joined = key_words
        .iter()
        .chain(rest.iter().take(WORKTREE_NAME_MAX_WORDS))
        .cloned()
        .collect::<Vec<_>>()
        .join("-");
    clip(&joined).unwrap_or_else(|| WORKTREE_NAME_FALLBACK.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference<'a>(
        key: &'a str,
        matched: &'a str,
        title: Option<&'a str>,
    ) -> WorktreeNameReference<'a> {
        WorktreeNameReference {
            key,
            matched,
            title,
        }
    }

    #[test]
    fn a_plain_message_keeps_add_worktrees_slug() {
        assert_eq!(
            worktree_name_for_message("Fix the login redirect loop on Safari please", None),
            "fix-the-login-redirect-loop-on"
        );
        assert_eq!(
            worktree_name_for_message("Don't break \"quotes\"", None),
            "dont-break-quotes"
        );
    }

    #[test]
    fn a_message_with_no_usable_words_is_named_worktree() {
        assert_eq!(worktree_name_for_message("🚀🚀 — ✨", None), "worktree");
        assert_eq!(worktree_name_for_message("修复登录", None), "worktree");
        assert_eq!(worktree_name_for_message("", None), "worktree");
    }

    #[test]
    fn a_linear_key_leads_the_message_words() {
        let named = reference("spx-123", "SPX-123", None);
        assert_eq!(
            worktree_name_for_message("SPX-123 fix the copy link button", Some(&named)),
            "spx-123-fix-the-copy-link-button"
        );
        assert_eq!(
            worktree_name_for_message("Please look at SPX-123: copy link breaks", Some(&named)),
            "spx-123-please-look-at-copy-link-breaks"
        );
    }

    #[test]
    fn a_known_title_replaces_the_message_words() {
        let named = reference(
            "spx-123",
            "SPX-123",
            Some("Copy link button breaks on Safari"),
        );
        assert_eq!(
            worktree_name_for_message("do SPX-123", Some(&named)),
            "spx-123-copy-link-button-breaks-on-safari"
        );
        let repeated = reference("spx-123", "SPX-123", Some("SPX-123 Copy link"));
        assert_eq!(
            worktree_name_for_message("SPX-123", Some(&repeated)),
            "spx-123-copy-link"
        );
    }

    #[test]
    fn a_github_number_leads_and_its_url_is_left_out() {
        let issue = reference("218", "#218", None);
        assert_eq!(
            worktree_name_for_message("Fix #218 arabic plan cards", Some(&issue)),
            "218-fix-arabic-plan-cards"
        );
        let url = "https://github.com/acme/web/pull/42";
        let pr = reference("42", url, None);
        assert_eq!(
            worktree_name_for_message(&format!("Review {url} and fix the nits"), Some(&pr)),
            "42-review-and-fix-the-nits"
        );
    }

    #[test]
    fn a_reference_alone_is_just_its_key_and_long_names_are_cut() {
        assert_eq!(
            worktree_name_for_message("SPX-123", Some(&reference("spx-123", "SPX-123", None))),
            "spx-123"
        );
        let long = reference(
            "spx-123",
            "SPX-123",
            Some("an extraordinarily lengthy title words keep going"),
        );
        let name = worktree_name_for_message("SPX-123", Some(&long));
        assert!(name.len() <= WORKTREE_NAME_MAX_CHARS, "{name}");
        assert!(name.starts_with("spx-123-an-extraordinarily"), "{name}");
        assert!(!name.ends_with('-'));
    }
}

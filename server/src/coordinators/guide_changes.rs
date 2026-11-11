//! What changed in a coordinator's guide since the version it last read, for
//! `ghostex coordinator guide --changes`: the paragraphs that are new or different, and the ones
//! that are gone, each under the heading of the section it belongs to.
//!
//! CDXC:Coordinators 2026-11-11 WHY:
//! Every playbook change (an update, an override added for every coordinator, the reply format setting) used to have each coordinator re-read the whole guide, about 7,000 tokens, though it still held the earlier version in its context. `--changes` prints only what differs. The line gxserver adds under a coordinator's next message keeps its old wording (run `ghostex coordinator guide`), because phones from 11.2.1 on strip only the note texts they know (packages/gx-chat-core/src/session/appended_notes.rs) and an unknown one shows the user's message twice; the pointer skill, the playbook and the `status` line tell a coordinator to answer that note with `--changes`. Paragraphs are the unit because the playbook's rules and the user's overrides are paragraphs (one override is one paragraph, ghostex_cli/coordinator/playbook.rs). When printing the changes would take more than half the guide's length (a rewrite shipped in an update, a long override removed), the full text costs little more to read than the diff, so the caller prints the guide instead.
//! SEE-ALSO: server/src/coordinators/playbook.rs (`readCoordinatorGuide`, the stored texts), skills/ghostex-coordinator/SKILL.md and server/src/coordinators/playbook.md (where `--changes` is named), packages/gx-chat-core/src/session/appended_notes.rs (the note's fixed wire text).

/// One paragraph of a guide and the heading of the section it sits in.
#[derive(Debug, PartialEq, Eq)]
struct Paragraph<'a> {
    heading: &'a str,
    text: &'a str,
}

/// The guide's paragraphs in order. A heading (`#`, `##`, …, after up to three spaces as in
/// Markdown) is a paragraph of its own and the heading of the ones after it.
fn paragraphs(guide: &str) -> Vec<Paragraph<'_>> {
    let mut heading = "";
    let mut out = Vec::new();
    for text in guide
        .split("\n\n")
        .map(without_blank_edges)
        .filter(|text| !text.is_empty())
    {
        if is_heading(text) {
            heading = text;
        }
        out.push(Paragraph { heading, text });
    }
    out
}

/// Whether a paragraph is a Markdown ATX heading: one line starting with `#` after zero to three
/// spaces. Four spaces make it a code block.
fn is_heading(text: &str) -> bool {
    let unindented = text.trim_start_matches(' ');
    text.len() - unindented.len() <= 3 && unindented.starts_with('#') && !text.contains('\n')
}

/// `text` without the blank lines around it and the spaces after it. The indentation of its
/// first line stays: four spaces make a Markdown paragraph a code block, so a rule indented into
/// an example or out of one is a change.
fn without_blank_edges(text: &str) -> &str {
    let first_line = text[..text.len() - text.trim_start().len()]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    text[first_line..].trim_end()
}

/// The most comparisons the paragraph match may make: 4 MB of `u32` lengths. Past it the guide
/// is printed in full instead.
///
/// CDXC:Coordinators 2026-11-11 WHY:
/// The match keeps an `n × m` table of the paragraphs that differ between the two guides, and a guide can be big: an edited full playbook kept from before 11.0 may hold 100,000 characters and the overrides 20,000, which in short paragraphs is tens of thousands of them, so two such guides would ask for gigabytes before any fallback. Paragraphs both guides share at the start and the end are set aside first, so an override added, changed or removed costs a small table whatever the size of the guide, and only what is left in between is bounded.
const MAX_MATCH_CELLS: usize = 1_000_000;

/// For each of `old` and `new`, whether that paragraph is kept in a longest common subsequence
/// of the two (by text), or `None` when the paragraphs that differ need more than
/// [`MAX_MATCH_CELLS`] comparisons.
fn kept(old: &[Paragraph<'_>], new: &[Paragraph<'_>]) -> Option<(Vec<bool>, Vec<bool>)> {
    let (mut old_kept, mut new_kept) = (vec![false; old.len()], vec![false; new.len()]);
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(a, b)| a.text == b.text)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a.text == b.text)
        .count();
    old_kept[..prefix].fill(true);
    new_kept[..prefix].fill(true);
    old_kept[old.len() - suffix..].fill(true);
    new_kept[new.len() - suffix..].fill(true);
    let (old_middle, new_middle) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    let (n, m) = (old_middle.len(), new_middle.len());
    if (n + 1).checked_mul(m + 1)? > MAX_MATCH_CELLS {
        return None;
    }
    let mut lengths = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lengths[i][j] = if old_middle[i].text == new_middle[j].text {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old_middle[i].text == new_middle[j].text {
            old_kept[prefix + i] = true;
            new_kept[prefix + j] = true;
            i += 1;
            j += 1;
        } else if lengths[i + 1][j] >= lengths[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    Some((old_kept, new_kept))
}

/// What `--changes` prints for a coordinator that last read `old` while the guide is now `new`.
#[derive(Debug, PartialEq, Eq)]
pub enum GuideChanges {
    /// Nothing changed since that read.
    Unchanged,
    /// The changed paragraphs, ready to print.
    Changed(String),
    /// The changes would not print materially shorter than the guide: print it in full.
    MostlyNew,
}

/// The paragraphs of `new` that `old` did not have, and those of `old` that `new` dropped,
/// grouped under their section headings in the order they appear. [`GuideChanges::MostlyNew`]
/// when printing them would take more than half the length of `new` (removed text and the
/// headings included), or when the guides differ in too many paragraphs to match.
pub fn guide_changes(old: &str, new: &str) -> GuideChanges {
    let (old_paragraphs, new_paragraphs) = (paragraphs(old), paragraphs(new));
    let Some((old_kept, new_kept)) = kept(&old_paragraphs, &new_paragraphs) else {
        return GuideChanges::MostlyNew;
    };
    let added: Vec<&Paragraph<'_>> = new_paragraphs
        .iter()
        .zip(&new_kept)
        .filter(|(_, kept)| !**kept)
        .map(|(paragraph, _)| paragraph)
        .collect();
    let removed: Vec<&Paragraph<'_>> = old_paragraphs
        .iter()
        .zip(&old_kept)
        .filter(|(_, kept)| !**kept)
        .map(|(paragraph, _)| paragraph)
        .collect();
    if added.is_empty() && removed.is_empty() {
        return GuideChanges::Unchanged;
    }
    let mut out = String::from(
        "Your playbook changed since you last read it. Only the changes are below: everything else is as you read it, and the user's overrides still outrank the playbook. If your conversation was compacted or cleared since that read, run `ghostex coordinator guide` for the full text.\n",
    );
    let mut write_group = |title: &str, group: &[&Paragraph<'_>]| {
        let mut heading = None;
        for paragraph in group {
            if heading != Some(paragraph.heading) {
                heading = Some(paragraph.heading);
                let place = if paragraph.heading.is_empty() {
                    "At the top".to_string()
                } else {
                    format!(
                        "Under \"{}\"",
                        paragraph
                            .heading
                            .trim_start()
                            .trim_start_matches('#')
                            .trim()
                    )
                };
                out.push_str(&format!("\n{title} ({place}):\n"));
            }
            out.push_str(&format!("\n{}\n", paragraph.text));
        }
    };
    write_group("New or changed", &added);
    write_group("Removed", &removed);
    if out.len() * 2 > new.trim().len() {
        return GuideChanges::MostlyNew;
    }
    GuideChanges::Changed(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A guide shaped like the real one: a title, three sections of rules, a reply format.
    fn guide() -> String {
        let mut text = String::from("# Playbook\n\nIntro rule for every coordinator, said once.\n");
        for section in ["Briefing", "Asking", "Safety"] {
            text.push_str(&format!("\n## {section}\n"));
            for rule in 1..=15 {
                text.push_str(&format!(
                    "\n{section} rule {rule}: a sentence long enough to read like a real rule.\n"
                ));
            }
        }
        text.push_str("\nReply format: prose.\n");
        text
    }

    fn with_overrides(guide: &str, overrides: &[String]) -> String {
        format!(
            "{guide}\n## The user's overrides\n\nThey outrank the playbook.\n\n{}\n",
            overrides.join("\n\n")
        )
    }

    fn changed(old: &str, new: &str) -> String {
        match guide_changes(old, new) {
            GuideChanges::Changed(text) => text,
            other => panic!("expected changes, got {other:?}"),
        }
    }

    #[test]
    fn an_unchanged_guide_has_no_changes() {
        let guide = guide();
        assert_eq!(guide_changes(&guide, &guide), GuideChanges::Unchanged);
        // Whitespace around paragraphs is not a change.
        assert_eq!(
            guide_changes(
                &guide,
                &format!("\n{}\n\n", guide.replace("\n\n", "\n\n\n"))
            ),
            GuideChanges::Unchanged
        );
    }

    #[test]
    fn indenting_a_paragraph_into_a_code_block_or_out_of_one_is_a_change() {
        let indented = with_overrides(&guide(), &["First.".into(), "    Never push.".into()]);
        let plain = with_overrides(&guide(), &["First.".into(), "Never push.".into()]);
        let text = changed(&indented, &plain);
        assert!(text.contains("New or changed (Under \"The user's overrides\"):\n\nNever push.\n"));
        assert!(text.contains("Removed (Under \"The user's overrides\"):\n\n    Never push.\n"));
        let text = changed(&plain, &indented);
        assert!(
            text.contains("New or changed (Under \"The user's overrides\"):\n\n    Never push.\n")
        );
    }

    #[test]
    fn a_heading_indented_up_to_three_spaces_is_still_a_heading_and_four_make_it_code() {
        for indent in 0..=4 {
            let old = guide().replace(
                "\n## Safety\n",
                &format!("\n{}## Safety\n", " ".repeat(indent)),
            );
            let new = old.replace("Safety rule 3:", "Safety rule 3, reworded:");
            let text = changed(&old, &new);
            let section = if indent <= 3 { "Safety" } else { "Asking" };
            assert!(
                text.contains(&format!(
                    "New or changed (Under \"{section}\"):\n\nSafety rule 3, reworded:"
                )),
                "indent {indent}: {text}"
            );
        }
    }

    #[test]
    fn an_added_override_prints_only_the_override_under_its_heading() {
        let old = with_overrides(&guide(), &["Never merge.".to_string()]);
        let new = with_overrides(
            &guide(),
            &[
                "Never merge.".to_string(),
                "Ask before deleting branches.".to_string(),
            ],
        );
        let text = changed(&old, &new);
        assert!(text.contains(
            "New or changed (Under \"The user's overrides\"):\n\nAsk before deleting branches.\n"
        ));
        assert!(!text.contains("Never merge."));
        assert!(!text.contains("Briefing rule 1:"));
        assert!(!text.contains("Removed"));
    }

    #[test]
    fn a_changed_rule_prints_its_new_and_old_text() {
        let old = guide();
        let new = old.replace("Asking rule 7:", "Asking rule 7, reworded:");
        let text = changed(&old, &new);
        assert!(text.contains("New or changed (Under \"Asking\"):\n\nAsking rule 7, reworded:"));
        assert!(text.contains("Removed (Under \"Asking\"):\n\nAsking rule 7:"));
        assert!(!text.contains("Asking rule 6:"));
        assert!(!text.contains("Safety rule"));
    }

    #[test]
    fn a_removed_rule_lists_what_is_gone() {
        let old = guide();
        let new = old.replace(
            "\nSafety rule 3: a sentence long enough to read like a real rule.\n",
            "",
        );
        let text = changed(&old, &new);
        assert!(!text.contains("New or changed"));
        assert!(text.contains("Removed (Under \"Safety\"):\n\nSafety rule 3:"));
    }

    #[test]
    fn a_changed_paragraph_before_any_heading_says_it_is_at_the_top() {
        let old = format!(
            "Reply format: prose.\n\n{}",
            guide().replace("# Playbook\n\n", "")
        );
        let new = old.replacen("Reply format: prose.", "Reply format: marked sections.", 1);
        let text = changed(&old, &new);
        assert!(text.contains("New or changed (At the top):\n\nReply format: marked sections.\n"));
    }

    #[test]
    fn a_rewrite_is_printed_in_full() {
        let old = guide();
        let new = old.replace(
            "a sentence long enough",
            "a different sentence, long enough",
        );
        assert_eq!(guide_changes(&old, &new), GuideChanges::MostlyNew);
    }

    #[test]
    fn removing_a_long_override_prints_the_full_guide() {
        // Nothing is added, but listing what was removed would be longer than the guide itself.
        let long_rule = "Never push without asking. ".repeat(700);
        let old = with_overrides(&guide(), &[long_rule]);
        let new = guide();
        assert_eq!(guide_changes(&old, &new), GuideChanges::MostlyNew);
    }

    /// `n` override paragraphs that fill the 20,000-character override limit
    /// (COORDINATOR_PLAYBOOK_OVERRIDES_MAX_CHARS) with the shortest rules a user could write.
    fn short_overrides(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    #[test]
    fn overrides_at_their_limit_are_bounded_and_still_diff_an_added_rule() {
        let old_overrides = short_overrides("a", 3_000);
        assert!(old_overrides.join("\n\n").len() <= 20_000);
        let old = with_overrides(&guide(), &old_overrides);
        // Every override replaced: 3,000 × 3,000 paragraphs to match, past the bound.
        let replaced = with_overrides(&guide(), &short_overrides("b", 3_000));
        let started = std::time::Instant::now();
        assert_eq!(guide_changes(&old, &replaced), GuideChanges::MostlyNew);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        // One rule added at the end: the shared start is set aside, so only it is printed.
        let mut added = old_overrides.clone();
        added.push("Ask before deleting branches.".to_string());
        let text = changed(&old, &with_overrides(&guide(), &added));
        assert!(text.contains("\n\nAsk before deleting branches.\n"));
        assert!(!text.contains("\na1\n"));
    }

    #[test]
    fn a_legacy_playbook_at_its_limit_is_bounded() {
        // A full copy kept from before 11.0 may hold 100,000 characters
        // (COORDINATOR_PLAYBOOK_MAX_CHARS): here in 13,000 short paragraphs.
        let legacy: Vec<String> = (0..13_000).map(|i| format!("p{i}")).collect();
        let old = legacy.join("\n\n");
        assert!(old.len() <= 100_000);
        // The first and last paragraphs changed: all 13,000 lie between them.
        let mut changed_ends = legacy.clone();
        changed_ends[0] = "first changed".to_string();
        changed_ends[12_999] = "last changed".to_string();
        let started = std::time::Instant::now();
        assert_eq!(
            guide_changes(&old, &changed_ends.join("\n\n")),
            GuideChanges::MostlyNew
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        // One paragraph changed in the middle: matched without the full table.
        let mut changed_middle = legacy.clone();
        changed_middle[6_500] = "middle changed".to_string();
        let text = changed(&old, &changed_middle.join("\n\n"));
        assert!(text.contains("\n\nmiddle changed\n"));
        assert!(text.contains("Removed (At the top):\n\np6500\n"));
    }
}

//! What changed in a coordinator's guide since the version it last read, for
//! `ghostex coordinator guide --changes`: the paragraphs that are new or different, and the ones
//! that are gone, each under the heading of the section it belongs to.
//!
//! CDXC:Coordinators 2026-11-11 WHY:
//! Every playbook change (an update, an override added for every coordinator, the reply format setting) used to have each coordinator re-read the whole guide, about 7,000 tokens, though it still held the earlier version in its context. `--changes` prints only what differs. The line gxserver adds under a coordinator's next message keeps its old wording (run `ghostex coordinator guide`), because phones from 11.2.1 on strip only the note texts they know (packages/gx-chat-core/src/session/appended_notes.rs) and an unknown one shows the user's message twice; the pointer skill, the playbook and the `status` line tell a coordinator to answer that note with `--changes`. Paragraphs are the unit because the playbook's rules and the user's overrides are paragraphs (one override is one paragraph, ghostex_cli/coordinator/playbook.rs). When most of the guide changed (a rewrite shipped in an update) the full text is shorter to read than a diff, so the caller prints the guide instead.
//! SEE-ALSO: server/src/coordinators/playbook.rs (`readCoordinatorGuide`, the stored texts), skills/ghostex-coordinator/SKILL.md and server/src/coordinators/playbook.md (where `--changes` is named), packages/gx-chat-core/src/session/appended_notes.rs (the note's fixed wire text).

/// One paragraph of a guide and the heading of the section it sits in.
#[derive(Debug, PartialEq, Eq)]
struct Paragraph<'a> {
    heading: &'a str,
    text: &'a str,
}

/// The guide's paragraphs in order. A heading (`#`, `##`, …) is a paragraph of its own and the
/// heading of the ones after it.
fn paragraphs(guide: &str) -> Vec<Paragraph<'_>> {
    let mut heading = "";
    let mut out = Vec::new();
    for text in guide
        .split("\n\n")
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        if text.starts_with('#') && !text.contains('\n') {
            heading = text;
        }
        out.push(Paragraph { heading, text });
    }
    out
}

/// For each of `old` and `new`, whether that paragraph is kept in a longest common subsequence
/// of the two (by text).
fn kept(old: &[Paragraph<'_>], new: &[Paragraph<'_>]) -> (Vec<bool>, Vec<bool>) {
    let (n, m) = (old.len(), new.len());
    let mut lengths = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lengths[i][j] = if old[i].text == new[j].text {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }
    let (mut old_kept, mut new_kept) = (vec![false; n], vec![false; m]);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i].text == new[j].text {
            old_kept[i] = true;
            new_kept[j] = true;
            i += 1;
            j += 1;
        } else if lengths[i + 1][j] >= lengths[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    (old_kept, new_kept)
}

/// What `--changes` prints for a coordinator that last read `old` while the guide is now `new`.
#[derive(Debug, PartialEq, Eq)]
pub enum GuideChanges {
    /// Nothing changed since that read.
    Unchanged,
    /// The changed paragraphs, ready to print.
    Changed(String),
    /// Most of the guide changed: print it in full.
    MostlyNew,
}

/// The paragraphs of `new` that `old` did not have, and those of `old` that `new` dropped,
/// grouped under their section headings in the order they appear. [`GuideChanges::MostlyNew`]
/// when the new or changed text is more than half of `new`.
pub fn guide_changes(old: &str, new: &str) -> GuideChanges {
    let (old_paragraphs, new_paragraphs) = (paragraphs(old), paragraphs(new));
    let (old_kept, new_kept) = kept(&old_paragraphs, &new_paragraphs);
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
    let added_chars: usize = added.iter().map(|paragraph| paragraph.text.len()).sum();
    if added_chars * 2 > new.trim().len() {
        return GuideChanges::MostlyNew;
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
                        paragraph.heading.trim_start_matches('#').trim()
                    )
                };
                out.push_str(&format!("\n{title} ({place}):\n"));
            }
            out.push_str(&format!("\n{}\n", paragraph.text));
        }
    };
    write_group("New or changed", &added);
    write_group("Removed", &removed);
    GuideChanges::Changed(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUIDE: &str = "# Playbook\n\nIntro rule.\n\n## Briefing\n\nBrief rule one.\n\nBrief rule two.\n\n## Safety\n\nNever push.\n\nReply format: prose.\n";

    #[test]
    fn an_unchanged_guide_has_no_changes() {
        assert_eq!(guide_changes(GUIDE, GUIDE), GuideChanges::Unchanged);
        // Whitespace around paragraphs is not a change.
        assert_eq!(
            guide_changes(GUIDE, &format!("\n{}\n\n", GUIDE.replace("\n\n", "\n\n\n"))),
            GuideChanges::Unchanged
        );
    }

    #[test]
    fn an_added_override_prints_only_the_override_under_its_heading() {
        let old = format!(
            "{GUIDE}\n## The user's overrides\n\nThey outrank the playbook.\n\nNever merge.\n"
        );
        let new = format!("{old}\nAsk before deleting branches.\n");
        let GuideChanges::Changed(text) = guide_changes(&old, &new) else {
            panic!("expected changes");
        };
        assert!(text.contains(
            "New or changed (Under \"The user's overrides\"):\n\nAsk before deleting branches.\n"
        ));
        assert!(!text.contains("Never merge."));
        assert!(!text.contains("Brief rule one."));
        assert!(!text.contains("Removed"));
    }

    #[test]
    fn a_changed_rule_prints_its_new_and_old_text() {
        let new = GUIDE.replace("Brief rule two.", "Brief rule two, reworded.");
        let GuideChanges::Changed(text) = guide_changes(GUIDE, &new) else {
            panic!("expected changes");
        };
        assert!(
            text.contains("New or changed (Under \"Briefing\"):\n\nBrief rule two, reworded.\n")
        );
        assert!(text.contains("Removed (Under \"Briefing\"):\n\nBrief rule two.\n"));
        assert!(!text.contains("Brief rule one."));
        assert!(!text.contains("Never push."));
    }

    #[test]
    fn a_removed_section_lists_what_is_gone() {
        let new = GUIDE.replace("## Safety\n\nNever push.\n\n", "");
        let GuideChanges::Changed(text) = guide_changes(GUIDE, &new) else {
            panic!("expected changes");
        };
        assert!(!text.contains("New or changed"));
        assert!(text.contains("Removed (Under \"Safety\"):\n\n## Safety\n\nNever push.\n"));
    }

    #[test]
    fn a_changed_paragraph_before_any_heading_says_it_is_at_the_top() {
        let old = "Reply format: prose.\n\n## Overrides\n\nA rule the user wrote for every coordinator, long enough to outweigh the change.";
        let new = "Reply format: marked sections.\n\n## Overrides\n\nA rule the user wrote for every coordinator, long enough to outweigh the change.";
        let GuideChanges::Changed(text) = guide_changes(old, new) else {
            panic!("expected changes");
        };
        assert!(text.contains("New or changed (At the top):\n\nReply format: marked sections.\n"));
    }

    #[test]
    fn a_rewrite_is_printed_in_full() {
        let new = "# Playbook\n\nA wholly different text that replaces nearly every rule there was.\n\n## Safety\n\nNever push.\n";
        assert_eq!(guide_changes(GUIDE, new), GuideChanges::MostlyNew);
    }
}

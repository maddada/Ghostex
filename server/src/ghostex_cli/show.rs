use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::rpc::{
    request_gxserver_rpc, resolve_gxserver_server_target, CliError, CliResult,
};
use crate::ghostex_cli::visual_page_check;
use crate::visual_pages::{
    is_current_layout, page_too_large, prepare_page, PAGE_VERSION_META_TAG,
    PUBLISH_VISUAL_PAGE_ENDPOINT, VISUAL_PAGE_MAX_BYTES,
};

/// `ghostex show <file.html> [--title <text>] [--json]`: publishes a self-contained HTML page
/// through gxserver and prints the ```visual block that shows it as a card in the chat.
///
/// CDXC:SessionChat 2026-10-06 DECISION:
/// User: "both": the chat's native ```visual blocks (charts, stats, tables, text) and HTML pages ship together. A page covers what a block cannot express, such as a UI mockup or a small interactive tool; this verb publishes it and prints the page block the agent pastes into its reply.
pub fn show_command(args: &[String]) -> CliResult<()> {
    if matches!(args.first().map(String::as_str), Some("help")) {
        println!("{}", crate::ghostex_cli::usage::command_usage("show"));
        return Ok(());
    }
    let parsed = parse_args(args);
    // `--browser` and `--no-check` take no value, but the shared parser reads the next word as one
    // when the file comes after them.
    let switch_value = |name: &str| {
        parsed
            .flags
            .string_value(name)
            .filter(|value| !matches!(*value, "true" | "false"))
            .map(str::to_string)
    };
    let file = parsed
        .rest
        .first()
        .cloned()
        .or_else(|| switch_value("browser"))
        .or_else(|| switch_value("no-check"));
    let popup = !parsed.flags.contains("browser");
    let Some(file) = file.as_ref() else {
        return Err(CliError::Other(
            "show needs an HTML file: ghostex show <file.html> [--title <text>] [--json]"
                .to_string(),
        ));
    };
    let path = absolute_path(file);
    let source = read_source(&path)?;
    let base_dir = path.parent().unwrap_or(Path::new("/"));
    let prepared = prepare_page(&source, base_dir).map_err(CliError::Other)?;
    let title = parsed
        .flags
        .string_value("title")
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .or(prepared.title)
        .unwrap_or_else(|| file_stem(&path));
    // gxserver keeps the file's path so the desktop's page window can offer to refresh the page
    // when the file changes (visual_pages/source.rs).
    let mut params = json!({
        "title": title,
        "html": prepared.html,
        "sourcePath": path.to_string_lossy(),
    });
    if let Some(session_ref) = calling_session_ref() {
        params["sessionRef"] = Value::String(session_ref);
    }
    let target = resolve_gxserver_server_target(&parsed.flags, &params)?;
    let result = request_gxserver_rpc(
        &target,
        PUBLISH_VISUAL_PAGE_ENDPOINT,
        &params,
        &parsed.flags,
    )?;
    let text = |key: &str| result.get(key).and_then(Value::as_str).unwrap_or_default();
    let (id, page_path) = (text("id"), text("path"));
    if id.is_empty() || page_path.is_empty() {
        return Err(CliError::Other(
            "gxserver did not return the published page's address. Update Ghostex and try again."
                .to_string(),
        ));
    }
    let title = Some(text("title"))
        .filter(|title| !title.is_empty())
        .unwrap_or(title.as_str());
    let url = format!("{}{page_path}", target.base_url.trim_end_matches('/'));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let block = visual_page_block(
        title,
        &url,
        &file_name,
        popup,
        page_question_tags(&source).as_deref(),
    );
    let hint = old_layout_hint(&source);
    let visual_page_check::ShowFindings { phone, picks } = if parsed.flags.contains("no-check") {
        visual_page_check::ShowFindings::default()
    } else {
        visual_page_check::findings_for_show(&path)
    };
    if parsed.flags.truthy("json") {
        let hint = hint
            .as_deref()
            .map(|hint| format!(",\"hint\":{}", json_string(hint)))
            .unwrap_or_default();
        let phone = if phone.is_empty() {
            String::new()
        } else {
            format!(",\"phone\":{}", json!(phone))
        };
        let picks = if picks.is_empty() {
            String::new()
        } else {
            format!(",\"picks\":{}", json!(picks))
        };
        println!(
            "{{\"ok\":true,\"id\":{},\"title\":{},\"url\":{},\"block\":{}{hint}{phone}{picks}}}",
            json_string(id),
            json_string(title),
            json_string(&url),
            json_string(&block)
        );
    } else {
        let hint = hint.map(|hint| format!("{hint}\n\n")).unwrap_or_default();
        println!(
            "Published \"{title}\".\n{hint}Put this block in your reply where the page should appear (the chat shows it as a card that opens the page where the reader's Open chat pages in setting says, a floating window over the chat by default):\n\n{block}"
        );
        if !phone.is_empty() {
            println!("\n{}", phone_note(&phone, file));
        }
        if !picks.is_empty() {
            println!("\n{}", picks_note(&picks, file));
        }
    }
    Ok(())
}

/// What the publishing agent is told when its page did not fit a phone (visual_page_check.rs): fix
/// the page and publish it again before replying, since the reader only sees the card once the
/// block is in the reply.
///
/// CDXC:SessionChat 2026-10-28 DECISION:
/// User: fix pages for phones "for customers and all future inline htmls like we did previously for light dark etc". The kit fits any page, and the agent is told right where it reads `ghostex show`'s result when its page needed fitting, so its next page is written for a phone; `--no-check` skips the check.
fn phone_note(findings: &[String], file: &str) -> String {
    let mut note = format!(
        "Phone check ({}px): the page needs work to read well on a phone. Fix it, look with `ghostex visual check {file}`, then run ghostex show again and reply with the new block.",
        visual_page_check::PHONE_WIDTH
    );
    for finding in findings {
        note.push_str("\n- ");
        note.push_str(finding);
    }
    note
}

/// What the publishing agent is told when a question sits under mockups that have no Pick button
/// (visual_page_check.rs `pick_findings`). A warning: the page is published and still works.
fn picks_note(findings: &[String], file: &str) -> String {
    let mut note = format!(
        "Pick buttons: the reader can only choose below. Fix it, look with `ghostex visual check {file}`, then run ghostex show again and reply with the new block."
    );
    for finding in findings {
        note.push_str("\n- ");
        note.push_str(finding);
    }
    note
}

/// What the publishing agent is told when its page lacks the v2 marker (visual_pages/version.rs):
/// the page still publishes, with the old layout and a notice to the reader about it.
///
/// CDXC:SessionChat 2026-10-27 DECISION:
/// User: tell the sessions that still write old pages ("mention it again to his sessions"). The agent that publishes is the one that can change, so `ghostex show` says it right where the agent reads its result; it covers customers' agents too, whatever skill copy they loaded.
fn old_layout_hint(source: &str) -> Option<String> {
    (!is_current_layout(source)).then(|| {
        format!(
            "Note: this page has no {PAGE_VERSION_META_TAG}, so it was written for the old page layout and the reader sees a notice saying so. Load the current $ghostex-visuals skill again (Ghostex keeps it up to date) and start your next page from its skeleton."
        )
    })
}

/// The tags of the questions a page asks (`<gx-question tag="Q1">`), or `None` when it asks none:
/// the page holds the question kit (`<gx-questions>`) or sends answers of its own
/// (`ghostex.submit`).
///
/// CDXC:SessionChat 2026-10-14 WHY:
/// The chat counts a question page as unanswered until its answers come back, and links its tags, but the card only knows what the block says; the page's HTML is not in the transcript. So the block names the tags it asks (`"questions": [...]`), read here from the file being published.
fn page_question_tags(source: &str) -> Option<Vec<String>> {
    if !source.contains("<gx-questions") && !source.contains("ghostex.submit(") {
        return None;
    }
    let mut tags: Vec<String> = Vec::new();
    for (at, _) in source.match_indices("<gx-question") {
        let rest = &source[at + "<gx-question".len()..];
        if !rest.starts_with(|character: char| character.is_whitespace() || character == '>') {
            continue;
        }
        let head = &rest[..rest.find('>').unwrap_or(rest.len())];
        if let Some(tag) = attribute(head, "tag").filter(|tag| !tag.is_empty()) {
            if !tags.iter().any(|known| known.eq_ignore_ascii_case(&tag)) {
                tags.push(tag);
            }
        }
    }
    Some(tags)
}

/// An attribute's value in a start tag's text (`tag="Q1"` or `tag='Q1'`).
fn attribute(head: &str, name: &str) -> Option<String> {
    let mut search = head;
    while let Some(at) = search.find(name) {
        let before = search[..at].chars().next_back();
        let after = search[at + name.len()..].trim_start();
        search = &search[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value
            .chars()
            .next()
            .filter(|quote| *quote == '"' || *quote == '\'')?;
        let value = &value[1..];
        return Some(value[..value.find(quote)?].trim().to_string());
    }
    None
}

/// CDXC:SessionChat 2026-10-06 SEE-ALSO:
/// The page block `{"page":{"title","url","file","open","questions"}}` is a contract with the chat's visual parser in packages/gx-visual, which draws it as a card whose Open button opens the URL, and with skills/ghostex-visuals/SKILL.md, which tells agents to paste it exactly as printed.
/// CDXC:SessionChat 2026-10-09 DECISION:
/// User: the agent gives its HTML link "some mark so when we click to open it it's intended to be opened floating", so the agent can show a card for its page in the chat. The mark is `"open": "popup"`, printed by default; `--browser` leaves it out. Superseded in part on 2026-10-26, user: "My setting wins for every page": the desktop opens every card where Settings > Chat > Open chat pages in says and ignores the mark, which only steers Ghostex apps older than that (they open an unmarked card in the browser). `--browser` stays accepted so agents and scripts written for older apps keep working.
fn visual_page_block(
    title: &str,
    url: &str,
    file: &str,
    popup: bool,
    questions: Option<&[String]>,
) -> String {
    let open = if popup { ",\"open\":\"popup\"" } else { "" };
    let questions = questions
        .map(|tags| format!(",\"questions\":{}", json!(tags)))
        .unwrap_or_default();
    format!(
        "```visual\n{{\"page\":{{\"title\":{},\"url\":{},\"file\":{}{open}{questions}}}}}\n```",
        json_string(title),
        json_string(url),
        json_string(file)
    )
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

fn absolute_path(file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or(path)
}

fn read_source(path: &Path) -> CliResult<String> {
    let metadata = fs::metadata(path)
        .map_err(|_| CliError::Other(format!("Could not find {}.", path.display())))?;
    if !metadata.is_file() {
        return Err(CliError::Other(format!(
            "{} is not a file.",
            path.display()
        )));
    }
    // The page only grows once its head and images are added, so a source already over the
    // finished page's limit is refused before it is read.
    if metadata.len() > VISUAL_PAGE_MAX_BYTES as u64 {
        return Err(CliError::Other(page_too_large(
            usize::try_from(metadata.len()).unwrap_or(usize::MAX),
        )));
    }
    let bytes = fs::read(path)
        .map_err(|error| CliError::Other(format!("Could not read {}: {error}", path.display())))?;
    String::from_utf8(bytes).map_err(|_| {
        CliError::Other(
            "The page is not UTF-8 text. Save it as UTF-8 and run ghostex show again.".to_string(),
        )
    })
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().trim().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "Page".to_string())
}

/// The session this runs in, from the environment every Ghostex pane exports; kept with the page
/// so it can be traced back to the conversation that made it, and so the reader's answers
/// (`ghostex.submit`) reach that session.
fn calling_session_ref() -> Option<String> {
    ["GHOSTEX_GLOBAL_SESSION_REF", "GHOSTEX_NATIVE_SESSION_ID"]
        .iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_without_the_v2_marker_gets_the_old_layout_hint() {
        let hint = old_layout_hint("<html><head><title>t</title></head><body></body></html>")
            .expect("a hint");
        assert!(hint.contains(PAGE_VERSION_META_TAG));
        assert!(hint.contains("$ghostex-visuals"));
    }

    #[test]
    fn a_v2_page_gets_no_hint() {
        assert!(old_layout_hint(
            "<html><head><meta name=\"ghostex-page\" content=\"v2\"></head><body><gx-page></gx-page></body></html>"
        )
        .is_none());
    }
}

//! A Ghostex page opened from its file (the Files view, a file link in chat, Open in browser):
//! the HTML file is served as the page `ghostex show` would publish from it, with the page kit,
//! instead of as plain HTML.
//!
//! CDXC:SessionChat 2026-11-06 DECISION:
//! User, on a page an agent wrote and a coordinator linked by its file path that opened in Files as plain unstyled HTML: "why is this html file looking like that without styles, is this normal? or we need to fix this?" A file that declares itself a Ghostex page (`<meta name="ghostex-page" content="v2">`) opens with the page kit wherever Ghostex opens it, the same as a published page; any other HTML file stays as it is. The page answers to the session that published that same file with `ghostex show`, when one did; a file nobody published opens read-only with a note, since no agent is waiting for its answers.
//! SEE-ALSO: server/src/file_links/serve.rs (the browser link redirects here), server/src/server/project_docs_http.rs (`openPage`, the Files view's ask), apps/desktop/src/app/native_docs/published_page.rs (the Files view shows it), serve.rs (the read-only note).

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::{
    prepare::prepare_page,
    store::{
        VISUAL_PAGE_MAX_BYTES, read_record, replace_page_html, source_modified_ms,
        visual_pages_dir, write_page,
    },
    version::is_current_layout,
};

/// The record mark of a page made here rather than by `ghostex show`.
pub(super) const OPENED_FROM_FILE_KEY: &str = "openedFromFile";
/// The record mark of a page made for a cover's link: the cover's page id.
pub(super) const LINKED_FROM_KEY: &str = "linkedFrom";

/// The page served for a Ghostex page file.
pub(crate) struct FilePage {
    pub(crate) id: String,
    pub(crate) title: String,
    /// Whether a session published this file, so its answers reach an agent.
    pub(crate) answerable: bool,
}

/// The page for `file` when the file declares itself a Ghostex page, `Ok(None)` for any other
/// file. Reuses the page `ghostex show` published from the same file (the newest one with a
/// session first), prepared again when the file changed since; otherwise publishes one with no
/// session, which serve.rs opens read-only.
pub(crate) fn page_for_file(
    gxserver_state_dir: &Path,
    file: &Path,
) -> Result<Option<FilePage>, String> {
    let Some((file, source, modified)) = read_page_file(file) else {
        return Ok(None);
    };
    let prepare = || prepare_page(&source, file.parent().unwrap_or(Path::new("/")));
    // A page made for a cover's link answers to that cover's agent only through that link: the
    // file opened on its own (Files, a file link) never takes it over.
    if let Some((id, record)) = published_from(gxserver_state_dir, &file, |record| {
        linked_from(record).is_none()
    }) {
        let published = record.get("sourceModifiedMs").and_then(Value::as_u64);
        if published != modified {
            let prepared = prepare()?;
            replace_page_html(gxserver_state_dir, &id, &prepared.html, modified)
                .map_err(|_| "Could not save the page.".to_string())?;
        }
        return Ok(Some(FilePage {
            id,
            title: record
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Live page")
                .to_string(),
            answerable: has_session(&record),
        }));
    }
    let prepared = prepare()?;
    let title = prepared.title.clone().unwrap_or_else(|| {
        file.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Live page".to_string())
    });
    let id = write_page(
        gxserver_state_dir,
        &title,
        &prepared.html,
        None,
        Some(&file),
    )
    .map_err(|error| format!("Could not save the page: {error}"))?;
    super::store::mark_opened_from_file(gxserver_state_dir, &id);
    Ok(Some(FilePage {
        id,
        title,
        answerable: false,
    }))
}

/// The canonical path, the text and the modification time of a Ghostex page file (an `.html` file
/// no larger than a page may be, holding the v2 marker), or `None` for any other file. The time is
/// read before the text, so an edit made while it is prepared shows as a change next time.
fn read_page_file(file: &Path) -> Option<(PathBuf, String, Option<u64>)> {
    let file = fs::canonicalize(file).ok()?;
    let is_html = file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm"));
    let metadata = fs::metadata(&file)
        .ok()
        .filter(|metadata| metadata.is_file())?;
    if !is_html || metadata.len() > VISUAL_PAGE_MAX_BYTES as u64 {
        return None;
    }
    let modified = source_modified_ms(&file);
    let source = fs::read_to_string(&file).ok()?;
    is_current_layout(&source).then_some((file, source, modified))
}

/// The page a link in another page opens for `file` (linked.rs): the one [`page_for_file`] gives
/// a Ghostex page file, else the page `ghostex show` published from the file whatever its layout,
/// as published. `Ok(None)` for any other file.
pub(super) fn page_for_linked_file(
    gxserver_state_dir: &Path,
    file: &Path,
) -> Result<Option<FilePage>, String> {
    if let Some(page) = page_for_file(gxserver_state_dir, file)? {
        return Ok(Some(page));
    }
    let Ok(file) = fs::canonicalize(file) else {
        return Ok(None);
    };
    Ok(
        published_from(gxserver_state_dir, &file, shown_by_a_session)
            .filter(|(_, record)| has_session(record))
            .map(|(id, record)| FilePage {
                id,
                title: record
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("Live page")
                    .to_string(),
                answerable: true,
            }),
    )
}

/// The page a published cover's link opens for `file`, a file inside the cover's own folder
/// (linked.rs checks that), when the cover answers to a session: the page a session published from
/// the file with `ghostex show` when there is one (a real publisher wins), else the cover's own page
/// for the file, made on the first click, which answers to the cover's session and, when it asks
/// nothing, waits to be marked as read. `Ok(None)` when the cover answers to no session (the caller
/// falls back to [`page_for_linked_file`]) or the file is no page a link opens.
///
/// CDXC:SessionChat 2026-11-21 DECISION:
/// User (Q4352, "A. Same publisher as the cover"), after a cover's section pages opened "Read-only:
/// no agent sent this page", so they could not be answered, marked as read or listed in Needs you:
/// a section opened through a published cover's link belongs to the cover's publisher. Only files
/// inside the cover's folder (symlinks resolved), the same files the link route already opened; a
/// link out of the folder still opens only a page `ghostex show` published, and is never given to
/// the cover's agent. Each such page records its cover (`linkedFrom`), so two covers by two agents
/// linking one file each reach their own agent, and the file opened from Files stays read-only.
pub(super) fn page_for_cover_link(
    gxserver_state_dir: &Path,
    file: &Path,
    cover_id: &str,
) -> Result<Option<CoverLinkPage>, String> {
    let Some(cover) = read_record(gxserver_state_dir, cover_id).filter(has_session) else {
        return Ok(None);
    };
    let Some(session_ref) = cover
        .get("sessionRef")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    // A section's own links belong to the cover it was opened from, so sections that link each
    // other (next, back, themselves) open one page per file, never a new one on every hop.
    let cover_id = linked_from(&cover).unwrap_or(cover_id).to_string();
    let cover_id = cover_id.as_str();
    // A retired cover (`ghostex page retire`, or a newer `ghostex show` of its file) and its
    // sections answer for nothing more: their links open read-only (the caller's fallback).
    let retired = |record: &Value| record.get("retiredAt").is_some();
    if retired(&cover)
        || read_record(gxserver_state_dir, cover_id).is_none_or(|root| retired(&root))
    {
        return Ok(None);
    }
    let Ok(canonical) = fs::canonicalize(file) else {
        return Ok(None);
    };
    if let Some((id, record)) = published_from(gxserver_state_dir, &canonical, |record| {
        shown_by_a_session(record) && has_session(record)
    }) {
        if let Some((_, source, modified)) = read_page_file(&canonical) {
            if record.get("sourceModifiedMs").and_then(Value::as_u64) != modified {
                let prepared = prepare_page(&source, canonical.parent().unwrap_or(Path::new("/")))?;
                replace_page_html(gxserver_state_dir, &id, &prepared.html, modified)
                    .map_err(|_| "Could not save the page.".to_string())?;
            }
        }
        return Ok(Some(CoverLinkPage { id, made_for: None }));
    }
    let Some((file, source, modified)) = read_page_file(&canonical) else {
        return Ok(None);
    };
    let prepare = || prepare_page(&source, file.parent().unwrap_or(Path::new("/")));
    let awaits_seen = asks_nothing(&source);
    // Held from the lookup to the write, so first clicks that arrive together open one page.
    static MAKING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _making = MAKING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((id, record)) = published_from(gxserver_state_dir, &file, |record| {
        linked_from(record) == Some(cover_id)
    }) {
        if record.get("sourceModifiedMs").and_then(Value::as_u64) != modified {
            let prepared = prepare()?;
            replace_page_html(gxserver_state_dir, &id, &prepared.html, modified)
                .map_err(|_| "Could not save the page.".to_string())?;
        }
        return Ok(Some(CoverLinkPage {
            id,
            made_for: Some(LinkedSession {
                session_ref,
                awaits_seen,
            }),
        }));
    }
    // A backstop against a folder of generated sections: past the cap a link opens the file
    // read-only, as a file nobody published (the caller's fallback).
    if pages_linked_from(gxserver_state_dir, cover_id) >= LINKED_PAGES_PER_COVER {
        return Ok(None);
    }
    let prepared = prepare()?;
    let title = prepared.title.clone().unwrap_or_else(|| {
        file.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Live page".to_string())
    });
    let mut marks = vec![(LINKED_FROM_KEY, serde_json::json!(cover_id))];
    if awaits_seen {
        marks.push(("awaitsSeen", serde_json::json!(true)));
    }
    let id = super::store::write_page_marked(
        gxserver_state_dir,
        &title,
        &prepared.html,
        Some(&session_ref),
        Some(&file),
        &marks,
    )
    .map_err(|error| format!("Could not save the page: {error}"))?;
    Ok(Some(CoverLinkPage {
        id,
        made_for: Some(LinkedSession {
            session_ref,
            awaits_seen,
        }),
    }))
}

/// How many pages one cover's links may make (`page_for_cover_link`); a link to another file past
/// that opens it read-only.
///
/// CDXC:SessionChat 2026-11-21 DECISION:
/// After the security review of linked sections: "cap linked-section pages per cover at 50
/// (beyond that, links open read-only)". Each page is one row in the cover's Needs you and one
/// page on disk, so a folder of generated sections, or anyone holding a cover's address, cannot add
/// them without end.
const LINKED_PAGES_PER_COVER: usize = 50;

/// How many pages links made for the cover `cover_id` (`linkedFrom`), retired ones included.
fn pages_linked_from(gxserver_state_dir: &Path, cover_id: &str) -> usize {
    records_linked_from(gxserver_state_dir, cover_id).len()
}

/// The pages links made for the cover `cover_id` that are not retired yet: retire.rs retires them
/// with their cover.
pub(super) fn sections_of(gxserver_state_dir: &Path, cover_id: &str) -> Vec<String> {
    records_linked_from(gxserver_state_dir, cover_id)
        .into_iter()
        .filter(|(_, record)| record.get("retiredAt").is_none())
        .map(|(id, _)| id)
        .collect()
}

/// The pages links made for the cover `cover_id`, with their records.
fn records_linked_from(gxserver_state_dir: &Path, cover_id: &str) -> Vec<(String, Value)> {
    let Ok(entries) = fs::read_dir(visual_pages_dir(gxserver_state_dir)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json")?.to_string();
            read_record(gxserver_state_dir, &id).map(|record| (id, record))
        })
        .filter(|(_, record)| linked_from(record) == Some(cover_id))
        .collect()
}

/// The cover a page opened through a cover's link belongs to: the page itself when it is no
/// section, else the cover its record names. linked.rs keeps a section's links inside that cover's
/// folder, so a section in a subfolder can link back up to a sibling (`../a.html`).
pub(super) fn cover_of(gxserver_state_dir: &Path, page_id: &str) -> String {
    read_record(gxserver_state_dir, page_id)
        .as_ref()
        .and_then(linked_from)
        .unwrap_or(page_id)
        .to_string()
}

/// What [`page_for_cover_link`] opens.
pub(super) struct CoverLinkPage {
    pub(super) id: String,
    /// Set when the page is the cover's own page for the file, which answers to the cover's
    /// session: that session lists it (session_chat_open_questions.rs `note_linked_page`).
    pub(super) made_for: Option<LinkedSession>,
}

/// The session a cover's page for a linked file answers to.
pub(super) struct LinkedSession {
    pub(super) session_ref: String,
    /// It asks nothing, so it waits to be marked as read.
    pub(super) awaits_seen: bool,
}

/// What a session lists of a page made for its cover's link (session_chat_open_questions.rs).
pub(crate) struct LinkedPageSummary {
    pub(crate) title: String,
    /// The cover whose link made it.
    pub(crate) cover: String,
    /// It asks nothing and waits to be marked as read.
    pub(crate) awaits_seen: bool,
    /// When it was made (epoch ms).
    pub(crate) created_ms: Option<i64>,
    /// Its answers (or its Mark as read) came back.
    pub(crate) answered: bool,
}

/// The page `id` when a cover's link made it to answer to the session `project_id`/`session_id`,
/// else `None`.
pub(crate) fn linked_page_summary(
    gxserver_state_dir: &Path,
    id: &str,
    project_id: &str,
    session_id: &str,
) -> Option<LinkedPageSummary> {
    if !super::store::is_page_id(id) {
        return None;
    }
    let record = read_record(gxserver_state_dir, id)?;
    let cover = linked_from(&record)?.to_string();
    let (project, session) = crate::agent_hooks::probing::parse_global_session_ref(
        record.get("sessionRef").and_then(Value::as_str)?,
    );
    if project.as_deref() != Some(project_id)
        || session.as_deref() != Some(session_id)
        || record.get("retiredAt").is_some()
    {
        return None;
    }
    Some(LinkedPageSummary {
        title: record
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Live page")
            .to_string(),
        cover,
        awaits_seen: record.get("awaitsSeen") == Some(&Value::Bool(true)),
        created_ms: record
            .get("createdAt")
            .and_then(Value::as_str)
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .map(|at| at.timestamp_millis()),
        answered: record.get("answeredAt").is_some_and(|at| !at.is_null()),
    })
}

/// Whether a page's file asks nothing: no question kit (`<gx-questions>`) and no answers of its own
/// (`ghostex.submit`), as `ghostex show` decides `awaitsSeen` (show.rs `page_question_tags`).
fn asks_nothing(source: &str) -> bool {
    !source.contains("<gx-questions") && !source.contains("ghostex.submit(")
}

/// The cover a page was made for by a link (`page_for_cover_link`), if it was.
pub(crate) fn linked_from(record: &Value) -> Option<&str> {
    record
        .get(LINKED_FROM_KEY)
        .and_then(Value::as_str)
        .filter(|cover| !cover.is_empty())
}

/// Whether `ghostex show` published the page: not made by opening its file or for a cover's link.
fn shown_by_a_session(record: &Value) -> bool {
    record.get(OPENED_FROM_FILE_KEY) != Some(&Value::Bool(true)) && linked_from(record).is_none()
}

fn has_session(record: &Value) -> bool {
    record
        .get("sessionRef")
        .and_then(Value::as_str)
        .is_some_and(|session| !session.trim().is_empty())
}

/// The page `ghostex show` published from `file` (the newest with a session, else the newest), as
/// it was published; a page made by opening the file (`page_for_file`) or for a cover's link
/// (`page_for_cover_link`) does not count. linked.rs opens a link outside the linking page's folder
/// only to such a page.
pub(super) fn page_show_published(gxserver_state_dir: &Path, file: &Path) -> Option<FilePage> {
    let file = fs::canonicalize(file).ok()?;
    let (id, record) = published_from(gxserver_state_dir, &file, shown_by_a_session)?;
    Some(FilePage {
        title: record
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Live page")
            .to_string(),
        answerable: has_session(&record),
        id,
    })
}

/// The page published from `file` among the records `keep` takes: the newest with a session, else
/// the newest made here.
fn published_from(
    gxserver_state_dir: &Path,
    file: &Path,
    keep: impl Fn(&Value) -> bool,
) -> Option<(String, Value)> {
    let file = file.to_string_lossy();
    let entries = fs::read_dir(visual_pages_dir(gxserver_state_dir)).ok()?;
    let mut best: Option<(bool, String, String, Value)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        let Some(record) = read_record(gxserver_state_dir, id) else {
            continue;
        };
        let same_file = record
            .get("sourcePath")
            .and_then(Value::as_str)
            .is_some_and(|source| {
                source == file
                    || fs::canonicalize(source).is_ok_and(|source| source.to_string_lossy() == file)
            });
        if !same_file || !keep(&record) {
            continue;
        }
        let rank = (
            has_session(&record),
            record
                .get("createdAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        );
        if best
            .as_ref()
            .is_none_or(|(session, created, _, _)| rank > (*session, created.clone()))
        {
            best = Some((rank.0, rank.1, id.to_string(), record));
        }
    }
    best.map(|(_, _, id, record)| (id, record))
}

/// Whether the page was made by opening its file, with no session to answer to.
pub(super) fn opened_from_file(record: &Value) -> bool {
    record.get(OPENED_FROM_FILE_KEY) == Some(&Value::Bool(true)) && !has_session(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><head><meta name=\"ghostex-page\" content=\"v2\"><title>Signing</title></head><body><gx-card>x</gx-card></body></html>";

    fn write(dir: &Path, name: &str, html: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, html).unwrap();
        path
    }

    #[test]
    fn plain_html_is_not_a_page() {
        let state = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        let plain = write(
            files.path(),
            "plain.html",
            "<html><body><p>hi</p></body></html>",
        );
        assert!(page_for_file(state.path(), &plain).unwrap().is_none());
        let text = write(files.path(), "page.txt", PAGE);
        assert!(page_for_file(state.path(), &text).unwrap().is_none());
    }

    #[test]
    fn an_unpublished_page_file_opens_once_without_a_session() {
        let state = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        let file = write(files.path(), "page.html", PAGE);
        let first = page_for_file(state.path(), &file).unwrap().unwrap();
        assert!(!first.answerable);
        assert_eq!(first.title, "Signing");
        let record = read_record(state.path(), &first.id).unwrap();
        assert!(opened_from_file(&record));
        // Opening it again serves the same page, not a new one.
        let again = page_for_file(state.path(), &file).unwrap().unwrap();
        assert_eq!(again.id, first.id);
    }

    #[test]
    fn a_page_a_session_published_answers_to_that_session_and_follows_the_file() {
        let state = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        let file = fs::canonicalize(write(files.path(), "page.html", PAGE)).unwrap();
        let unowned = page_for_file(state.path(), &file).unwrap().unwrap();
        let published = write_page(
            state.path(),
            "Signing",
            PAGE,
            Some("S90:P1:G1"),
            Some(&file),
        )
        .unwrap();
        let page = page_for_file(state.path(), &file).unwrap().unwrap();
        assert_eq!(page.id, published);
        assert_ne!(page.id, unowned.id);
        assert!(page.answerable);
        assert!(!opened_from_file(
            &read_record(state.path(), &published).unwrap()
        ));
        // The file changed: the same page is prepared again.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&file, PAGE.replace("<gx-card>x", "<gx-card>changed")).unwrap();
        let page = page_for_file(state.path(), &file).unwrap().unwrap();
        assert_eq!(page.id, published);
        let served = super::super::store::read_page(state.path(), &published).unwrap();
        assert!(String::from_utf8(served).unwrap().contains("changed"));
    }
}

//! A Ghostex page opened from its file (the Files view, a file link in chat, Open in browser):
//! the HTML file is served as the page `ghostex show` would publish from it, with the page kit,
//! instead of as plain HTML.
//!
//! CDXC:SessionChat 2026-11-06 DECISION:
//! User, on a page an agent wrote and a coordinator linked by its file path that opened in Files as plain unstyled HTML: "why is this html file looking like that without styles, is this normal? or we need to fix this?" A file that declares itself a Ghostex page (`<meta name="ghostex-page" content="v2">`) opens with the page kit wherever Ghostex opens it, the same as a published page; any other HTML file stays as it is. The page answers to the session that published that same file with `ghostex show`, when one did; a file nobody published opens read-only with a note, since no agent is waiting for its answers.
//! SEE-ALSO: server/src/file_links/serve.rs (the browser link redirects here), server/src/server/project_docs_http.rs (`openPage`, the Files view's ask), apps/desktop/src/app/native_docs/published_page.rs (the Files view shows it), serve.rs (the read-only note).

use std::{fs, path::Path};

use serde_json::Value;

use super::{
    prepare::prepare_page,
    store::{
        read_record, replace_page_html, source_modified_ms, visual_pages_dir, write_page,
        VISUAL_PAGE_MAX_BYTES,
    },
    version::is_current_layout,
};

/// The record mark of a page made here rather than by `ghostex show`.
pub(super) const OPENED_FROM_FILE_KEY: &str = "openedFromFile";

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
    let Ok(file) = fs::canonicalize(file) else {
        return Ok(None);
    };
    let is_html = file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm"));
    let Some(metadata) = fs::metadata(&file)
        .ok()
        .filter(|metadata| metadata.is_file())
    else {
        return Ok(None);
    };
    if !is_html || metadata.len() > VISUAL_PAGE_MAX_BYTES as u64 {
        return Ok(None);
    }
    // Read before the file is: an edit made while it is prepared shows as a change next time.
    let modified = source_modified_ms(&file);
    let Ok(source) = fs::read_to_string(&file) else {
        return Ok(None);
    };
    if !is_current_layout(&source) {
        return Ok(None);
    }
    let prepare = || prepare_page(&source, file.parent().unwrap_or(Path::new("/")));
    if let Some((id, record)) = published_from(gxserver_state_dir, &file) {
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
                .unwrap_or("Ghostex page")
                .to_string(),
            answerable: has_session(&record),
        }));
    }
    let prepared = prepare()?;
    let title = prepared.title.clone().unwrap_or_else(|| {
        file.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Ghostex page".to_string())
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

fn has_session(record: &Value) -> bool {
    record
        .get("sessionRef")
        .and_then(Value::as_str)
        .is_some_and(|session| !session.trim().is_empty())
}

/// The page published from `file`: the newest with a session, else the newest made here.
fn published_from(gxserver_state_dir: &Path, file: &Path) -> Option<(String, Value)> {
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
        if !same_file {
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

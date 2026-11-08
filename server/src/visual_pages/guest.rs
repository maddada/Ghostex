//! Ghostex pages and HTML files for a Ghostex Collab guest, whose browser cannot reach this
//! computer's `/visual/<id>` routes: the page's HTML goes through the share's tunnel instead
//! (`/api/collabGuestPage`, server/src/collab_shim/http.rs), and the guest's page sends its
//! answers back the same way (`/api/collabGuestPageAnswers`), as a guest message.
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User: "Pls make sure htmls and images can be viewed by the guest when clicking on them and even inline htmls can be answered by them." A guest opens a page the shared chat published and an HTML file the shared chat links to (`session_files_links::session_shows_path`), and nothing else; a page answers to the shared chat only, so a page another chat published opens read-only. Answers reach the chat the way the guest's messages do: held for the owner's approval in Approve mode.
//! SEE-ALSO: apps/gpui-web/www/src/collab/page.js (the guest's page viewer).

use std::{fs, path::Path};

use serde_json::Value;

use super::{
    file_page::page_for_file,
    prepare::inline_local_images,
    serve::{with_page_state, with_read_only},
    store::{read_page, read_record, VISUAL_PAGE_MAX_BYTES},
};

/// The note on a page a guest may read but not answer.
const NOT_THIS_CHAT_NOTE: &str =
    "Read-only: this page doesn't answer to the shared chat, so answers sent from here go nowhere.";

/// A page for the guest's viewer.
pub(crate) struct GuestPage {
    pub(crate) html: String,
    pub(crate) title: String,
    /// The Ghostex page it is, when it is one: its answers go to `/api/collabGuestPageAnswers`.
    pub(crate) page_id: Option<String>,
    /// Whether its answers reach the shared chat.
    pub(crate) answerable: bool,
}

fn published_by(record: &Value, project_id: &str, session_id: &str) -> bool {
    let (project, session) = record
        .get("sessionRef")
        .and_then(Value::as_str)
        .map(crate::agent_hooks::probing::parse_global_session_ref)
        .unwrap_or((None, None));
    project.as_deref() == Some(project_id) && session.as_deref() == Some(session_id)
}

/// The Ghostex page `id` when the shared chat published it.
pub(crate) fn guest_page(
    gxserver_state_dir: &Path,
    id: &str,
    project_id: &str,
    session_id: &str,
) -> Option<GuestPage> {
    let record = read_record(gxserver_state_dir, id)?;
    if !published_by(&record, project_id, session_id) {
        return None;
    }
    page_with_state(gxserver_state_dir, id, record, true)
}

/// The HTML file at `file` (one the shared chat links to: the caller checks): as its Ghostex page
/// when the file is one, else as the file itself with its local pictures inlined.
pub(crate) fn guest_file_page(
    gxserver_state_dir: &Path,
    file: &Path,
    project_id: &str,
    session_id: &str,
) -> Result<Option<GuestPage>, String> {
    let is_html = file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm"));
    if !is_html {
        return Ok(None);
    }
    if let Some(page) = page_for_file(gxserver_state_dir, file)? {
        let Some(record) = read_record(gxserver_state_dir, &page.id) else {
            return Ok(None);
        };
        let answerable = published_by(&record, project_id, session_id);
        return Ok(page_with_state(
            gxserver_state_dir,
            &page.id,
            record,
            answerable,
        ));
    }
    let Some(metadata) = fs::metadata(file)
        .ok()
        .filter(|metadata| metadata.is_file())
    else {
        return Ok(None);
    };
    if metadata.len() > VISUAL_PAGE_MAX_BYTES as u64 {
        return Err(super::page_too_large(metadata.len() as usize));
    }
    let source = fs::read(file).map_err(|error| format!("Could not read the file: {error}"))?;
    let source = String::from_utf8_lossy(&source);
    let html = inline_local_images(&source, file.parent().unwrap_or(Path::new("/")))?;
    let title = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Page".to_string());
    Ok(Some(GuestPage {
        html,
        title,
        page_id: None,
        answerable: false,
    }))
}

fn page_with_state(
    gxserver_state_dir: &Path,
    id: &str,
    record: Value,
    answerable: bool,
) -> Option<GuestPage> {
    let title = record
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Ghostex page")
        .to_string();
    let page = with_page_state(read_page(gxserver_state_dir, id)?, Some(record), false);
    let page = if answerable {
        page
    } else {
        with_read_only(page, NOT_THIS_CHAT_NOTE)
    };
    Some(GuestPage {
        html: String::from_utf8(page).ok()?,
        title,
        page_id: Some(id.to_string()),
        answerable,
    })
}

/// Why a guest's answers to page `id` cannot go to the shared chat, or the message that carries
/// them when they can.
pub(crate) fn guest_page_answers_message(
    gxserver_state_dir: &Path,
    id: &str,
    project_id: &str,
    session_id: &str,
    answers: &Value,
) -> Result<String, &'static str> {
    let record = read_record(gxserver_state_dir, id).ok_or("No such page.")?;
    if !published_by(&record, project_id, session_id) {
        return Err("This page doesn't answer to the shared chat.");
    }
    if record.get("answeredAt").and_then(Value::as_str).is_some() {
        return Err("These answers were already sent.");
    }
    let title = record
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Ghostex page");
    Ok(ghostex_gx_visual::questions::page_answers_message(
        id, title, answers,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn publish(dir: &Path, id: &str, session_ref: &str) {
        let pages = super::super::store::visual_pages_dir(dir);
        fs::create_dir_all(&pages).unwrap();
        fs::write(
            pages.join(format!("{id}.html")),
            "<!doctype html><html><head><title>Pick</title></head><body>x</body></html>",
        )
        .unwrap();
        fs::write(
            pages.join(format!("{id}.json")),
            json!({"title": "Pick", "sessionRef": session_ref}).to_string(),
        )
        .unwrap();
    }

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_guest_reads_only_the_shared_chats_pages() {
        let state = tempfile::tempdir().unwrap();
        publish(state.path(), ID, "S1:P1:G1");
        let page = guest_page(state.path(), ID, "P1", "G1").unwrap();
        assert!(page.answerable);
        assert_eq!(page.title, "Pick");
        assert!(guest_page(state.path(), ID, "P1", "G2").is_none());
    }

    #[test]
    fn answers_go_only_to_the_chat_that_published_the_page() {
        let state = tempfile::tempdir().unwrap();
        publish(state.path(), ID, "S1:P1:G1");
        let answers = json!({"Q1": "Tabs"});
        let message = guest_page_answers_message(state.path(), ID, "P1", "G1", &answers).unwrap();
        assert!(message.contains("ghostex-page-answers"));
        assert!(message.contains("Tabs"));
        assert!(guest_page_answers_message(state.path(), ID, "P2", "G1", &answers).is_err());
    }

    #[test]
    fn a_plain_html_file_keeps_its_own_markup() {
        let state = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("report.html");
        fs::write(&file, "<html><body><h1>Report</h1></body></html>").unwrap();
        let page = guest_file_page(state.path(), &file, "P1", "G1")
            .unwrap()
            .unwrap();
        assert!(page.html.contains("<h1>Report</h1>"));
        assert!(page.page_id.is_none());
        assert!(
            guest_file_page(state.path(), &folder.path().join("notes.txt"), "P1", "G1")
                .unwrap()
                .is_none()
        );
    }
}

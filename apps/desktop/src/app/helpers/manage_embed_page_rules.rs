//! What a file shown in the Files embed page (`apps/desktop/views/manage/embed.tsx`) may do: which
//! of the page's Files requests the app answers, which files the Files resource origin hands to the
//! rendered HTML, and where that HTML may connect or navigate. Plain Rust, so the rules are tested
//! without CEF; `cef/shell/request_handling.rs` and `app/workspace_events.rs` apply them.
//!
//! CDXC:Docs 2026-11-14 DECISION:
//! User: an HTML page shown in Files may read plain files in its own folder and below, read only,
//! never dot-files or dot-folders, with network access blocked for the page so nothing it reads
//! can leave. The page runs in its own sealed origin (`html-viewer.tsx`); these rules are the app's
//! side of it, so they hold for every frame the page makes, whatever its script does.

use crate::app::helpers::MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT;
use crate::app::helpers::manage_docs_chat_files::manage_chat_file_address;

/// The Files resource origin (`PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL` in cef/sidebar_bridge_manifest.rs).
pub(crate) const MANAGE_EMBED_RESOURCE_BASE_URL: &str = "https://ghostex-docs.invalid/";

/// Public library CDNs a rendered HTML file may still load scripts, styles, fonts and pictures
/// from, with GET only. None of them shows a page's requests to anyone the page could name, so a
/// request there carries nothing out. Keep in step with `MANAGE_HTML_LIBRARY_HOSTS` in
/// `apps/desktop/views/manage/preview/html-viewer.tsx`.
pub(crate) const MANAGE_HTML_PAGE_LIBRARY_HOSTS: &[&str] = &[
    "cdn.jsdelivr.net",
    "unpkg.com",
    "cdnjs.cloudflare.com",
    "fonts.googleapis.com",
    "fonts.gstatic.com",
    "cdn.tailwindcss.com",
    "d3js.org",
    "cdn.plot.ly",
    "code.jquery.com",
];

/// True when a Files resource path names a dot-file or lies in a dot-folder (any segment that
/// starts with `.`). The reserved mount segments that address an outside file's folder
/// (`.ghostex-chat-file/<id>/`) and the configured Docs folder (`.ghostex-docs-root/`) are the
/// app's own addressing, not a dot-folder; the path inside them follows the same rule.
pub(crate) fn manage_docs_resource_path_is_hidden(relative_path: &str) -> bool {
    let inner = if let Some((_, inner)) = manage_chat_file_address(relative_path) {
        inner
    } else if let Some(inner) = relative_path
        .strip_prefix(MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT)
        .and_then(|rest| rest.strip_prefix('/'))
    {
        inner
    } else {
        relative_path
    };
    inner.is_empty() || inner.split('/').any(|segment| segment.starts_with('.'))
}

/// Whether the HTML file at `page_resource_path` may read `relative_path` (CORS): a file in the
/// page's own folder or below. A page at the project root reads the project, not the folders
/// mounted under reserved segments.
pub(crate) fn manage_docs_page_may_read(page_resource_path: &str, relative_path: &str) -> bool {
    if manage_docs_resource_path_is_hidden(relative_path) {
        return false;
    }
    match page_resource_path.rsplit_once('/') {
        Some((folder, _)) if !folder.is_empty() => relative_path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/')),
        _ => !relative_path.starts_with('.'),
    }
}

/// One query value of the embed page's own URL.
pub(crate) fn manage_embed_url_param(page_url: &str, name: &str) -> Option<String> {
    url::Url::parse(page_url).ok().and_then(|url| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    })
}

/// The path the page's own resources load under (`resourcePath`, else `path`).
pub(crate) fn manage_embed_url_resource_path(page_url: &str) -> Option<String> {
    manage_embed_url_param(page_url, "resourcePath")
        .filter(|path| !path.is_empty())
        .or_else(|| manage_embed_url_param(page_url, "path"))
}

/// True unless the embed page shows a drawing or a media file: then every frame below the page
/// is the rendered HTML file (or a frame it made), and the HTML rules apply. Anything that does
/// not read as a drawing or media embed counts as HTML, so a surprise fails closed.
pub(crate) fn manage_embed_url_shows_html(page_url: &str) -> bool {
    let Ok(url) = url::Url::parse(page_url) else {
        return true;
    };
    let media = url
        .query_pairs()
        .find(|(key, _)| key == "media")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default();
    let drawing = url
        .query_pairs()
        .any(|(key, value)| key == "path" && value.to_ascii_lowercase().ends_with(".excalidraw"));
    media.is_empty() && !drawing
}

/// Whether the rendered HTML may send a request to `url` (other than the Files resource origin,
/// which serves it under its own rules): GET to a library CDN, or a `data:`/`blob:` URL, which
/// never leaves the computer.
pub(crate) fn manage_html_page_may_request(url: &str, method: &str) -> bool {
    if url.starts_with("data:") || url.starts_with("blob:") {
        return true;
    }
    if !(method.is_empty()
        || method.eq_ignore_ascii_case("GET")
        || method.eq_ignore_ascii_case("HEAD"))
    {
        return false;
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.port().is_none()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed
            .host_str()
            .is_some_and(|host| MANAGE_HTML_PAGE_LIBRARY_HOSTS.contains(&host))
}

/// Whether a frame of the rendered HTML may load `url` in place: its own document and other
/// documents of the Files resource origin. `data:` documents are refused: they would not carry
/// the page's content policy. Anything else is a way out (a clicked link opens in the browser
/// instead, see `GhostexManageDocsRequestHandler::on_before_browse`).
pub(crate) fn manage_html_frame_may_navigate(url: &str) -> bool {
    url == "about:srcdoc"
        || url == "about:blank"
        || url.starts_with("blob:")
        || url.starts_with(MANAGE_EMBED_RESOURCE_BASE_URL)
}

/// The fields of the embed page's own URL a Files request is held to.
struct ManageEmbedPage {
    path: String,
    project_id: String,
    project_editor_id: String,
    annotate: bool,
}

impl ManageEmbedPage {
    fn from_url(page_url: &str) -> Option<Self> {
        let param = |name: &str| manage_embed_url_param(page_url, name);
        if param("embed").as_deref() != Some("1") {
            return None;
        }
        let path = param("path").filter(|path| !path.is_empty())?;
        let project_id = param("projectId").unwrap_or_default();
        Some(Self {
            path,
            project_editor_id: param("projectEditorId").unwrap_or_else(|| project_id.clone()),
            project_id,
            annotate: param("annotate").as_deref() == Some("1"),
        })
    }
}

/// Whether the app answers a Files request from the embed page at `page_url`; `Err` says why not.
///
/// CDXC:Docs 2026-11-14 DECISION:
/// The embed page shows one file, so it may only ask for what that takes: `read` its own file,
/// `save` it when it is a drawing (Excalidraw's autosave), `openDocsFile` another HTML file (a
/// link in the page), and `sendPageAnnotation` for its own file while Annotate is on. Every other
/// action (writing, moving, deleting or opening other files, Open With, the agent input) is
/// refused here, so nothing that runs in that page can reach them even if it reached the bridge.
pub(crate) fn manage_embed_request_allowed(
    page_url: &str,
    request: &serde_json::Value,
) -> Result<(), &'static str> {
    let page = ManageEmbedPage::from_url(page_url).ok_or("This page cannot use Files.")?;
    let field = |name: &str| request.get(name).and_then(serde_json::Value::as_str);
    let action = field("action").unwrap_or_default();
    let path = field("path").unwrap_or_default();
    // A request names the page's own project, or none (it then runs in the page's project).
    if field("projectId").is_some_and(|id| id != page.project_id)
        || field("projectEditorId").is_some_and(|id| id != page.project_editor_id)
    {
        return Err("This page can only use its own project.");
    }
    let own_file = path == page.path;
    match action {
        "read" if own_file => Ok(()),
        "save" if own_file && page.path.to_ascii_lowercase().ends_with(".excalidraw") => Ok(()),
        "sendPageAnnotation" if own_file && page.annotate => Ok(()),
        "sendPageAnnotation" if own_file => Err("Annotate is off for this file."),
        "openDocsFile" if manage_embed_linked_html_path_allowed(path) => Ok(()),
        _ => Err("This page cannot do that."),
    }
}

/// A link from an HTML file to another one: a relative `.html`/`.htm` path with no dot-folder,
/// empty or `..` segment.
fn manage_embed_linked_html_path_allowed(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && (lower.ends_with(".html") || lower.ends_with(".htm"))
        && !path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "..")
        && !manage_docs_resource_path_is_hidden(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::helpers::MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT;
    use serde_json::json;

    const CHAT_ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn page(path: &str, annotate: bool) -> String {
        let mut url = url::Url::parse(
            "file:///Applications/Ghostex.app/Contents/Resources/sidebar/manage.html",
        )
        .unwrap();
        url.query_pairs_mut()
            .append_pair("projectId", "P1")
            .append_pair("projectEditorId", "project-editor:P1:manage")
            .append_pair("embed", "1")
            .append_pair("path", path)
            .append_pair("resourcePath", path)
            .append_pair("annotate", if annotate { "1" } else { "0" })
            .append_pair("media", "");
        url.to_string()
    }

    fn request(action: &str, path: &str) -> serde_json::Value {
        json!({ "action": action, "path": path, "projectId": "P1", "projectEditorId": "project-editor:P1:manage", "requestId": "r" })
    }

    #[test]
    fn embed_page_reads_only_its_own_file() {
        let url = page("docs/report.html", true);
        assert!(manage_embed_request_allowed(&url, &request("read", "docs/report.html")).is_ok());
        assert!(manage_embed_request_allowed(&url, &request("read", ".env")).is_err());
        assert!(manage_embed_request_allowed(&url, &request("read", "docs/data.json")).is_err());
    }

    #[test]
    fn embed_page_saves_only_its_own_drawing() {
        let drawing = page("docs/plan.excalidraw", false);
        assert!(
            manage_embed_request_allowed(&drawing, &request("save", "docs/plan.excalidraw"))
                .is_ok()
        );
        assert!(manage_embed_request_allowed(&drawing, &request("save", ".git/config")).is_err());
        let html = page("docs/report.html", true);
        assert!(manage_embed_request_allowed(&html, &request("save", "docs/report.html")).is_err());
    }

    #[test]
    fn embed_page_refuses_every_other_action() {
        let url = page("docs/report.html", true);
        for action in [
            "list",
            "search",
            "rename",
            "duplicate",
            "delete",
            "createFolder",
            "move",
            "openWithSystemApp",
            "revealInFinder",
            "openInCodeView",
            "addToSessionContext",
            "sqlite",
            "annotationSendTarget",
            "sendAnnotationFeedback",
            "openMermaidDiagram",
            "",
        ] {
            assert!(
                manage_embed_request_allowed(&url, &request(action, "docs/report.html")).is_err(),
                "{action} should be refused"
            );
        }
    }

    #[test]
    fn embed_page_sends_annotations_only_while_annotate_is_on() {
        let on = page("docs/report.html", true);
        let off = page("docs/report.html", false);
        assert!(
            manage_embed_request_allowed(&on, &request("sendPageAnnotation", "docs/report.html"))
                .is_ok()
        );
        assert!(
            manage_embed_request_allowed(&off, &request("sendPageAnnotation", "docs/report.html"))
                .is_err()
        );
        assert!(
            manage_embed_request_allowed(&on, &request("sendPageAnnotation", "docs/other.html"))
                .is_err()
        );
    }

    #[test]
    fn embed_page_opens_only_other_html_files() {
        let url = page("docs/report.html", true);
        assert!(
            manage_embed_request_allowed(&url, &request("openDocsFile", "docs/other.html")).is_ok()
        );
        assert!(
            manage_embed_request_allowed(&url, &request("openDocsFile", "docs/Index.HTM")).is_ok()
        );
        let outside = format!("{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/{CHAT_ID}/sibling.html");
        assert!(manage_embed_request_allowed(&url, &request("openDocsFile", &outside)).is_ok());
        for refused in [
            "docs/run.command",
            ".git/x.html",
            "docs/../x.html",
            "/etc/x.html",
            "docs/.hidden/x.html",
            "",
        ] {
            assert!(
                manage_embed_request_allowed(&url, &request("openDocsFile", refused)).is_err(),
                "{refused} should be refused"
            );
        }
    }

    #[test]
    fn embed_page_cannot_name_another_project_or_be_another_page() {
        let url = page("docs/report.html", true);
        let mut other = request("read", "docs/report.html");
        other["projectId"] = json!("P2");
        assert!(manage_embed_request_allowed(&url, &other).is_err());
        let unnamed = json!({ "action": "read", "path": "docs/report.html", "requestId": "r" });
        assert!(manage_embed_request_allowed(&url, &unnamed).is_ok());
        let not_embed = "file:///x/manage.html?path=docs%2Freport.html";
        assert!(
            manage_embed_request_allowed(not_embed, &request("read", "docs/report.html")).is_err()
        );
    }

    #[test]
    fn dot_paths_are_hidden_but_mount_segments_are_not() {
        for hidden in [
            ".env",
            ".git/config",
            ".claude/settings.json",
            "docs/.secret/a.png",
            "docs/.DS_Store",
            ".ghostex-chat-file/short/a.html",
        ] {
            assert!(manage_docs_resource_path_is_hidden(hidden), "{hidden}");
        }
        for plain in ["docs/a.png", "a.html", "docs/sub/b.json", "docs/a.b.c.png"] {
            assert!(!manage_docs_resource_path_is_hidden(plain), "{plain}");
        }
        let chat = format!("{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/{CHAT_ID}/page.html");
        assert!(!manage_docs_resource_path_is_hidden(&chat));
        let chat_dot = format!("{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/{CHAT_ID}/.env");
        assert!(manage_docs_resource_path_is_hidden(&chat_dot));
        let extra = format!("{MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT}/notes/a.png");
        assert!(!manage_docs_resource_path_is_hidden(&extra));
        let extra_dot = format!("{MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT}/.obsidian/app.json");
        assert!(manage_docs_resource_path_is_hidden(&extra_dot));
    }

    #[test]
    fn page_reads_its_own_folder_and_below_only() {
        assert!(manage_docs_page_may_read(
            "docs/report.html",
            "docs/data.json"
        ));
        assert!(manage_docs_page_may_read(
            "docs/report.html",
            "docs/sub/nested.json"
        ));
        assert!(!manage_docs_page_may_read(
            "docs/report.html",
            "outside.json"
        ));
        assert!(!manage_docs_page_may_read(
            "docs/report.html",
            "docs-other/a.json"
        ));
        assert!(!manage_docs_page_may_read("docs/report.html", "docs/.env"));
        assert!(manage_docs_page_may_read("report.html", "data/a.json"));
        assert!(!manage_docs_page_may_read("report.html", ".env"));
        let chat_page = format!("{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/{CHAT_ID}/page.html");
        let chat_data = format!("{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/{CHAT_ID}/data.json");
        assert!(manage_docs_page_may_read(&chat_page, &chat_data));
        assert!(!manage_docs_page_may_read("report.html", &chat_data));
    }

    #[test]
    fn html_page_reaches_only_library_cdns() {
        assert!(manage_html_page_may_request(
            "https://cdn.jsdelivr.net/npm/chart.js@4",
            "GET"
        ));
        assert!(manage_html_page_may_request(
            "https://fonts.gstatic.com/s/inter.woff2",
            ""
        ));
        assert!(manage_html_page_may_request("data:text/plain,hi", "GET"));
        assert!(!manage_html_page_may_request(
            "https://cdn.jsdelivr.net/npm/x",
            "POST"
        ));
        assert!(!manage_html_page_may_request(
            "http://cdn.jsdelivr.net/npm/x",
            "GET"
        ));
        assert!(!manage_html_page_may_request(
            "https://cdn.jsdelivr.net:8443/x",
            "GET"
        ));
        assert!(!manage_html_page_may_request(
            "https://evil.example/?d=secret",
            "GET"
        ));
        assert!(!manage_html_page_may_request(
            "https://cdn.jsdelivr.net.evil.example/x",
            "GET"
        ));
        assert!(!manage_html_page_may_request(
            "http://127.0.0.1:47123/exfil",
            "GET"
        ));
        assert!(!manage_html_page_may_request(
            "wss://cdn.jsdelivr.net/x",
            "GET"
        ));
    }

    #[test]
    fn html_frames_stay_on_their_document_and_the_files_origin() {
        assert!(manage_html_frame_may_navigate("about:srcdoc"));
        assert!(manage_html_frame_may_navigate(
            "https://ghostex-docs.invalid/docs/other.html"
        ));
        assert!(!manage_html_frame_may_navigate("https://example.com/"));
        assert!(!manage_html_frame_may_navigate(
            "data:text/html,<script>1</script>"
        ));
        assert!(!manage_html_frame_may_navigate(
            "https://ghostex-docs.invalid.evil.example/"
        ));
    }

    #[test]
    fn only_html_embeds_get_the_html_rules() {
        assert!(manage_embed_url_shows_html(&page("docs/report.html", true)));
        assert!(!manage_embed_url_shows_html(&page(
            "docs/plan.excalidraw",
            false
        )));
        assert!(!manage_embed_url_shows_html(
            "file:///x/manage.html?embed=1&path=a.pdf&media=pdf"
        ));
        assert!(manage_embed_url_shows_html("not a url"));
        assert_eq!(
            manage_embed_url_resource_path(&page("docs/report.html", true)).as_deref(),
            Some("docs/report.html")
        );
    }
}

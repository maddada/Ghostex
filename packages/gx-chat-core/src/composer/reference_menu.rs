//! The right-click menu behind a composer pill or a transcript link.
//!
//! Port of `packages/shared/session-chat-presentation/reference-menu.ts`. The rows are serialized
//! exactly as the TypeScript built them, key order included, because the renderer forwards
//! `command` back to the host untouched.

use serde::{Deserialize, Serialize};

use crate::composer::json::OrderedMap;
use crate::composer::links::{
    classify_link_href, file_position_from_href, FilePosition, LinkTarget,
};
use crate::composer::reference_pills::{media_kind, path_noun, reference_kind, ReferenceKind};
use crate::ordered;

/// One row of the menu: the command the host runs, an icon, and a label.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceMenuRow {
    pub command: OrderedMap,
    /// Only the transcript menu's own rows carry this; a reference row leaves it out, and it sits
    /// between `command` and `iconPath` because that is where `sessionChatTranscriptMenuRows`
    /// writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    pub icon_path: String,
    pub label: String,
}

/// The rows for `href`, empty when the chat can do nothing with it.
pub fn reference_menu_rows(href: &str) -> Vec<ReferenceMenuRow> {
    let target = classify_link_href(href);
    if let LinkTarget::Url(url) = target {
        return vec![
            ReferenceMenuRow {
                command: ordered! { "text": url, "type": "copyText" },
                icon_path: "titlebar/copy.svg".to_string(),
                label: "Copy URL".to_string(),
                disabled: None,
            },
            ReferenceMenuRow {
                command: ordered! {
                    "action": "openLink",
                    "external": false,
                    "forceEmbedded": true,
                    "type": "host",
                    "url": url,
                },
                icon_path: "titlebar/world.svg".to_string(),
                label: "Open in Embedded Browser".to_string(),
                disabled: None,
            },
            ReferenceMenuRow {
                command: ordered! {
                    "action": "openLink",
                    "external": true,
                    "type": "host",
                    "url": url,
                },
                icon_path: "titlebar/link.svg".to_string(),
                label: "Open in External Browser".to_string(),
                disabled: None,
            },
        ];
    }
    let LinkTarget::File(path) = target else {
        return Vec::new();
    };
    let position = file_position_from_href(href);
    let mut rows = Vec::new();
    let noun = path_noun(&path);
    if media_kind(&path).is_some() {
        // No `view`: the host picks where media opens (the desktop's "open in" settings).
        rows.push(ReferenceMenuRow {
            command: ordered! { "action": "openFile", "path": path.clone(), "type": "host" },
            icon_path: "titlebar/external-link.svg".to_string(),
            label: format!("Open {noun}"),
            disabled: None,
        });
    } else if reference_kind("", &path) != ReferenceKind::Folder {
        rows.push(ReferenceMenuRow {
            command: file_command(&path, "code", position),
            icon_path: "titlebar/code.svg".to_string(),
            label: "Open in VS Code".to_string(),
            disabled: None,
        });
    }
    // CDXC:SessionChat 2026-10-17 DECISION:
    // User (Q233): "Yes, for every file except folders and media": Open in Files is offered for every file, not only Markdown, HTML and Excalidraw, because the Files view opens any file since Quick Access's Files tab. Folders and media (pictures, video, audio, and PDFs, which keep their own Open row) are left out. The phone shows the row only for files its viewer opens (`phoneOpensFile` in apps/mobile/app); a remote session's file opens in Files through the remote route.
    let kind = reference_kind("", &path);
    if media_kind(&path).is_none() && kind != ReferenceKind::Folder && kind != ReferenceKind::Image
    {
        rows.push(ReferenceMenuRow {
            command: file_command(&path, "docs", position),
            icon_path: "titlebar/file-text.svg".to_string(),
            label: "Open in Files".to_string(),
            disabled: None,
        });
    }
    rows.push(ReferenceMenuRow {
        command: ordered! { "text": path, "type": "copyText" },
        icon_path: "titlebar/copy.svg".to_string(),
        label: "Copy Path".to_string(),
        disabled: None,
    });
    rows.push(ReferenceMenuRow {
        command: ordered! { "action": "locateFile", "path": path, "type": "host" },
        icon_path: "titlebar/folder-open.svg".to_string(),
        label: format!("Open {noun} Location"),
        disabled: None,
    });
    rows
}

/// `{ action, path, type, view, ...position }`: the position spreads last, after `view`.
fn file_command(path: &str, view: &str, position: Option<FilePosition>) -> OrderedMap {
    let mut command = ordered! {
        "action": "openFile",
        "path": path,
        "type": "host",
        "view": view,
    };
    if let Some(position) = position {
        command.insert("line", position.line);
        if let Some(end_line) = position.end_line {
            command.insert("endLine", end_line);
        }
        if let Some(column) = position.column {
            command.insert("column", column);
        }
    }
    command
}

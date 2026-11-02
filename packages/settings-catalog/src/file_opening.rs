//! Settings > File opening: the choices of its two dropdowns (`documentFileOpenTarget` for HTML,
//! Markdown and images, `otherFileOpenTarget` for every other file) and what the settings that came
//! before them still choose.
//!
//! CDXC:Navigation 2026-10-17 DECISION:
//! User: Settings > File opening gets two dropdowns, one for HTML, Markdown and images and one for every other file (code and text), both defaulting to Files. Their choices are Files, the built-in Code view, VS Code and the other editors the app already knows; an editor that isn't installed on this computer shows but can't be picked. Existing users' Markdown, HTML and Images choices must not silently change, so until the first dropdown is saved each kind keeps following its old key.

use serde_json::{Map, Value};

use crate::data::BUILT_IN_WORKSPACE_OPEN_TARGETS;
use crate::json::J;
use crate::rows::{option, SettingOption};

/// Files, the VS Code view, the system's default app, the VS Code app, then every other built-in
/// Open In editor in its catalog order (Finder is not an editor).
///
/// CDXC:Navigation 2026-11-02 DECISION:
/// User: "fix Issue #391 please", whose feature request asks that `documentFileOpenTarget` and `otherFileOpenTarget` get the same system default app choice as `videoFileOpenTarget` and `audioFileOpenTarget`, so a file opens in the app Windows, macOS or Linux has set for its type (Markdown in the user's usual editor without it being on the editor list). It is the same `system-app` value and opens through the same route as video and audio; defaults stay Files.
pub fn file_open_target_options() -> Vec<SettingOption> {
    let editors: Vec<(&str, &str)> = BUILT_IN_WORKSPACE_OPEN_TARGETS
        .as_array()
        .iter()
        .filter_map(|target| {
            Some((
                target.get("id").and_then(J::as_str)?,
                target.get("label").and_then(J::as_str)?,
            ))
        })
        .filter(|(id, _)| *id != "finder")
        .collect();
    // The built-in view is called VS Code, so the VS Code app is told apart as "VS Code app".
    let mut options = vec![
        option("Files", "files"),
        option("VS Code", "code"),
        option("System App", "system-app"),
    ];
    options.extend(
        editors
            .iter()
            .filter(|(id, _)| *id == "vscode")
            .chain(editors.iter().filter(|(id, _)| *id != "vscode"))
            .map(|(id, label)| {
                if *id == "vscode" {
                    option("VS Code app", *id)
                } else {
                    option(*label, *id)
                }
            }),
    );
    options
}

/// The kinds `documentFileOpenTarget` covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentFileKind {
    Markdown,
    Html,
    Image,
}

/// Where `kind` opened under the settings before `documentFileOpenTarget`
/// (`markdownFileOpenView`, `htmlFileOpenView`, `imageFileOpenTarget`): `"files"`, `"code"` or
/// `"system-app"`.
pub fn legacy_document_file_open_target(
    settings: &Map<String, Value>,
    kind: DocumentFileKind,
) -> &'static str {
    let read = |key: &str| settings.get(key).and_then(Value::as_str);
    match kind {
        DocumentFileKind::Markdown | DocumentFileKind::Html => {
            let key = if kind == DocumentFileKind::Markdown {
                "markdownFileOpenView"
            } else {
                "htmlFileOpenView"
            };
            match read(key) {
                Some("code") => "code",
                _ => "files",
            }
        }
        DocumentFileKind::Image => match read("imageFileOpenTarget") {
            Some("system-app") => "system-app",
            _ => "files",
        },
    }
}

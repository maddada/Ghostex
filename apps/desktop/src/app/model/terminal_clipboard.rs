// C1 wave-3 re-cluster: terminal clipboard read/paste/copy helpers and terminal attachment references, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;


/*
CDXC:GPUITerminalPaste 2026-06-23-09:59:
Terminal paste may read the platform clipboard only at the command-action boundary and may forward only explicit string entries to the exact focused mounted Ghostty surface. Do not use ClipboardItem::text here because it can synthesize local file paths from external-path entries; path, image, metadata-only, and empty clipboard data must no-op without logging, persistence, or fallback text.

CDXC:GPUITerminalClipboard 2026-06-23-19:07:
Runtime clipboard drains may use the app-thread standard clipboard only after the caller has re-fetched the exact currently mounted Ghostty surface owner from the Agents or command surface map. The handoff reads through the explicit-string boundary used by Cmd+V, writes only runtime-provided text as a new string ClipboardItem, leaves selection clipboard unsupported by Ghostty runtime config, and does not log, persist, inspect, or store clipboard content beyond the closure call.

CDXC:GPUICommandTerminalClipboard 2026-06-27-04:10:
Command runtime clipboard drains need source-level regression evidence that requester identity comes from still-mounted command owners, not focused-shell fallback. Keep authorization as the intersection of a snapped owner key and the current mounted-owner map before any app-thread clipboard read or runtime-text write can run.

CDXC:GPUITerminalImagePaste 2026-06-27-10:23:
GPUI command-pane paste needs a pure normalization helper that keeps Paste previewable images disabled behavior identical to explicit-string-only paste, but when enabled converts only validated local image file references or raw clipboard image bytes into numbered Markdown links. Do not call ClipboardItem::text, do not synthesize non-image paths, and do not persist anything except saved raw image bytes under the resolved Ghostex image directory.
*/
pub(crate) fn terminal_clipboard_paste_text(
    item: &ClipboardItem,
    paste_previewable_images_enabled: bool,
    factory_droid_image_padding: bool,
) -> Option<String> {
    if paste_previewable_images_enabled {
        if let Some(markdown) = terminal_clipboard_previewable_image_markdown_text(item) {
            return Some(if factory_droid_image_padding {
                format!("  {markdown}")
            } else {
                markdown
            });
        }
    }

    terminal_clipboard_explicit_string_entries_text(item)
}


pub(crate) fn terminal_clipboard_explicit_string_entries_text(item: &ClipboardItem) -> Option<String> {
    let mut text = String::new();

    for entry in item.entries() {
        if let ClipboardEntry::String(clipboard_string) = entry {
            text.push_str(clipboard_string.text());
        }
    }

    if text.is_empty() { None } else { Some(text) }
}


pub(crate) fn terminal_clipboard_previewable_image_markdown_text(item: &ClipboardItem) -> Option<String> {
    let file_paths = terminal_clipboard_image_file_paths(item);
    if !file_paths.is_empty() {
        return Some(terminal_clipboard_markdown_image_references(&file_paths));
    }

    terminal_clipboard_saved_image_markdown_text(item)
}


pub(crate) fn terminal_clipboard_image_file_paths(item: &ClipboardItem) -> Vec<PathBuf> {
    let external_paths = terminal_clipboard_external_image_file_paths(item);
    if !external_paths.is_empty() {
        return external_paths;
    }

    terminal_clipboard_string_image_file_paths(item)
}


pub(crate) fn terminal_clipboard_external_image_file_paths(item: &ClipboardItem) -> Vec<PathBuf> {
    let mut image_paths = Vec::new();
    let mut seen = HashSet::new();

    for entry in item.entries() {
        if let ClipboardEntry::ExternalPaths(paths) = entry {
            for path in paths.paths() {
                if is_project_board_image_file_path(path) && seen.insert(path.clone()) {
                    image_paths.push(path.clone());
                }
            }
        }
    }

    image_paths
}


pub(crate) fn terminal_clipboard_string_image_file_paths(item: &ClipboardItem) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for entry in item.entries() {
        if let ClipboardEntry::String(clipboard_string) = entry {
            candidates.extend(
                clipboard_string
                    .text()
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty()),
            );
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    let mut image_paths = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidates {
        let Some(path) = project_board_image_path_from_reference(candidate) else {
            return Vec::new();
        };
        if !is_project_board_image_file_path(&path) {
            return Vec::new();
        }
        if seen.insert(path.clone()) {
            image_paths.push(path);
        }
    }

    image_paths
}


pub(crate) fn terminal_clipboard_saved_image_markdown_text(item: &ClipboardItem) -> Option<String> {
    for entry in item.entries() {
        if let ClipboardEntry::Image(image) = entry {
            let bytes = image.bytes();
            if bytes.is_empty() || bytes.len() > PROJECT_BOARD_CLIPBOARD_IMAGE_MAX_BYTES {
                return None;
            }

            let path =
                unique_project_board_image_path(project_board_image_extension(image.format()))
                    .ok()?;
            let path = terminal_clipboard_absolute_path(path)?;
            fs::write(&path, bytes).ok()?;
            return Some(terminal_clipboard_markdown_image_reference(&path, 1));
        }
    }

    None
}


/*
CDXC:GPUITerminalRemoteImagePaste 2026-08-21:
The remote paste route needs the clipboard image *before* it is written
anywhere, because a remote terminal's reference has to point at a file on the
remote machine. This extractor keeps the exact acceptance order the local
Markdown helper uses (validated image file references first, raw clipboard
image bytes second) so local and remote paste accept and reject the same
clipboard shapes; only the destination differs.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TerminalClipboardImagePayload {
    FilePaths(Vec<PathBuf>),
    Bytes {
        bytes: Vec<u8>,
        extension: &'static str,
    },
}


pub(crate) fn terminal_clipboard_image_payload(item: &ClipboardItem) -> Option<TerminalClipboardImagePayload> {
    let file_paths = terminal_clipboard_image_file_paths(item);
    if !file_paths.is_empty() {
        return Some(TerminalClipboardImagePayload::FilePaths(file_paths));
    }

    for entry in item.entries() {
        if let ClipboardEntry::Image(image) = entry {
            let bytes = image.bytes();
            if bytes.is_empty() || bytes.len() > PROJECT_BOARD_CLIPBOARD_IMAGE_MAX_BYTES {
                return None;
            }

            return Some(TerminalClipboardImagePayload::Bytes {
                bytes: bytes.to_vec(),
                extension: project_board_image_extension(image.format()),
            });
        }
    }

    None
}


pub(crate) fn terminal_clipboard_absolute_path(path: PathBuf) -> Option<PathBuf> {
    if path.is_absolute() {
        Some(path)
    } else {
        env::current_dir()
            .ok()
            .map(|current_dir| current_dir.join(path))
    }
}


pub(crate) fn terminal_clipboard_markdown_image_references(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| terminal_clipboard_markdown_image_reference(path, index + 1))
        .collect::<Vec<_>>()
        .join("\n")
}


pub(crate) fn terminal_clipboard_markdown_image_reference(path: &Path, image_number: usize) -> String {
    format!("[Image #{image_number}]({})", path.to_string_lossy())
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTerminalAttachmentKind {
    Image,
    File,
    Folder,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTerminalAttachmentReference {
    pub(crate) kind: GpuiTerminalAttachmentKind,
    pub(crate) path: String,
}


pub(crate) fn gpui_local_terminal_attachment_reference(
    path: &Path,
) -> Result<GpuiTerminalAttachmentReference, String> {
    let metadata = fs::metadata(path)
        .map_err(|_| "The selected file or folder is no longer available.".to_string())?;
    let kind = if metadata.is_dir() {
        GpuiTerminalAttachmentKind::Folder
    } else if metadata.is_file() && is_project_board_image_file_path(path) {
        GpuiTerminalAttachmentKind::Image
    } else if metadata.is_file() {
        GpuiTerminalAttachmentKind::File
    } else {
        return Err("The selected item is not a file or folder.".to_string());
    };

    #[cfg(target_os = "windows")]
    let referenced_path = if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. })
    ) {
        windows_terminal_backend::wsl_path_for_windows_path(path)
            .map_err(|_| "The selected path could not be converted for WSL.".to_string())?
    } else {
        path.to_string_lossy().into_owned()
    };
    #[cfg(not(target_os = "windows"))]
    let referenced_path = path.to_string_lossy().into_owned();

    Ok(GpuiTerminalAttachmentReference {
        kind,
        path: referenced_path,
    })
}


pub(crate) fn gpui_terminal_attachment_markdown_text(
    references: &[GpuiTerminalAttachmentReference],
) -> String {
    let mut image_number = 0usize;
    let mut file_number = 0usize;
    let mut folder_number = 0usize;
    references
        .iter()
        .map(|reference| {
            let (label, number) = match reference.kind {
                GpuiTerminalAttachmentKind::Image => {
                    image_number += 1;
                    ("Image", image_number)
                }
                GpuiTerminalAttachmentKind::File => {
                    file_number += 1;
                    ("File", file_number)
                }
                GpuiTerminalAttachmentKind::Folder => {
                    folder_number += 1;
                    ("Folder", folder_number)
                }
            };
            format!("[{label} #{number}]({})", reference.path)
        })
        .collect::<Vec<_>>()
        .join(" ")
}


pub(crate) fn terminal_runtime_clipboard_read_text(
    read_standard_clipboard: impl FnOnce() -> Option<ClipboardItem>,
    paste_previewable_images_enabled: bool,
) -> Option<String> {
    read_standard_clipboard().as_ref().and_then(|item| {
        terminal_clipboard_paste_text(item, paste_previewable_images_enabled, false)
    })
}


pub(crate) fn terminal_runtime_clipboard_write_standard_text(
    text: String,
    mut write_standard_clipboard: impl FnMut(ClipboardItem),
) {
    write_standard_clipboard(ClipboardItem::new_string(text));
}


pub(crate) fn terminal_runtime_clipboard_authorized_mounted_slot_ids<SlotId, Owner>(
    snapshot_slot_ids: impl IntoIterator<Item = SlotId>,
    owners_by_slot: &HashMap<SlotId, Owner>,
) -> Vec<SlotId>
where
    SlotId: Copy + Eq + std::hash::Hash,
{
    snapshot_slot_ids
        .into_iter()
        .filter(|slot_id| owners_by_slot.contains_key(slot_id))
        .collect()
}

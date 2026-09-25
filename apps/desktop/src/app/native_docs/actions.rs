//! The files list's menus (Create, the overflow menu and a row's right-click menu), the one action
//! their rows dispatch, the file operations behind them, and the prompts Docs asks.

use gpui::{
    AppContext as _, Bounds, ClipboardItem, Context, Focusable as _, Pixels, Point, Window, px,
};
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};

use super::files::parent_path;
use super::files_list::{CREATE_MENU_ANCHOR, OVERFLOW_MENU_ANCHOR};
use super::state::{DocsEntryKind, DocsRename};
use crate::GhostexGpuiApp;
use crate::app::context_menu::GpuiContextMenu;

/// The Docs page's reserved routing segment for the mounted Docs directory.
const EXTRA_ROOT_MOUNT_PATH: &str = ".ghostex-docs-root";

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct NativeDocsAction {
    pub(crate) command: Value,
}

fn action(command: Value) -> Box<dyn gpui::Action> {
    Box::new(NativeDocsAction { command })
}

/// What a new file is created as (`ManageArtifactKind`).
#[derive(Clone, Copy)]
enum ArtifactKind {
    Markdown,
    Html,
    Excalidraw,
}

impl ArtifactKind {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            "markdown" => Some(Self::Markdown),
            "html" => Some(Self::Html),
            "excalidraw" => Some(Self::Excalidraw),
            _ => None,
        }
    }

    /// `artifactNameParts`.
    fn stem_and_extension(self) -> (&'static str, &'static str) {
        match self {
            Self::Markdown => ("note", "md"),
            Self::Html => ("page", "html"),
            Self::Excalidraw => ("drawing", "excalidraw"),
        }
    }

    /// `createInitialArtifactContent`, byte for byte.
    ///
    /// CDXC:Docs 2026-09-24 SEE-ALSO:
    /// templates/page.html and templates/drawing.excalidraw are generated from `createInitialArtifactContent` in apps/desktop/views/manage/file-tree-utils.ts, and must match it while that page still ships.
    fn initial_content(self) -> &'static str {
        match self {
            Self::Markdown => "# Untitled\n\n",
            Self::Html => include_str!("templates/page.html"),
            Self::Excalidraw => include_str!("templates/drawing.excalidraw"),
        }
    }
}

/// Focuses a text field and selects its text once it is on screen (a field created this frame
/// has no place in the focus path until the next one).
pub(crate) fn focus_and_select_all(input: &gpui::Entity<InputState>, window: &mut Window) {
    let input = input.clone();
    window.on_next_frame(move |window, cx| {
        input.update(cx, |input, cx| input.focus(window, cx));
        window.dispatch_action(Box::new(gpui_component::input::SelectAll), cx);
    });
}

impl GhostexGpuiApp {
    fn native_docs_path_taken(&self, candidate: &str) -> bool {
        let candidate = candidate.to_lowercase();
        self.native_docs
            .entries
            .iter()
            .any(|entry| entry.path.to_lowercase() == candidate)
    }

    /// `createUniqueArtifactPath`: `note.md`, then `note-2.md`, `note-3.md`.
    fn native_docs_new_artifact_path(&self, directory: &str, kind: ArtifactKind) -> String {
        let (stem, extension) = kind.stem_and_extension();
        (1..10_000)
            .map(|index| {
                let suffix = if index == 1 {
                    String::new()
                } else {
                    format!("-{index}")
                };
                format!("{directory}/{stem}{suffix}.{extension}")
            })
            .find(|path| !self.native_docs_path_taken(path))
            .unwrap_or_else(|| format!("{directory}/{stem}-{}.{extension}", uuid::Uuid::new_v4()))
    }

    /// `createUniqueFolderPath`: `folder`, then `folder-2`.
    fn native_docs_new_folder_path(&self, directory: &str) -> String {
        (1..10_000)
            .map(|index| {
                let suffix = if index == 1 {
                    String::new()
                } else {
                    format!("-{index}")
                };
                format!("{directory}/folder{suffix}")
            })
            .find(|path| !self.native_docs_path_taken(path))
            .unwrap_or_else(|| format!("{directory}/folder-{}", uuid::Uuid::new_v4()))
    }

    /// `createDuplicateManageFilePath`: `notes (1).md`, then `notes (2).md`, beside the original.
    fn native_docs_duplicate_path(&self, path: &str) -> String {
        let parent = parent_path(path);
        let name = path.rsplit('/').next().unwrap_or(path);
        let (stem, extension) = match name.rfind('.') {
            Some(index) if index > 0 => (&name[..index], &name[index..]),
            _ => (name, ""),
        };
        let join = |file: String| {
            if parent.is_empty() {
                file
            } else {
                format!("{parent}/{file}")
            }
        };
        (1..10_000)
            .map(|index| join(format!("{stem} ({index}){extension}")))
            .find(|candidate| !self.native_docs_path_taken(candidate))
            .unwrap_or_else(|| join(format!("{stem} ({}){extension}", uuid::Uuid::new_v4())))
    }

    pub(crate) fn handle_native_docs_action(
        &mut self,
        action: &NativeDocsAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = &action.command;
        let text = |key: &str| command[key].as_str().unwrap_or_default().to_string();
        let path = text("path");
        match command["type"].as_str().unwrap_or_default() {
            "open" => {
                let display_path = self.native_docs_display_path(&path);
                self.native_docs_open(&path, &display_path, cx);
            }
            "copyRelativePath" => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    self.native_docs_display_path(&path),
                ));
                crate::app::helpers::gpui_copy_feedback(cx);
            }
            "copyFullPath" | "revealInFinder" | "addToSessionContext" => self
                .native_docs_bridge_side_effect(
                    command["type"].as_str().unwrap_or_default(),
                    &path,
                    cx,
                ),
            "newFile" => {
                if let Some(kind) = ArtifactKind::from_id(&text("kind")) {
                    self.native_docs_create_file(&text("directory"), kind, cx);
                }
            }
            "newFolder" => self.native_docs_create_folder(&text("directory"), cx),
            "duplicate" => self.native_docs_duplicate(&path, cx),
            "rename" => self.native_docs_begin_rename(&path, window, cx),
            "delete" => self.native_docs_confirm_delete(&path, window, cx),
            "refresh" => self.native_docs_refresh(cx),
            "sendAcrossFiles" => self.native_docs_send_across_files(cx),
            "resendAll" => self.native_docs_send_notes(true, cx),
            "configureFolders" => self.native_docs_open_folders_settings(window, cx),
            "barItem" => {
                if let Some(item) = super::format_bar::DocsBarItem::from_command(&text("item")) {
                    let level = command["level"].as_u64().unwrap_or(1).clamp(1, 6) as u8;
                    self.native_docs_run_bar_item(item, Some(level), window, cx);
                }
            }
            _ => {}
        }
        self.native_docs_notify(cx);
    }

    /// The path the tree names `path` by, for a file the tree has listed.
    pub(crate) fn native_docs_display_path(&self, path: &str) -> String {
        self.native_docs
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.display_path.clone())
            .unwrap_or_else(|| path.to_string())
    }

    fn native_docs_bridge_side_effect(&mut self, action: &str, path: &str, cx: &mut Context<Self>) {
        let run = move |this: &mut Self, action: String, path: String, cx: &mut Context<Self>| {
            let request = this.native_docs_request(&action, json!({ "path": path }));
            this.run_docs_files_request(request.to_string(), cx, |this, response, cx| {
                if let Some(error) = response["error"].as_str() {
                    this.dispatch_gpui_workspace_action_toast("error", "Docs", error, cx);
                }
            });
        };
        // Add to Session Context sends what is on disk, so a dirty open file is saved first.
        let dirty = self
            .native_docs
            .document(path)
            .is_some_and(|document| document.dirty);
        if action == "addToSessionContext" && dirty {
            let (action, path_owned) = (action.to_string(), path.to_string());
            self.native_docs_save(path, cx, move |this, cx| run(this, action, path_owned, cx));
        } else {
            run(self, action.to_string(), path.to_string(), cx);
        }
    }

    /// Shows a Docs menu for its trigger. A trigger drawn in one of Docs' child windows (the
    /// files list's drawer, the formatting bar's frosted window) opens its menu over the main
    /// window at the same spot on screen, where every app menu is hosted.
    pub(crate) fn native_docs_show_menu(
        &mut self,
        menu: GpuiContextMenu,
        trigger: Bounds<Pixels>,
        toggle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(origin) = self.native_docs_child_window_origin(window) else {
            if toggle {
                menu.toggle_below(trigger, window, cx);
            } else {
                menu.show(trigger.origin, window, cx);
            }
            return;
        };
        let Some(main) = self.main_window_handle else {
            return;
        };
        let trigger = Bounds::new(origin + trigger.origin, trigger.size);
        let app = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                if toggle {
                    menu.toggle_below_for_app(app, trigger, window, cx);
                } else {
                    menu.show_for_app(app, trigger.origin, window, cx);
                }
            });
        });
    }

    /// The header's `+` menu, or a folder's New File Here, creating inside `directory`.
    pub(crate) fn show_native_docs_create_menu(
        &mut self,
        directory: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let create =
            |kind: &str| action(json!({ "type": "newFile", "kind": kind, "directory": directory }));
        let menu = GpuiContextMenu::new()
            .menu_with_icon(
                "New folder",
                "titlebar/folder-plus.svg",
                false,
                action(json!({ "type": "newFolder", "directory": directory })),
            )
            .menu_with_icon(
                "New Markdown",
                "titlebar/markdown.svg",
                false,
                create("markdown"),
            )
            .menu_with_icon(
                "New HTML",
                "titlebar/file-type-html.svg",
                false,
                create("html"),
            )
            .menu_with_icon(
                "New drawing",
                "titlebar/edit.svg",
                false,
                create("excalidraw"),
            );
        let trigger = CREATE_MENU_ANCHOR.with(|cell| cell.get());
        self.native_docs_show_menu(menu, trigger, true, window, cx);
    }

    /// The header's menu: Refresh and Configure docs folders.
    pub(crate) fn show_native_docs_overflow_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let loading = self.native_docs.load_state == Some(super::state::DocsLoadState::Loading);
        let menu = GpuiContextMenu::new()
            .menu_with_icon(
                "Refresh",
                "titlebar/refresh.svg",
                loading,
                action(json!({ "type": "refresh" })),
            )
            .menu_with_icon(
                "Configure docs folders",
                "titlebar/settings.svg",
                false,
                action(json!({ "type": "configureFolders" })),
            );
        let trigger = OVERFLOW_MENU_ANCHOR.with(|cell| cell.get());
        self.native_docs_show_menu(menu, trigger, true, window, cx);
    }

    /// A tree row's right-click menu, in the Docs page's order.
    pub(crate) fn show_native_docs_entry_menu(
        &mut self,
        path: &str,
        kind: DocsEntryKind,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_file = kind == DocsEntryKind::File;
        let root_node = self.native_docs.entries.iter().any(|entry| {
            entry.path == path && entry.depth == 0 && entry.kind == DocsEntryKind::Directory
        });
        let command = |kind: &str| action(json!({ "type": kind, "path": path }));
        let mut menu = GpuiContextMenu::new()
            .menu_with_icon(
                "Copy Relative Path",
                "titlebar/copy.svg",
                false,
                command("copyRelativePath"),
            )
            .menu_with_icon(
                "Copy Full Path",
                "titlebar/copy-plus.svg",
                false,
                command("copyFullPath"),
            )
            .menu_with_icon(
                if is_file {
                    "Open File Location"
                } else {
                    "Open Folder Location"
                },
                "titlebar/folder-open.svg",
                false,
                command("revealInFinder"),
            );
        if is_file {
            menu = menu.menu_with_icon(
                "Add to Session Context",
                "titlebar/message-plus.svg",
                false,
                command("addToSessionContext"),
            );
        }
        menu = menu.separator();
        if !is_file {
            let create =
                |kind: &str| action(json!({ "type": "newFile", "kind": kind, "directory": path }));
            menu = menu
                .submenu_with_icon(
                    "New File Here",
                    Some("titlebar/file.svg"),
                    vec![
                        ("Markdown".into(), create("markdown")),
                        ("HTML".into(), create("html")),
                        ("Excalidraw".into(), create("excalidraw")),
                    ],
                )
                .menu_with_icon(
                    "New Folder Here",
                    "titlebar/folder-plus.svg",
                    false,
                    action(json!({ "type": "newFolder", "directory": path })),
                );
        }
        if is_file {
            menu = menu.menu_with_icon(
                "Duplicate",
                "titlebar/copy.svg",
                false,
                command("duplicate"),
            );
        }
        if !root_node {
            menu = menu.menu_with_icon("Rename", "titlebar/pencil.svg", false, command("rename"));
        }
        if path != EXTRA_ROOT_MOUNT_PATH {
            menu = menu.menu_with_icon("Delete", "titlebar/trash.svg", false, command("delete"));
        }
        let trigger = Bounds::new(position, gpui::size(px(1.0), px(1.0)));
        self.native_docs_show_menu(menu, trigger, false, window, cx);
    }

    fn native_docs_create_file(
        &mut self,
        directory: &str,
        kind: ArtifactKind,
        cx: &mut Context<Self>,
    ) {
        let directory = if directory.is_empty() {
            "docs"
        } else {
            directory
        };
        let path = self.native_docs_new_artifact_path(directory, kind);
        let request = self.native_docs_request(
            "save",
            json!({ "path": path, "content": kind.initial_content() }),
        );
        let directory = directory.to_string();
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if let Some(error) = response["error"].as_str() {
                this.dispatch_gpui_workspace_action_toast(
                    "error",
                    "Couldn't create the file",
                    error,
                    cx,
                );
                return;
            }
            this.native_docs.expanded.insert(directory.clone());
            this.native_docs_reveal_in_tree(&path);
            this.native_docs_open(&path, &path, cx);
            this.native_docs_refresh(cx);
        });
    }

    fn native_docs_create_folder(&mut self, directory: &str, cx: &mut Context<Self>) {
        let directory = if directory.is_empty() {
            "docs"
        } else {
            directory
        };
        let path = self.native_docs_new_folder_path(directory);
        let request = self.native_docs_request("createFolder", json!({ "path": path }));
        let directory = directory.to_string();
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if let Some(error) = response["error"].as_str() {
                this.dispatch_gpui_workspace_action_toast(
                    "error",
                    "Couldn't create the folder",
                    error,
                    cx,
                );
                return;
            }
            this.native_docs.expanded.insert(directory.clone());
            this.native_docs.expanded.insert(path.clone());
            this.native_docs_refresh(cx);
        });
    }

    /// Duplicates a file beside itself (saving it first when dirty) and opens the copy.
    fn native_docs_duplicate(&mut self, path: &str, cx: &mut Context<Self>) {
        let dirty = self
            .native_docs
            .document(path)
            .is_some_and(|document| document.dirty);
        if dirty {
            let path = path.to_string();
            self.native_docs_save(&path.clone(), cx, move |this, cx| {
                this.native_docs_duplicate(&path, cx)
            });
            return;
        }
        let new_path = self.native_docs_duplicate_path(path);
        let request =
            self.native_docs_request("duplicate", json!({ "path": path, "newPath": new_path }));
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if let Some(error) = response["error"].as_str() {
                this.dispatch_gpui_workspace_action_toast("error", "Couldn't duplicate", error, cx);
                return;
            }
            this.native_docs_open(&new_path, &new_path, cx);
            this.native_docs_refresh(cx);
        });
    }

    /// Turns the row into a name field with the whole name selected.
    fn native_docs_begin_rename(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let renaming = path.to_string();
        let subscription = cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let name = input.read(cx).value().trim().to_string();
                    this.native_docs_commit_rename(&renaming, &name, window, cx);
                }
                InputEvent::Blur => {
                    if this
                        .native_docs
                        .rename
                        .as_ref()
                        .is_some_and(|rename| rename.path == renaming)
                    {
                        this.native_docs.rename = None;
                        this.native_docs_notify(cx);
                    }
                }
                _ => {}
            },
        );
        self.native_docs_focus_list_input(&input, window, cx);
        self.native_docs.rename = Some(DocsRename {
            path: path.to_string(),
            input,
            _subscription: subscription,
        });
    }

    /// `validateManageRenameFileName`, the collision check, and the folder-with-unsaved-file rule.
    fn native_docs_commit_rename(
        &mut self,
        path: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let error = if name.is_empty() {
            Some("Enter a file name.")
        } else if name == "." || name == ".." {
            Some("Use a normal file name.")
        } else if name.contains(['/', '\\', '\0']) {
            Some("File names cannot contain path separators.")
        } else {
            None
        };
        if let Some(error) = error {
            self.dispatch_gpui_workspace_action_toast("error", "Couldn't rename", error, cx);
            return;
        }
        let parent = parent_path(path);
        let new_path = if parent.is_empty() {
            name.to_string()
        } else {
            format!("{parent}/{name}")
        };
        self.native_docs.rename = None;
        if new_path == path {
            self.native_docs_notify(cx);
            return;
        }
        if new_path.to_lowercase() != path.to_lowercase() && self.native_docs_path_taken(&new_path)
        {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't rename",
                "A file or folder with that name already exists.",
                cx,
            );
            return;
        }
        let folder_prefix = format!("{path}/");
        if self
            .native_docs
            .documents
            .iter()
            .any(|document| document.dirty && document.path.starts_with(&folder_prefix))
        {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't rename",
                "Save the current file before renaming its folder.",
                cx,
            );
            return;
        }
        let _ = window;
        let request =
            self.native_docs_request("rename", json!({ "path": path, "newPath": new_path }));
        let old_path = path.to_string();
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if let Some(error) = response["error"].as_str() {
                this.dispatch_gpui_workspace_action_toast("error", "Couldn't rename", error, cx);
                return;
            }
            this.native_docs_remap_paths(&old_path, &new_path, cx);
            this.native_docs_refresh(cx);
        });
    }

    fn native_docs_confirm_delete(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder_prefix = format!("{path}/");
        if self
            .native_docs
            .documents
            .iter()
            .any(|document| document.dirty && document.path.starts_with(&folder_prefix))
        {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't delete",
                "Save the current file before deleting its folder.",
                cx,
            );
            return;
        }
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &format!("Delete {name}?"),
            Some("This can't be undone."),
            &["Cancel", "Delete"],
            cx,
        );
        let path = path.to_string();
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |this, cx| {
                    let request = this.native_docs_request("delete", json!({ "path": path }));
                    let deleted = path.clone();
                    this.run_docs_files_request(
                        request.to_string(),
                        cx,
                        move |this, response, cx| {
                            if let Some(error) = response["error"].as_str() {
                                this.dispatch_gpui_workspace_action_toast(
                                    "error",
                                    "Couldn't delete",
                                    error,
                                    cx,
                                );
                            } else {
                                this.native_docs_forget_paths(&deleted, cx);
                            }
                            this.native_docs_refresh(cx);
                        },
                    );
                });
            }
        })
        .detach();
    }

    /// After a rename or move: open files, drafts, folder state and the selection follow the item.
    pub(crate) fn native_docs_remap_paths(&mut self, old: &str, new: &str, cx: &mut Context<Self>) {
        let remap = |path: &str| -> Option<String> {
            if path == old {
                Some(new.to_string())
            } else {
                path.strip_prefix(&format!("{old}/"))
                    .map(|rest| format!("{new}/{rest}"))
            }
        };
        let state = &mut self.native_docs;
        for document in &mut state.documents {
            if let Some(next) = remap(&document.path) {
                document.display_path = next.clone();
                document.name = next.rsplit('/').next().unwrap_or(&next).to_string();
                document.path = next;
            }
        }
        if let Some(next) = state.active.as_deref().and_then(remap) {
            state.active = Some(next);
        }
        state.expanded = std::mem::take(&mut state.expanded)
            .into_iter()
            .map(|path| remap(&path).unwrap_or(path))
            .collect();
        state.drafts = std::mem::take(&mut state.drafts)
            .into_iter()
            .map(|(path, draft)| (remap(&path).unwrap_or(path), draft))
            .collect();
        if self.native_docs.notes.remap_for_move(old, new) {
            self.native_docs_notes_changed(cx);
        }
        self.native_docs_persist_open_files(cx);
        self.native_docs_write_drafts(cx);
        self.native_docs_notify(cx);
    }

    /// After a delete: the item and everything under it leave the open files, drafts and folder
    /// state, and a deleted selection clears.
    fn native_docs_forget_paths(&mut self, deleted: &str, cx: &mut Context<Self>) {
        let prefix = format!("{deleted}/");
        let gone = |path: &str| path == deleted || path.starts_with(&prefix);
        let state = &mut self.native_docs;
        state.documents.retain(|document| !gone(&document.path));
        if state.active.as_deref().is_some_and(gone) {
            state.active = None;
        }
        state.expanded.retain(|path| !gone(path));
        state.drafts.retain(|path, _| !gone(path));
        if self.native_docs.notes.remove_for_deleted_entry(deleted) {
            self.native_docs_notes_changed(cx);
        }
        self.native_docs_persist_open_files(cx);
        self.native_docs_write_drafts(cx);
        self.native_docs_notify(cx);
    }

    /// Configure docs folders: Settings on the Projects page, where the Docs folders live.
    fn native_docs_open_folders_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modal = crate::app::model::GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = json!({
            "initialTab": "projects",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    /// The Open Files "x": closes every open file, asking once about the unsaved ones.
    pub(crate) fn native_docs_request_close_all(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.native_docs_defer_to_main_window(window, cx, |this, window, cx| {
            this.native_docs_request_close_all(window, cx)
        }) {
            return;
        }
        let (dirty, clean): (Vec<_>, Vec<_>) = self
            .native_docs
            .documents
            .iter()
            .map(|document| (document.path.clone(), document.dirty))
            .partition(|(_, dirty)| *dirty);
        let close_clean = move |this: &mut Self, cx: &mut Context<Self>| {
            for (path, _) in &clean {
                this.native_docs_close(path, cx);
            }
        };
        if dirty.is_empty() {
            close_clean(self, cx);
            return;
        }
        let message = if dirty.len() == 1 {
            let name = self
                .native_docs
                .document(&dirty[0].0)
                .map(|document| document.name.clone())
                .unwrap_or_default();
            format!("Save changes to {name}?")
        } else {
            format!("Save changes to {} files?", dirty.len())
        };
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &message,
            Some("Your changes will be lost if you close the files without saving."),
            &["Save", "Discard", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let choice = answer.await;
            let _ = this.update(cx, |this, cx| {
                let dirty: Vec<String> = dirty.into_iter().map(|(path, _)| path).collect();
                match choice {
                    Ok(0) => {
                        close_clean(this, cx);
                        for path in dirty {
                            let closing = path.clone();
                            this.native_docs_save(&path, cx, move |this, cx| {
                                this.native_docs_close(&closing, cx)
                            });
                        }
                    }
                    Ok(1) => {
                        close_clean(this, cx);
                        for path in dirty {
                            this.native_docs_close(&path, cx);
                        }
                    }
                    _ => {}
                }
            });
        })
        .detach();
    }

    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: closing an open file that has unsaved changes asks first, with Save, Discard, and Cancel, the way familiar editors do.
    pub(crate) fn native_docs_confirm_close(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let deferred_path = path.to_string();
        if self.native_docs_defer_to_main_window(window, cx, move |this, window, cx| {
            this.native_docs_confirm_close(&deferred_path, window, cx)
        }) {
            return;
        }
        let name = self
            .native_docs
            .document(path)
            .map(|document| document.name.clone())
            .unwrap_or_default();
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &format!("Save changes to {name}?"),
            Some("Your changes will be lost if you close the file without saving."),
            &["Save", "Discard", "Cancel"],
            cx,
        );
        let path = path.to_string();
        cx.spawn(async move |this, cx| {
            let choice = answer.await;
            let _ = this.update(cx, |this, cx| match choice {
                Ok(0) => {
                    let closing = path.clone();
                    this.native_docs_save(&path, cx, move |this, cx| {
                        this.native_docs_close(&closing, cx)
                    });
                }
                Ok(1) => this.native_docs_close(&path, cx),
                _ => {}
            });
        })
        .detach();
    }

    /// The files list's search box, created once and kept across projects. Escape in it clears
    /// the query and goes no further.
    pub(crate) fn native_docs_ensure_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.native_docs.search.is_some() {
            return;
        }
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let subscription = cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value().to_string();
                    if query != this.native_docs.search_query {
                        this.native_docs.search_query = query;
                        this.native_docs_notify(cx);
                    }
                }
            },
        );
        self.native_docs.search = Some(search);
        self.native_docs.search_subscription = Some(subscription);
    }

    /// Whether the files search field has keyboard focus.
    pub(crate) fn native_docs_search_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.native_docs
            .search
            .as_ref()
            .is_some_and(|search| search.read(cx).focus_handle(cx).is_focused(window))
    }
}

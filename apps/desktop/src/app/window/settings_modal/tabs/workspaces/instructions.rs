//! Per-workspace agent instructions row inside Settings > Workspaces.
//!
//! Backed by one Markdown file per workspace in Ghostex's config directory
//! (`<config_dir>/workspace-instructions/<workspace_id>.md`), read and written through
//! `/api/readWorkspaceInstructions` and `/api/writeWorkspaceInstructions`. Never edits the user's
//! own global or repository CLAUDE.md / AGENTS.md files.

use super::super::super::super::native_modal_kit::{MODAL_MONO_FONT, hsla};
use super::super::super::fields::{settings_textarea, tooltip_text};
use super::*;
use gpui_component::v_flex;

/// Matches `server/src/workspaces/instructions.rs::MAX_WORKSPACE_INSTRUCTIONS_BYTES`.
const MAX_WORKSPACE_INSTRUCTIONS_BYTES: usize = 32_768;

#[derive(Clone, Default)]
pub(crate) struct WorkspaceInstructionsState {
    pub(crate) content: String,
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) truncated: bool,
    pub(crate) loaded: bool,
    pub(crate) loading: bool,
    pub(crate) saving: bool,
}

impl WorkspacesTab {
    pub(super) fn reload_all_workspace_instructions(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            return;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readWorkspaceInstructions",
            json!({}),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Ok(result) = result
                        && let Some(map) = result.get("workspaces").and_then(Value::as_object)
                    {
                        for (workspace_id, entry) in map {
                            let content = text(entry, "content");
                            let path = text(entry, "path");
                            let exists = entry.get("exists").and_then(Value::as_bool) == Some(true);
                            let truncated =
                                entry.get("truncated").and_then(Value::as_bool) == Some(true);
                            let input_id = instructions_input_id(workspace_id);
                            let state = page.instructions.entry(workspace_id.clone()).or_default();
                            let prev_saved = state.content.clone();
                            state.content = content.clone();
                            state.path = path;
                            state.exists = exists;
                            state.truncated = truncated;
                            state.loaded = true;
                            state.loading = false;
                            // Refresh the visible textarea if the user has not edited away from
                            // the previous saved text.
                            let has_unsaved = page
                                .drafts
                                .get(&input_id)
                                .is_some_and(|draft| draft != &prev_saved);
                            if !has_unsaved {
                                page.drafts.remove(&input_id);
                                page.fields.textareas.remove(&input_id);
                            }
                        }
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    fn reload_workspace_instructions(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            return;
        }
        let state = self.instructions.entry(workspace_id.clone()).or_default();
        state.loading = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readWorkspaceInstructions",
            json!({ "workspaceId": workspace_id }),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let state = page.instructions.entry(workspace_id.clone()).or_default();
                    state.loading = false;
                    match result {
                        Ok(entry) => {
                            state.content = text(&entry, "content");
                            state.path = text(&entry, "path");
                            state.exists =
                                entry.get("exists").and_then(Value::as_bool) == Some(true);
                            state.truncated =
                                entry.get("truncated").and_then(Value::as_bool) == Some(true);
                            state.loaded = true;
                            let input_id = instructions_input_id(&workspace_id);
                            page.drafts.remove(&input_id);
                            page.fields.textareas.remove(&input_id);
                        }
                        Err(error) => {
                            page.toast(
                                "error",
                                "Couldn't reload workspace instructions",
                                &error,
                                cx,
                            );
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn save_workspace_instructions(
        &mut self,
        workspace_id: String,
        content: String,
        cx: &mut Context<Self>,
    ) {
        let state = self.instructions.entry(workspace_id.clone()).or_default();
        if state.saving {
            return;
        }
        state.saving = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/writeWorkspaceInstructions",
            json!({
                "workspaceId": workspace_id,
                "content": content,
            }),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let state = page.instructions.entry(workspace_id.clone()).or_default();
                    state.saving = false;
                    match result {
                        Ok(entry) => {
                            let saved = text(&entry, "content");
                            let path = text(&entry, "path");
                            let exists = entry.get("exists").and_then(Value::as_bool) == Some(true);
                            state.content = saved;
                            state.path = path;
                            state.exists = exists;
                            state.truncated = false;
                            state.loaded = true;
                            let input_id = instructions_input_id(&workspace_id);
                            page.drafts.remove(&input_id);
                            page.fields.textareas.remove(&input_id);
                        }
                        Err(error) => {
                            page.toast("error", "Couldn't save workspace instructions", &error, cx);
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    pub(super) fn agent_instructions_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        workspace_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self
            .instructions
            .get(workspace_id)
            .cloned()
            .unwrap_or_default();
        let input_id = instructions_input_id(workspace_id);
        let shown = self
            .drafts
            .get(&input_id)
            .cloned()
            .unwrap_or_else(|| state.content.clone());
        let draft_id = input_id.clone();
        let placeholder = format!(
            "## {workspace_name} rules\n- Rules for every project in {workspace_name}.\n- Applies on top of global rules; repo CLAUDE.md / AGENTS.md still govern repo details."
        );
        let input = FieldStates::textarea_state(
            self,
            &input_id,
            &shown,
            Some(&placeholder),
            (5, 20),
            move |page: &mut Self, text, _window, cx| {
                page.drafts.insert(draft_id.clone(), text);
                cx.notify();
            },
            window,
            cx,
        );
        let changed = self.draft_changed(&input_id, &state.content);
        let too_long = shown.len() > MAX_WORKSPACE_INSTRUCTIONS_BYTES;
        let save_workspace = workspace_id.to_string();
        let save_id = input_id.clone();
        let discard_id = input_id.clone();
        let open_path = state.path.clone();
        let can_open_file = state.exists && !open_path.is_empty();
        let reload_workspace = workspace_id.to_string();

        let status_label = if state.saving {
            "Saving...".to_string()
        } else if state.loading {
            "Reloading from file...".to_string()
        } else if too_long {
            "At most 32 KB.".to_string()
        } else if state.truncated && !changed {
            "File exceeds 32 KB · showing the first 32 KB".to_string()
        } else if changed {
            "Unsaved changes".to_string()
        } else if !state.content.trim().is_empty() {
            "Saved on this computer (max 32 KB)".to_string()
        } else {
            "Empty · stored on this computer only (max 32 KB)".to_string()
        };

        let display_path = shorten_home_path(&state.path);
        let full_path = state.path.clone();

        let control = v_flex()
            .w_full()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .font_family(MODAL_MONO_FONT)
                    .child(settings_textarea(
                        p,
                        &input,
                        126.0,
                        true,
                        state.saving,
                        window,
                        cx,
                    )),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .items_center()
                            .child(settings_button(
                                p,
                                SharedString::from(format!(
                                    "workspace-instructions-save-{workspace_id}"
                                )),
                                "Save instructions",
                                None,
                                ButtonVariant::Outline,
                                state.saving || !changed || too_long,
                                too_long.then(|| SharedString::from("At most 32 KB.")),
                                move |page: &mut Self, _window, cx| {
                                    let typed = page.draft(&save_id);
                                    page.save_workspace_instructions(
                                        save_workspace.clone(),
                                        typed,
                                        cx,
                                    );
                                },
                                cx,
                            ))
                            .when(changed, |row| {
                                row.child(settings_button(
                                    p,
                                    SharedString::from(format!(
                                        "workspace-instructions-discard-{workspace_id}"
                                    )),
                                    "Discard",
                                    None,
                                    ButtonVariant::Ghost,
                                    state.saving,
                                    None,
                                    move |page: &mut Self, _window, cx| {
                                        page.drafts.remove(&discard_id);
                                        page.fields.textareas.remove(&discard_id);
                                        cx.notify();
                                    },
                                    cx,
                                ))
                            })
                            .child(settings_button(
                                p,
                                SharedString::from(format!(
                                    "workspace-instructions-open-{workspace_id}"
                                )),
                                "Open file",
                                Some("modals/settings/file-text.svg"),
                                ButtonVariant::Outline,
                                !can_open_file,
                                (!can_open_file).then(|| {
                                    SharedString::from(
                                        "Save instructions first to create the file.",
                                    )
                                }),
                                move |page: &mut Self, _window, cx| {
                                    if !open_path.is_empty() {
                                        post_store_message(
                                            &page.store,
                                            json!({
                                                "type": "openWorkspaceInstructionsFile",
                                                "path": open_path,
                                            }),
                                            cx,
                                        );
                                    }
                                },
                                cx,
                            ))
                            .child(settings_button(
                                p,
                                SharedString::from(format!(
                                    "workspace-instructions-reload-{workspace_id}"
                                )),
                                "Reload",
                                Some("modals/settings/refresh.svg"),
                                ButtonVariant::Ghost,
                                state.loading || state.saving || !self.rpc_available(cx),
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.reload_workspace_instructions(
                                        reload_workspace.clone(),
                                        cx,
                                    );
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(hsla(if too_long { p.destructive } else { p.muted }))
                            .child(status_label),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(hsla(p.muted))
                            .child("Order: Global -> Workspace -> Project (CLAUDE.md / AGENTS.md)"),
                    )
                    .when(!display_path.is_empty(), |row| {
                        let tip = full_path.clone();
                        row.child(
                            div()
                                .id(SharedString::from(format!(
                                    "workspace-instructions-path-{workspace_id}"
                                )))
                                .font_family(MODAL_MONO_FONT)
                                .text_size(px(11.0))
                                .text_color(hsla(p.muted))
                                .tooltip(tooltip_text(tip))
                                .child(display_path),
                        )
                    }),
            );

        setting_row(
            p,
            input_id,
            RowSpec::new("Agent instructions")
                .description(
                    "Sent automatically by Ghostex to Claude, Droid, Pi, OMP, and Copilot on next start or resume, and to Codex, Antigravity, ZCode, Kiro, CodeBuddy, and Qoder on new, cleared, or compacted sessions (including phone and remote sessions). Cursor, OpenCode, Hermes, Empryo, and Grok Build do not receive workspace instructions automatically. Backed by a Markdown file (up to 32 KB); never edits your global or repository CLAUDE.md / AGENTS.md files.",
                )
                .wide(),
            None,
            control.into_any_element(),
            cx,
        )
    }
}

fn instructions_input_id(workspace_id: &str) -> SharedString {
    SharedString::from(format!("workspace-agent-instructions-{workspace_id}"))
}

fn shorten_home_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if let Some(home) = std::env::var_os("HOME").map(|value| value.to_string_lossy().into_owned())
        && !home.is_empty()
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_string()
}

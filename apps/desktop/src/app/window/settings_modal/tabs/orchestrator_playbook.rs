//! The Orchestrator playbook editor nested in Settings: the row and Edit button of Settings > Chat
//! (the playbook for all projects) and of a project on the Projects page (that project's own,
//! which covers its worktrees), each with a switch that turns an edited copy off without losing
//! it. Both read and save through gxserver (`/api/readCoordinatorPlaybook`,
//! `/api/setCoordinatorPlaybook`).
//!
//! CDXC:Coordinators 2026-10-19 WHY: the editor is a dialog nested in the Settings window, not an app modal of its own: opening another app modal replaces the open one (`open_native_app_modal`), which would close Settings under the reader. The user's decision is on server/src/coordinators/playbook.rs.
use super::super::super::native_modal_kit::*;
use super::super::fields::{
    ButtonVariant, SettingsPage, settings_button, settings_dialog_sheet,
    settings_dialog_sheet_close_shown, settings_textarea, switch_control,
};
use super::super::palette::SettingsPalette;
use super::super::store::store_gxserver_rpc;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, div, px,
};
use gpui_component::h_flex;
use gpui_component::input::{InputEvent, TextareaState};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
/// How long Reset to Default stays armed for its confirming click.
const RESET_ARM_TIMEOUT: Duration = Duration::from_secs(4);
const RESET: &str = "Reset to Default";
const RESET_CONFIRM: &str = "Click again to reset";

/// One scope of the playbook as gxserver answers it: all projects (`None`) or a project's own.
#[derive(Clone, Debug, Default)]
pub(crate) struct PlaybookScope {
    /// An edited copy is stored for this scope.
    pub(crate) edited: bool,
    pub(crate) text: String,
    pub(crate) enabled: bool,
    /// What Reset to Default goes back to (Ghostex's playbook, or a project's all-projects one).
    pub(crate) default_text: String,
    /// The default changed since the copy was saved.
    pub(crate) default_changed: bool,
    /// The project whose copy this is (a worktree's parent).
    pub(crate) owner_name: String,
    pub(crate) is_worktree: bool,
}

impl PlaybookScope {
    fn from_view(view: &Value, project: bool) -> Self {
        let scope = if project {
            &view["project"]
        } else {
            &view["allProjects"]
        };
        let text = |key: &str| {
            scope
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            edited: scope["edited"] == true,
            text: text("text"),
            enabled: scope["enabled"] != false,
            default_text: text("defaultText"),
            default_changed: scope["defaultChanged"] == true,
            owner_name: text("name"),
            is_worktree: scope["isWorktree"] == true,
        }
    }

    /// The text the editor opens with: the edited copy, else what the scope uses now.
    fn editor_text(&self) -> &str {
        if self.edited {
            &self.text
        } else {
            &self.default_text
        }
    }

    /// An edited copy is stored and on.
    pub(crate) fn in_use(&self) -> bool {
        self.edited && self.enabled
    }
}

/// The open editor.
pub(crate) struct PlaybookSheet {
    focus: FocusHandle,
    /// The project as the page asked for it (`None`: all projects).
    project_id: Option<String>,
    /// Made by the first render after the text arrived (setting a field's text needs a window).
    input: Option<(Entity<TextareaState>, Subscription)>,
    /// Text for the field, applied at the next render: the loaded playbook, or the default after
    /// a reset.
    pending_text: Option<String>,
    saving: bool,
    error: Option<String>,
    /// Reset to Default was clicked once and waits for its confirming click.
    reset_armed: bool,
    /// Bumped on every arming, so an older timeout cannot disarm a newer one.
    reset_generation: u64,
}

/// Per page: the scopes read so far (keyed by project id, `None` for all projects) and the open
/// editor.
#[derive(Default)]
pub(crate) struct OrchestratorPlaybookState {
    scopes: HashMap<Option<String>, PlaybookScope>,
    requested: HashSet<Option<String>>,
    sheet: Option<PlaybookSheet>,
    draft: String,
}

/// A Settings page with the editor.
pub(crate) trait HasOrchestratorPlaybook: SettingsPage + Sized {
    fn orchestrator_playbook(&mut self) -> &mut OrchestratorPlaybookState;
}

fn scope_params(project_id: Option<&str>) -> Value {
    match project_id {
        Some(id) => json!({ "projectId": id }),
        None => json!({}),
    }
}

/// The scope, reading it from gxserver the first time it is asked for.
pub(crate) fn playbook_scope<V: HasOrchestratorPlaybook>(
    page: &mut V,
    project_id: Option<&str>,
    cx: &mut Context<V>,
) -> Option<PlaybookScope> {
    let key = project_id.map(str::to_string);
    let state = page.orchestrator_playbook();
    if let Some(scope) = state.scopes.get(&key) {
        return Some(scope.clone());
    }
    if state.requested.insert(key.clone()) {
        read_scope(page, key, cx);
    }
    None
}

fn read_scope<V: HasOrchestratorPlaybook>(
    page: &mut V,
    project_id: Option<String>,
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    if !store.read(cx).request().gxserver_rpc_available {
        return;
    }
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/readCoordinatorPlaybook",
        scope_params(project_id.as_deref()),
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                if let Ok(view) = result {
                    remember_view(page, &view, project_id.as_deref());
                    cx.notify();
                }
            });
        },
        cx,
    );
}

/// Stores what a read or a save answered: the scope asked for, and the one for all projects. A
/// change to the all-projects copy changes every project's default, so their reads are dropped.
fn remember_view<V: HasOrchestratorPlaybook>(page: &mut V, view: &Value, project_id: Option<&str>) {
    let state = page.orchestrator_playbook();
    let all = PlaybookScope::from_view(view, false);
    let all_changed = state.scopes.get(&None).is_some_and(|known| {
        known.edited != all.edited || known.enabled != all.enabled || known.text != all.text
    });
    if all_changed {
        state.scopes.retain(|key, _| key.is_none());
        state.requested.retain(|key| key.is_none());
    }
    state.scopes.insert(None, all);
    if let Some(project_id) = project_id {
        state.scopes.insert(
            Some(project_id.to_string()),
            PlaybookScope::from_view(view, true),
        );
    }
}

/// Opens the editor on the scope's playbook (read fresh, so another client's save shows).
pub(crate) fn open_playbook_sheet<V: HasOrchestratorPlaybook>(
    page: &mut V,
    project_id: Option<String>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let focus = cx.focus_handle();
    focus.focus(window, cx);
    {
        let state = page.orchestrator_playbook();
        state.draft.clear();
        state.sheet = Some(PlaybookSheet {
            focus,
            project_id: project_id.clone(),
            input: None,
            pending_text: None,
            saving: false,
            error: None,
            reset_armed: false,
            reset_generation: 0,
        });
    }
    let store = page.settings_store().clone();
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/readCoordinatorPlaybook",
        scope_params(project_id.as_deref()),
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                match result {
                    Ok(view) => {
                        remember_view(page, &view, project_id.as_deref());
                        let text = PlaybookScope::from_view(&view, project_id.is_some())
                            .editor_text()
                            .to_string();
                        let state = page.orchestrator_playbook();
                        state.draft = text.clone();
                        if let Some(sheet) = state.sheet.as_mut() {
                            sheet.pending_text = Some(text);
                        }
                    }
                    Err(message) => {
                        if let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() {
                            sheet.error = Some(message);
                        }
                    }
                }
                cx.notify();
            });
        },
        cx,
    );
    cx.notify();
}

fn close_sheet<V: HasOrchestratorPlaybook>(page: &mut V, cx: &mut Context<V>) {
    page.orchestrator_playbook().sheet = None;
    cx.notify();
}

/// Sends a change of the open scope (`text`, `reset` or `enabled`), keeping the editor open until
/// gxserver answers. `close` closes it on success; a reset stays open on the default text.
fn send_change<V: HasOrchestratorPlaybook>(
    page: &mut V,
    mut params: Value,
    close: bool,
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() else {
        return;
    };
    if sheet.saving {
        return;
    }
    sheet.saving = true;
    sheet.error = None;
    let project_id = sheet.project_id.clone();
    if let Some(project_id) = &project_id {
        params["projectId"] = json!(project_id);
    }
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/setCoordinatorPlaybook",
        params,
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                match result {
                    Ok(view) => {
                        remember_view(page, &view, project_id.as_deref());
                        let text = PlaybookScope::from_view(&view, project_id.is_some())
                            .editor_text()
                            .to_string();
                        let state = page.orchestrator_playbook();
                        if close {
                            state.sheet = None;
                        } else {
                            state.draft = text.clone();
                            if let Some(sheet) = state.sheet.as_mut() {
                                sheet.saving = false;
                                sheet.pending_text = Some(text);
                            }
                        }
                    }
                    Err(message) => {
                        if let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() {
                            sheet.saving = false;
                            sheet.error = Some(message);
                        }
                    }
                }
                cx.notify();
            });
        },
        cx,
    );
    cx.notify();
}

fn save_sheet<V: HasOrchestratorPlaybook>(page: &mut V, cx: &mut Context<V>) {
    let text = page.orchestrator_playbook().draft.trim().to_string();
    send_change(page, json!({ "text": text }), true, cx);
}

/// The first click arms Reset to Default; the confirming one removes the edited copy. Leaving the
/// button or waiting disarms it.
fn press_reset<V: HasOrchestratorPlaybook>(page: &mut V, cx: &mut Context<V>) {
    let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() else {
        return;
    };
    if sheet.reset_armed {
        sheet.reset_armed = false;
        send_change(page, json!({ "reset": true }), false, cx);
        return;
    }
    sheet.reset_armed = true;
    sheet.reset_generation += 1;
    let generation = sheet.reset_generation;
    cx.spawn(async move |page, cx| {
        cx.background_executor().timer(RESET_ARM_TIMEOUT).await;
        let _ = page.update(cx, |page, cx| disarm_reset(page, Some(generation), cx));
    })
    .detach();
    cx.notify();
}

fn disarm_reset<V: HasOrchestratorPlaybook>(
    page: &mut V,
    generation: Option<u64>,
    cx: &mut Context<V>,
) {
    if let Some(sheet) = page.orchestrator_playbook().sheet.as_mut()
        && sheet.reset_armed
        && generation.is_none_or(|armed_at| armed_at == sheet.reset_generation)
    {
        sheet.reset_armed = false;
        cx.notify();
    }
}

/// Whether the preview binary's state opens the editor (and shows its X at rest, since the
/// preview cannot hover): `…orchestrator-playbook…-edit`, or `…-armed` with Reset armed.
pub(crate) fn preview_opens_editor(state: &str) -> bool {
    state.contains("orchestrator-playbook")
        && (state.ends_with("-edit") || state.ends_with("-armed"))
}

/// The preview binary's `…-armed` state: Reset to Default waiting for its confirming click.
pub(crate) fn preview_arm_reset<V: HasOrchestratorPlaybook>(page: &mut V) {
    if let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() {
        sheet.reset_armed = true;
    }
}

/// Turns the scope's edited copy on or off from its row.
fn set_enabled<V: HasOrchestratorPlaybook>(
    page: &mut V,
    project_id: Option<String>,
    enabled: bool,
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    let mut params = scope_params(project_id.as_deref());
    params["enabled"] = json!(enabled);
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/setCoordinatorPlaybook",
        params,
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                if let Ok(view) = result {
                    remember_view(page, &view, project_id.as_deref());
                }
                cx.notify();
            });
        },
        cx,
    );
}

/// Applies text waiting for the field: makes the field the first time.
fn apply_pending_text<V: HasOrchestratorPlaybook>(
    page: &mut V,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() else {
        return;
    };
    let Some(text) = sheet.pending_text.take() else {
        return;
    };
    if let Some((input, _)) = &sheet.input {
        let input = input.clone();
        input.update(cx, |input, cx| input.set_value(text, window, cx));
        return;
    }
    let rows = playbook_field_rows(window);
    let input = cx.new(|cx| {
        // The playbook is long: the field keeps a fixed height that fits the window and scrolls.
        TextareaState::new(window, cx)
            .auto_grow(rows, rows)
            .default_value(text)
    });
    let subscription = cx.subscribe_in(
        &input,
        window,
        |page: &mut V, input, event: &InputEvent, _window, cx| {
            if matches!(event, InputEvent::Change) {
                page.orchestrator_playbook().draft = input.read(cx).value().to_string();
                cx.notify();
            }
        },
    );
    input.update(cx, |input, cx| input.focus(window, cx));
    if let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() {
        sheet.input = Some((input, subscription));
    }
}

/// Rows of 20px that fit the sheet in this window beside its header, notes and footer (about
/// 300px together), between 8 and 30.
fn playbook_field_rows(window: &Window) -> usize {
    let height = f32::from(window.viewport_size().height) * 0.92;
    (((height - 300.0) / 20.0).floor() as usize).clamp(8, 30)
}

fn note(p: &SettingsPalette, text: String) -> AnyElement {
    div()
        .text_size(px(12.0))
        .line_height(px(17.0))
        .text_color(hsla(p.muted))
        .child(text)
        .into_any_element()
}

/// The editor, while it is open.
pub(crate) fn render_playbook_sheet<V: HasOrchestratorPlaybook>(
    page: &mut V,
    p: &SettingsPalette,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let p = &p.nested_dialog();
    apply_pending_text(page, window, cx);
    let state = page.orchestrator_playbook();
    let sheet = state.sheet.as_ref()?;
    let focus = sheet.focus.clone();
    let input = sheet.input.as_ref().map(|(input, _)| input.clone());
    let (loaded, saving, armed) = (input.is_some(), sheet.saving, sheet.reset_armed);
    let error = sheet.error.clone();
    let project_id = sheet.project_id.clone();
    let scope = state.scopes.get(&project_id).cloned().unwrap_or_default();
    let draft_matches_default = state.draft.trim() == scope.default_text.trim();
    let name = if scope.owner_name.trim().is_empty() {
        "this project".to_string()
    } else {
        scope.owner_name.clone()
    };
    let (title, description, default_name) = match &project_id {
        None => (
            "Coordinator playbook".to_string(),
            "How every coordinator works: the ghostex-orchestrator skill's text. Coordinators, existing ones too, follow an edited playbook from their next guide read. A project can have its own in Settings > Projects.".to_string(),
            "Ghostex's playbook",
        ),
        Some(_) => (
            format!("Coordinator playbook for {name}"),
            format!(
                "Coordinators in {name} and all of its worktrees follow this instead of the playbook for all projects.{}",
                if scope.is_worktree {
                    " This is a worktree, so this edits its project's playbook."
                } else {
                    ""
                }
            ),
            "The playbook for all projects",
        ),
    };
    let field_height = playbook_field_rows(window) as f32 * 20.0 + 24.0;
    let field = match &input {
        Some(input) => settings_textarea(p, input, field_height, true, saving, window, cx),
        None => div()
            .w_full()
            .h(px(field_height))
            .px(px(12.0))
            .py(px(12.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.hairline))
            .text_color(hsla(p.muted))
            .child(if error.is_some() { "" } else { "Loading…" })
            .into_any_element(),
    };
    let mut body: Vec<AnyElement> = vec![field];
    if scope.edited && !scope.enabled {
        body.push(note(
            p,
            format!(
                "This edited playbook is off: coordinators follow {} until you turn it on with the switch on its row.",
                default_name.to_lowercase()
            ),
        ));
    }
    if scope.default_changed {
        body.push(note(
            p,
            format!(
                "{default_name} changed since this copy was saved. Reset to Default takes the new one; to keep your edits, ask an agent to merge `ghostex coordinator playbook{} --default` into this copy.",
                if project_id.is_some() { " --this-project" } else { "" }
            ),
        ));
    }
    if let Some(error) = error {
        body.push(
            div()
                .text_size(px(12.0))
                .text_color(hsla(p.destructive))
                .child(error)
                .into_any_element(),
        );
    }
    let reset = div()
        .id("orchestrator-playbook-reset-wrap")
        .on_hover(cx.listener(|page: &mut V, hovered: &bool, _window, cx| {
            if !*hovered {
                disarm_reset(page, None, cx);
            }
        }))
        .child(settings_button(
            p,
            "orchestrator-playbook-reset",
            if armed { RESET_CONFIRM } else { RESET },
            None,
            if armed {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Outline
            },
            !loaded || saving || draft_matches_default,
            Some("Already the default playbook.".into()),
            |page: &mut V, _window, cx| press_reset(page, cx),
            cx,
        ));
    let cancel = settings_button(
        p,
        "orchestrator-playbook-cancel",
        "Cancel",
        None,
        ButtonVariant::Outline,
        false,
        None,
        |page: &mut V, _window, cx| close_sheet(page, cx),
        cx,
    );
    let save = settings_button(
        p,
        "orchestrator-playbook-save",
        if saving { "Saving…" } else { "Save" },
        None,
        ButtonVariant::Primary,
        !loaded || saving,
        None,
        |page: &mut V, _window, cx| save_sheet(page, cx),
        cx,
    );
    let footer = h_flex()
        .w_full()
        .gap(px(8.0))
        .child(reset)
        .child(div().flex_1())
        .child(cancel)
        .child(save)
        .into_any_element();
    let max_height = f32::from(window.viewport_size().height) * 0.92;
    // The preview binary's editor states cannot hover, so their X shows at rest.
    let close_shown = page
        .settings_store()
        .read(cx)
        .request()
        .preview_state
        .as_deref()
        .is_some_and(preview_opens_editor);
    let description = Some(div().child(description).into_any_element());
    let on_close = |page: &mut V, _window: &mut Window, cx: &mut Context<V>| close_sheet(page, cx);
    Some(if close_shown {
        settings_dialog_sheet_close_shown(
            p,
            "orchestrator-playbook",
            &focus,
            title,
            description,
            820.0,
            Some(max_height),
            24.0,
            16.0,
            body,
            Some(footer),
            on_close,
            window,
            cx,
        )
    } else {
        settings_dialog_sheet(
            p,
            "orchestrator-playbook",
            &focus,
            title,
            description,
            820.0,
            Some(max_height),
            24.0,
            16.0,
            false,
            body,
            Some(footer),
            on_close,
            window,
            cx,
        )
    })
}

/// A row's control: the scope's state, the switch that turns an edited copy on or off, and Edit.
pub(crate) fn playbook_control<V: HasOrchestratorPlaybook>(
    p: &SettingsPalette,
    id: &'static str,
    project_id: Option<String>,
    scope: Option<&PlaybookScope>,
    rpc_available: bool,
    cx: &mut Context<V>,
) -> AnyElement {
    let state = match scope {
        None => "Loading…".to_string(),
        Some(scope) if !scope.edited && project_id.is_some() => {
            "Uses the playbook for all projects".to_string()
        }
        Some(scope) if !scope.edited => "Ghostex's playbook".to_string(),
        Some(scope) => {
            let mut state = if scope.enabled {
                "Edited".to_string()
            } else {
                "Edited, off".to_string()
            };
            if scope.default_changed {
                state.push_str(" · default changed");
            }
            state
        }
    };
    let edited = scope.is_some_and(|scope| scope.edited);
    let switch_project = project_id.clone();
    let switch = switch_control(
        p,
        format!("{id}-switch"),
        "Use the edited playbook",
        scope.is_some_and(PlaybookScope::in_use),
        !edited || !rpc_available,
        Some("Nothing to turn off: no edited playbook is stored here.".into()),
        move |page: &mut V, enabled, _window, cx| {
            set_enabled(page, switch_project.clone(), enabled, cx)
        },
        cx,
    );
    let edit = settings_button(
        p,
        format!("{id}-edit"),
        "Edit…",
        None,
        ButtonVariant::Outline,
        !rpc_available,
        None,
        move |page: &mut V, window, cx| open_playbook_sheet(page, project_id.clone(), window, cx),
        cx,
    );
    h_flex()
        .gap(px(10.0))
        .child(
            div()
                .max_w(px(220.0))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child(state),
        )
        .child(switch)
        .child(edit)
        .into_any_element()
}

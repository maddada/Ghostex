//! The Playbook overrides editor nested in Settings: the row and Edit button of Settings > Chat,
//! and the small sheet it opens. The overrides are the user's own instructions for every
//! coordinator, read after Ghostex's playbook and ranked above it; Ghostex's playbook itself is
//! never edited. Both read and save through gxserver (`/api/readCoordinatorPlaybook`,
//! `/api/setCoordinatorPlaybook`). While an edited full playbook saved before overrides is still
//! stored, the sheet says so and offers Use Ghostex's Playbook, which saves it as a Markdown file.
//! View playbook beside Edit opens the read-only playbook as coordinators read it
//! (`/api/readCoordinatorGuide`; CDXC:Coordinators 2026-11-08 in
//! window/new_coordinator_playbook.rs).
//!
//! CDXC:Coordinators 2026-10-19 WHY: the editor is a dialog nested in the Settings window, not an app modal of its own: opening another app modal replaces the open one (`open_native_app_modal`), which would close Settings under the reader. The user's decision (2026-11-07, overrides instead of editing) is on server/src/coordinators/playbook.rs.
use super::super::super::native_modal_kit::*;
use super::super::fields::{
    ButtonVariant, SettingsPage, settings_button, settings_dialog_sheet,
    settings_dialog_sheet_close_shown, settings_textarea,
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
use std::time::Duration;

use super::super::super::new_coordinator_modal::{
    EDIT_OVERRIDES, OVERRIDES_DESCRIPTION, OVERRIDES_HINT, OVERRIDES_PLACEHOLDER,
    PLAYBOOK_VIEW_DESCRIPTION, PLAYBOOK_VIEW_TITLE, PlaybookViewColors, VIEW_PLAYBOOK,
    guide_ghostex_part, old_copies_note, old_copy_places, overrides_summary, playbook_view_body,
};

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
/// How long Use Ghostex's Playbook stays armed for its confirming click.
const SWITCH_ARM_TIMEOUT: Duration = Duration::from_secs(4);
const USE_GHOSTEX: &str = "Use Ghostex's Playbook";
const USE_GHOSTEX_CONFIRM: &str = "Click again to switch";

/// The overrides as gxserver answers them, with the old full copies still stored.
#[derive(Clone, Debug, Default)]
pub(crate) struct PlaybookOverrides {
    pub(crate) text: String,
    /// Where each old full copy in use applies: "all projects" or a project's name.
    pub(crate) old_copies: Vec<String>,
}

impl PlaybookOverrides {
    fn from_view(view: &Value) -> Self {
        Self {
            text: view["overrides"]["text"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string(),
            old_copies: old_copy_places(view),
        }
    }
}

/// The open editor.
pub(crate) struct PlaybookSheet {
    focus: FocusHandle,
    /// Made by the first render after the text arrived (setting a field's text needs a window).
    input: Option<(Entity<TextareaState>, Subscription)>,
    /// Text for the field, applied at the next render.
    pending_text: Option<String>,
    saving: bool,
    error: Option<String>,
    /// Where Use Ghostex's Playbook saved the old copies.
    saved_note: Option<String>,
    /// Use Ghostex's Playbook was clicked once and waits for its confirming click.
    switch_armed: bool,
    /// Bumped on every arming, so an older timeout cannot disarm a newer one.
    switch_generation: u64,
}

/// The open read-only playbook.
pub(crate) struct PlaybookViewSheet {
    focus: FocusHandle,
    /// Ghostex's part of the guide once read, or why it could not be.
    guide: Option<Result<String, String>>,
}

/// Per page: the overrides read so far, the open editor and the open playbook view.
#[derive(Default)]
pub(crate) struct OrchestratorPlaybookState {
    known: Option<PlaybookOverrides>,
    requested: bool,
    sheet: Option<PlaybookSheet>,
    view: Option<PlaybookViewSheet>,
    draft: String,
}

/// A Settings page with the editor.
pub(crate) trait HasOrchestratorPlaybook: SettingsPage + Sized {
    fn orchestrator_playbook(&mut self) -> &mut OrchestratorPlaybookState;
}

/// The overrides, reading them from gxserver the first time they are asked for.
pub(crate) fn playbook_overrides<V: HasOrchestratorPlaybook>(
    page: &mut V,
    cx: &mut Context<V>,
) -> Option<PlaybookOverrides> {
    let state = page.orchestrator_playbook();
    if let Some(known) = &state.known {
        return Some(known.clone());
    }
    if !state.requested {
        state.requested = true;
        read_overrides(page, cx);
    }
    None
}

fn read_overrides<V: HasOrchestratorPlaybook>(page: &mut V, cx: &mut Context<V>) {
    let store = page.settings_store().clone();
    if !store.read(cx).request().gxserver_rpc_available {
        return;
    }
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/readCoordinatorPlaybook",
        json!({}),
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                if let Ok(view) = result {
                    page.orchestrator_playbook().known = Some(PlaybookOverrides::from_view(&view));
                    cx.notify();
                }
            });
        },
        cx,
    );
}

/// Opens the editor on the overrides (read fresh, so another client's save shows).
pub(crate) fn open_playbook_sheet<V: HasOrchestratorPlaybook>(
    page: &mut V,
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
            input: None,
            pending_text: None,
            saving: false,
            error: None,
            saved_note: None,
            switch_armed: false,
            switch_generation: 0,
        });
    }
    let store = page.settings_store().clone();
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/readCoordinatorPlaybook",
        json!({}),
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                let state = page.orchestrator_playbook();
                match result {
                    Ok(view) => {
                        let known = PlaybookOverrides::from_view(&view);
                        state.draft = known.text.clone();
                        if let Some(sheet) = state.sheet.as_mut() {
                            sheet.pending_text = Some(known.text.clone());
                        }
                        state.known = Some(known);
                    }
                    Err(message) => {
                        if let Some(sheet) = state.sheet.as_mut() {
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

/// Opens the read-only playbook, reading the guide and the overrides fresh.
pub(crate) fn open_playbook_view<V: HasOrchestratorPlaybook>(
    page: &mut V,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let focus = cx.focus_handle();
    focus.focus(window, cx);
    page.orchestrator_playbook().view = Some(PlaybookViewSheet { focus, guide: None });
    let store = page.settings_store().clone();
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/readCoordinatorGuide",
        json!({}),
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                if let Some(view) = page.orchestrator_playbook().view.as_mut() {
                    view.guide = Some(result.and_then(|answer| {
                        answer["guide"]
                            .as_str()
                            .map(guide_ghostex_part)
                            .ok_or_else(|| "Ghostex could not show its playbook.".to_string())
                    }));
                }
                cx.notify();
            });
        },
        cx,
    );
    read_overrides(page, cx);
    cx.notify();
}

fn close_view<V: HasOrchestratorPlaybook>(page: &mut V, cx: &mut Context<V>) {
    page.orchestrator_playbook().view = None;
    cx.notify();
}

/// The read-only playbook, while it is open.
pub(crate) fn render_playbook_view_sheet<V: HasOrchestratorPlaybook>(
    page: &mut V,
    p: &SettingsPalette,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let p = &p.nested_dialog();
    let state = page.orchestrator_playbook();
    let view = state.view.as_ref()?;
    let focus = view.focus.clone();
    let guide = view.guide.clone();
    let overrides = state
        .known
        .as_ref()
        .map(|known| known.text.clone())
        .unwrap_or_default();
    let colors = PlaybookViewColors {
        light: p.light,
        foreground: p.foreground,
        muted: p.muted,
        panel: p.raised,
        hairline: p.hairline,
        destructive: p.destructive,
    };
    let body = vec![playbook_view_body(
        colors,
        "orchestrator-playbook-view",
        guide.as_ref(),
        &overrides,
    )];
    let close = settings_button(
        p,
        "orchestrator-playbook-view-close",
        "Close",
        None,
        ButtonVariant::Outline,
        false,
        None,
        |page: &mut V, _window, cx| close_view(page, cx),
        cx,
    );
    let edit = settings_button(
        p,
        "orchestrator-playbook-view-edit",
        EDIT_OVERRIDES,
        None,
        ButtonVariant::Primary,
        !page
            .settings_store()
            .read(cx)
            .request()
            .gxserver_rpc_available,
        None,
        |page: &mut V, window, cx| {
            page.orchestrator_playbook().view = None;
            open_playbook_sheet(page, window, cx);
        },
        cx,
    );
    let footer = h_flex()
        .w_full()
        .gap(px(8.0))
        .child(div().flex_1())
        .child(close)
        .child(edit)
        .into_any_element();
    let max_height = f32::from(window.viewport_size().height) * 0.92;
    let description = Some(div().child(PLAYBOOK_VIEW_DESCRIPTION).into_any_element());
    let on_close = |page: &mut V, _window: &mut Window, cx: &mut Context<V>| close_view(page, cx);
    // A tall sheet whose body scrolls under a fixed header and footer.
    let body = vec![
        div()
            .id("orchestrator-playbook-view-scroll")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(body)
            .into_any_element(),
    ];
    Some(settings_dialog_sheet(
        p,
        "orchestrator-playbook-view",
        &focus,
        PLAYBOOK_VIEW_TITLE,
        description,
        720.0,
        Some(max_height),
        24.0,
        12.0,
        false,
        body,
        Some(footer),
        on_close,
        window,
        cx,
    ))
}

fn close_sheet<V: HasOrchestratorPlaybook>(page: &mut V, cx: &mut Context<V>) {
    page.orchestrator_playbook().sheet = None;
    cx.notify();
}

/// Sends `params` (`overrides`, or `retireOldCopies`), keeping the editor open until gxserver
/// answers. `close` closes it on success; switching to Ghostex's playbook stays open.
fn send_change<V: HasOrchestratorPlaybook>(
    page: &mut V,
    params: Value,
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
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/setCoordinatorPlaybook",
        params,
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                let state = page.orchestrator_playbook();
                match result {
                    Ok(view) => {
                        state.known = Some(PlaybookOverrides::from_view(&view));
                        if close {
                            state.sheet = None;
                        } else if let Some(sheet) = state.sheet.as_mut() {
                            sheet.saving = false;
                            let saved: Vec<&str> = view["savedFiles"]
                                .as_array()
                                .map(|files| files.iter().filter_map(Value::as_str).collect())
                                .unwrap_or_default();
                            sheet.saved_note = (!saved.is_empty()).then(|| {
                                format!(
                                    "Every coordinator follows Ghostex's playbook now. The old playbook is saved in {}.",
                                    saved.join(", ")
                                )
                            });
                        }
                    }
                    Err(message) => {
                        if let Some(sheet) = state.sheet.as_mut() {
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
    send_change(page, json!({ "overrides": text }), true, cx);
}

/// The first click arms Use Ghostex's Playbook; the confirming one saves the old copies as files
/// and removes them. Leaving the button or waiting disarms it.
fn press_use_ghostex<V: HasOrchestratorPlaybook>(page: &mut V, cx: &mut Context<V>) {
    let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() else {
        return;
    };
    if sheet.switch_armed {
        sheet.switch_armed = false;
        send_change(page, json!({ "retireOldCopies": true }), false, cx);
        return;
    }
    sheet.switch_armed = true;
    sheet.switch_generation += 1;
    let generation = sheet.switch_generation;
    cx.spawn(async move |page, cx| {
        cx.background_executor().timer(SWITCH_ARM_TIMEOUT).await;
        let _ = page.update(cx, |page, cx| disarm_switch(page, Some(generation), cx));
    })
    .detach();
    cx.notify();
}

fn disarm_switch<V: HasOrchestratorPlaybook>(
    page: &mut V,
    generation: Option<u64>,
    cx: &mut Context<V>,
) {
    if let Some(sheet) = page.orchestrator_playbook().sheet.as_mut()
        && sheet.switch_armed
        && generation.is_none_or(|armed_at| armed_at == sheet.switch_generation)
    {
        sheet.switch_armed = false;
        cx.notify();
    }
}

/// Whether the preview binary's state opens the editor (and shows its X at rest, since the
/// preview cannot hover): `…orchestrator-playbook…-edit`, or `…-armed` with Use Ghostex's
/// Playbook armed.
pub(crate) fn preview_opens_editor(state: &str) -> bool {
    state.contains("orchestrator-playbook")
        && (state.ends_with("-edit") || state.ends_with("-armed"))
}

/// The preview binary's `…-armed` state: Use Ghostex's Playbook waiting for its confirming click.
pub(crate) fn preview_arm_switch<V: HasOrchestratorPlaybook>(page: &mut V) {
    if let Some(sheet) = page.orchestrator_playbook().sheet.as_mut() {
        sheet.switch_armed = true;
    }
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
    let input = cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(8, 16)
            .placeholder(OVERRIDES_PLACEHOLDER)
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
    let (loaded, saving, armed) = (input.is_some(), sheet.saving, sheet.switch_armed);
    let error = sheet.error.clone();
    let saved_note = sheet.saved_note.clone();
    let known = state.known.clone().unwrap_or_default();
    let field_height = 8.0 * 20.0 + 24.0;
    let field = match &input {
        Some(input) => settings_textarea(p, input, field_height, false, saving, window, cx),
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
    let mut body: Vec<AnyElement> = vec![field, note(p, OVERRIDES_HINT.to_string())];
    if let Some(saved) = saved_note {
        body.push(note(p, saved));
    } else if let Some(old) = old_copies_note(&known.old_copies) {
        body.push(note(p, old));
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
    let mut footer = h_flex().w_full().gap(px(8.0));
    if !known.old_copies.is_empty() {
        footer = footer.child(
            div()
                .id("orchestrator-playbook-use-ghostex-wrap")
                .on_hover(cx.listener(|page: &mut V, hovered: &bool, _window, cx| {
                    if !*hovered {
                        disarm_switch(page, None, cx);
                    }
                }))
                .child(settings_button(
                    p,
                    "orchestrator-playbook-use-ghostex",
                    if armed {
                        USE_GHOSTEX_CONFIRM
                    } else {
                        USE_GHOSTEX
                    },
                    None,
                    if armed {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Outline
                    },
                    !loaded || saving,
                    None,
                    |page: &mut V, _window, cx| press_use_ghostex(page, cx),
                    cx,
                )),
        );
    }
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
    let footer = footer
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
    let title = "Playbook overrides";
    let description = Some(div().child(OVERRIDES_DESCRIPTION).into_any_element());
    let on_close = |page: &mut V, _window: &mut Window, cx: &mut Context<V>| close_sheet(page, cx);
    Some(if close_shown {
        settings_dialog_sheet_close_shown(
            p,
            "orchestrator-playbook",
            &focus,
            title,
            description,
            560.0,
            Some(max_height),
            24.0,
            12.0,
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
            560.0,
            Some(max_height),
            24.0,
            12.0,
            false,
            body,
            Some(footer),
            on_close,
            window,
            cx,
        )
    })
}

/// The row's control: the overrides' first line (or none) and Edit.
pub(crate) fn playbook_control<V: HasOrchestratorPlaybook>(
    p: &SettingsPalette,
    id: &'static str,
    known: Option<&PlaybookOverrides>,
    rpc_available: bool,
    cx: &mut Context<V>,
) -> AnyElement {
    let state = match known {
        None => "Loading…".to_string(),
        Some(known) if !known.old_copies.is_empty() => "Old edited playbook in use".to_string(),
        Some(known) if known.text.is_empty() => "None".to_string(),
        Some(known) => overrides_summary(&known.text),
    };
    let edit = settings_button(
        p,
        format!("{id}-edit"),
        "Edit…",
        None,
        ButtonVariant::Outline,
        !rpc_available,
        None,
        move |page: &mut V, window, cx| open_playbook_sheet(page, window, cx),
        cx,
    );
    let view = settings_button(
        p,
        format!("{id}-view"),
        format!("{VIEW_PLAYBOOK}…"),
        None,
        ButtonVariant::Outline,
        !rpc_available,
        None,
        move |page: &mut V, window, cx| open_playbook_view(page, window, cx),
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
        .child(view)
        .child(edit)
        .into_any_element()
}

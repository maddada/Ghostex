//! `HomeSecretFoldersField`: the folders and files agents Ghostex starts in the home folder are
//! kept out of (`homeSecretFolders`, server/src/agents/home_secret_folders.rs), as removable rows
//! and a box with Add for a `~/` or absolute path.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::controls::{ButtonVariant, settings_button, settings_icon_button};
use super::row::{PageAction, RowSpec, setting_row};
use super::text::settings_text_input;
use super::{FieldStates, SettingsPage, icon};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

const KEY: &str = "homeSecretFolders";
const INVALID_FOLDER_MESSAGE: &str = "Enter a path that starts with ~/ or /, such as ~/.ssh.";

pub(crate) struct HomeSecretFoldersState {
    input: Entity<InputState>,
    error: Option<String>,
}

/// A usable entry: `~/`-relative or absolute, without a trailing separator.
fn parse_folder(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches(['/', '\\']);
    let absolute = trimmed.starts_with('/')
        || (trimmed.len() > 2
            && trimmed.as_bytes()[1] == b':'
            && trimmed.as_bytes()[0].is_ascii_alphabetic());
    (trimmed.starts_with("~/") || absolute).then(|| trimmed.to_string())
}

fn read_folders(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|folders| {
            folders
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn save_folders<V: SettingsPage>(page: &mut V, folders: Vec<String>, cx: &mut Context<V>) {
    let store = page.settings_store().clone();
    store.update(cx, |store, cx| {
        store.update_setting(KEY, json!(folders), cx)
    });
}

fn add_folder<V: SettingsPage>(page: &mut V, window: &mut Window, cx: &mut Context<V>) {
    let Some(state) = page.field_states().home_secret_folders.as_mut() else {
        return;
    };
    let text = state.input.read(cx).value().to_string();
    let Some(folder) = parse_folder(&text) else {
        state.error = Some(INVALID_FOLDER_MESSAGE.to_string());
        cx.notify();
        return;
    };
    state.error = None;
    let input = state.input.clone();
    input.update(cx, |input, cx| input.set_value("", window, cx));
    let store = page.settings_store().clone();
    let mut folders = read_folders(&store.read(cx).value(KEY));
    if !folders.contains(&folder) {
        folders.push(folder);
    }
    save_folders(page, folders, cx);
}

impl FieldStates {
    fn home_secret_folders_input<V: SettingsPage>(
        page: &mut V,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Entity<InputState> {
        if page.field_states().home_secret_folders.is_none() {
            let input = cx.new(|cx| {
                InputState::new(window, cx).placeholder("e.g. ~/.kube or ~/Documents/Taxes")
            });
            let subscription = cx.subscribe_in(
                &input,
                window,
                |page: &mut V, _input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        if let Some(state) = page.field_states().home_secret_folders.as_mut()
                            && state.error.take().is_some()
                        {
                            cx.notify();
                        }
                    }
                    InputEvent::PressEnter { .. } => add_folder(page, window, cx),
                    _ => {}
                },
            );
            let states = page.field_states();
            states.subscriptions.push(subscription);
            states.home_secret_folders = Some(HomeSecretFoldersState { input, error: None });
        }
        page.field_states()
            .home_secret_folders
            .as_ref()
            .map(|state| state.input.clone())
            .expect("home secret folders state")
    }
}

/// The list as a wide row.
pub(crate) fn home_secret_folders_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    value: &Value,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let input = FieldStates::home_secret_folders_input(page, window, cx);
    let error = page
        .field_states()
        .home_secret_folders
        .as_ref()
        .and_then(|state| state.error.clone());
    let folders = read_folders(value);
    let list: AnyElement = if folders.is_empty() {
        div()
            .text_size(px(14.0))
            .text_color(hsla(p.muted))
            .child("No folders. Agents in your home folder can read everything you can.")
            .into_any_element()
    } else {
        v_flex()
            .gap(px(8.0))
            .children(folders.iter().enumerate().map(|(index, folder)| {
                let remaining: Vec<String> = folders
                    .iter()
                    .filter(|other| *other != folder)
                    .cloned()
                    .collect();
                h_flex()
                    .min_h(px(36.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(css_fade(p.hairline, 0.7)))
                    .bg(hsla(css_fade(p.raised, 0.4)))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(14.0))
                            .child(folder.clone()),
                    )
                    .child(settings_icon_button(
                        p,
                        SharedString::from(format!("home-secret-folder-remove-{index}")),
                        icon::TRASH,
                        14.0,
                        24.0,
                        ButtonVariant::Ghost,
                        None,
                        false,
                        move |page: &mut V, _window, cx| {
                            save_folders(page, remaining.clone(), cx);
                        },
                        cx,
                    ))
            }))
            .into_any_element()
    };
    let empty_input = input.read(cx).value().trim().is_empty();
    let control = v_flex()
        .w_full()
        .gap(px(12.0))
        .child(list)
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .child(settings_text_input(p, &input, None, false, window, cx))
                .child(settings_button(
                    p,
                    "home-secret-folder-add",
                    "Add",
                    Some(icon::PLUS),
                    ButtonVariant::Outline,
                    empty_input,
                    Some("Enter a folder first.".into()),
                    |page: &mut V, window, cx| add_folder(page, window, cx),
                    cx,
                )),
        )
        .children(error.map(|error| {
            div()
                .text_size(px(14.0))
                .text_color(hsla(p.destructive))
                .child(error)
        }))
        .into_any_element();
    setting_row(p, KEY, spec.wide(), on_reset, control, cx)
}

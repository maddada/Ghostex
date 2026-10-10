//! `ActionSettingsEditor`: the one editor section that replaces both action lists while an action
//! is added or edited (Type, Text, Icon, Command or URL, the terminal switches and links, Show on
//! the project's sidebar row, then Delete / Cancel / Save).
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::super::fields::{
    ButtonVariant, CONTROL_LANE_WIDTH, FieldStates, RowSpec, SELECT_WIDTH, SearchableList,
    card_inset, command_icon_picker_field, setting_row, settings_button, settings_icon_button,
    settings_section, settings_select, settings_textarea, switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::ActionsTab;
use super::model::{
    ActionType, CommandButton, CommandLink, CommandScope, draft_title, save_message, title_key,
};
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Entity, IntoElement, ParentElement as _, SharedString, Styled as _,
    Window, div, px,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};

const ICON_PLUS: &str = "modals/settings/plus.svg";
const ICON_TRASH: &str = "modals/settings/trash.svg";
const ICON_X: &str = "modals/settings/x.svg";
/// The browser's default `<input size=20>` width the link URL field keeps (it is not stretched).
const LINK_URL_WIDTH: f32 = 196.0;

/// `SettingsCommandEditorState` with the editor's `useState` draft.
pub(crate) struct ActionEditor {
    pub(crate) scope: CommandScope,
    pub(crate) locked: Option<ActionType>,
    pub(crate) command_id: Option<String>,
    pub(crate) action_type: ActionType,
    pub(crate) close_terminal_on_exit: bool,
    pub(crate) command: String,
    pub(crate) icon: String,
    pub(crate) links: Vec<CommandLink>,
    pub(crate) name: String,
    pub(crate) play_completion_sound: bool,
    pub(crate) show_on_project_row: bool,
    pub(crate) url: String,
    /// `autoFocus` on the Text field, applied on the first render.
    pub(crate) focus_name: bool,
    /// A link row added by Add link autofocuses its URL field.
    pub(crate) focus_link: Option<usize>,
    name_placeholder: Option<ActionType>,
}

/// `DEFAULT_BROWSER_ACTION_URL`.
pub(crate) fn default_browser_url() -> String {
    let url = settings_catalog().text(module::SIDEBAR_COMMANDS, "DEFAULT_BROWSER_ACTION_URL");
    if url.is_empty() {
        "http://localhost:5173".to_string()
    } else {
        url
    }
}

impl ActionEditor {
    /// `createSettingsCommandDraft(actionType)` with the type locked, as the Add buttons open it.
    pub(crate) fn new_action(scope: CommandScope, action_type: ActionType) -> Self {
        Self {
            scope,
            locked: Some(action_type),
            command_id: None,
            action_type,
            close_terminal_on_exit: false,
            command: String::new(),
            icon: super::super::super::fields::DEFAULT_COMMAND_ICON.to_string(),
            links: Vec::new(),
            name: String::new(),
            play_completion_sound: action_type == ActionType::Terminal,
            show_on_project_row: false,
            url: if action_type == ActionType::Browser {
                default_browser_url()
            } else {
                String::new()
            },
            focus_name: true,
            focus_link: None,
            name_placeholder: None,
        }
    }

    /// `createSettingsCommandDraftFromButton(command)`.
    pub(crate) fn edit(scope: CommandScope, command: &CommandButton) -> Self {
        let url = command.url.clone().unwrap_or_else(|| {
            if command.action_type == ActionType::Browser {
                default_browser_url()
            } else {
                String::new()
            }
        });
        Self {
            scope,
            locked: None,
            command_id: Some(command.command_id.clone()),
            action_type: command.action_type,
            close_terminal_on_exit: command.close_terminal_on_exit,
            command: command.command.clone().unwrap_or_default(),
            icon: command
                .icon
                .clone()
                .filter(|icon| super::super::super::fields::is_command_icon(icon))
                .unwrap_or_else(|| super::super::super::fields::DEFAULT_COMMAND_ICON.to_string()),
            links: command.links.clone().unwrap_or_default(),
            name: command.name.clone(),
            play_completion_sound: command.play_completion_sound,
            show_on_project_row: command.show_on_project_row,
            url,
            focus_name: true,
            focus_link: None,
            name_placeholder: None,
        }
    }

    fn title(&self) -> String {
        draft_title(self.action_type, &self.command, &self.name, &self.url)
    }

    /// The target the Save button needs (`targetValue`).
    fn target_empty(&self) -> bool {
        match self.action_type {
            ActionType::Browser => self.url.trim().is_empty(),
            ActionType::Terminal => self.command.trim().is_empty(),
        }
    }
}

/// `hasDuplicateTitle`.
///
/// CDXC:CommandPane 2026-05-16-15:08 SEE-ALSO: one action title per project, because command-pane reuse identifies a pane by its action title (packages/core-ui/settings-modal/tabs/actions.tsx (deleted 2026-10-01)).
fn has_duplicate_title(editor: &ActionEditor, existing: &[CommandButton]) -> bool {
    let key = title_key(&editor.title());
    existing.iter().any(|command| {
        Some(command.command_id.as_str()) != editor.command_id.as_deref()
            && title_key(&command.settings_title()) == key
    })
}

fn editor_mut(tab: &mut ActionsTab) -> Option<&mut ActionEditor> {
    tab.editor.as_mut()
}

fn icon_picker(tab: &mut ActionsTab) -> &mut SearchableList {
    &mut tab.icon_picker
}

/// The Text field: `SettingsInput` in the control lane, with the destructive edge and its 3px
/// ring while the title duplicates another action's (`aria-invalid`). The ring is its own border
/// around the field so the field's translucent fill does not tint with it.
fn name_input(
    p: &SettingsPalette,
    state: &Entity<InputState>,
    invalid: bool,
    window: &Window,
    cx: &gpui::App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    let border = if invalid {
        if p.light {
            p.destructive
        } else {
            css_fade(p.destructive, 0.5)
        }
    } else if focused {
        p.focus_border
    } else {
        p.hairline
    };
    let ring = css_fade(p.destructive, if p.light { 0.2 } else { 0.4 });
    div()
        .relative()
        .w(px(CONTROL_LANE_WIDTH))
        .max_w_full()
        .h(px(32.0))
        .when(invalid, |this| {
            this.child(
                div()
                    .absolute()
                    .top(px(-3.0))
                    .left(px(-3.0))
                    .right(px(-3.0))
                    .bottom(px(-3.0))
                    .rounded(px(MODAL_RADIUS_CONTROL + 3.0))
                    .border(px(3.0))
                    .border_color(hsla(ring)),
            )
        })
        .child(
            div()
                .size_full()
                .px(px(12.0))
                .flex()
                .items_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(border))
                .bg(hsla(p.input_background()))
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(state)
                            .with_size(ComponentSize::Small)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .px(px(0.0))
                            .py(px(0.0))
                            .text_size(px(14.0))
                            .text_color(hsla(p.foreground)),
                    ),
                ),
        )
        .into_any_element()
}

fn type_options() -> Vec<SettingOption> {
    vec![
        SettingOption {
            label: "Terminal".into(),
            value: "terminal".into(),
        },
        SettingOption {
            label: "Browser".into(),
            value: "browser".into(),
        },
    ]
}

fn link_target_options() -> Vec<SettingOption> {
    vec![
        SettingOption {
            label: "Integrated browser".into(),
            value: "integrated".into(),
        },
        SettingOption {
            label: "External browser".into(),
            value: "external".into(),
        },
    ]
}

/// A switch row of the editor bound to one draft flag.
fn switch_row(
    p: &SettingsPalette,
    id: &'static str,
    label: &'static str,
    checked: bool,
    set: fn(&mut ActionEditor, bool),
    cx: &mut Context<ActionsTab>,
) -> AnyElement {
    let control = switch_control(
        p,
        SharedString::from(format!("{id}-switch")),
        label,
        checked,
        false,
        None,
        move |tab: &mut ActionsTab, next, _window, cx| {
            if let Some(editor) = editor_mut(tab) {
                set(editor, next);
            }
            cx.notify();
        },
        cx,
    );
    setting_row(p, id, RowSpec::new(label), None, control, cx)
}

/// The editor's text inputs, kept in the page's field states and fed from the draft.
fn draft_input(
    tab: &mut ActionsTab,
    id: &str,
    value: &str,
    placeholder: &str,
    set: impl Fn(&mut ActionEditor, String) + 'static,
    window: &mut Window,
    cx: &mut Context<ActionsTab>,
) -> Entity<InputState> {
    FieldStates::text_state(
        tab,
        &SharedString::from(id.to_string()),
        value,
        Some(placeholder),
        move |tab: &mut ActionsTab, text, _window, cx| {
            if let Some(editor) = editor_mut(tab) {
                set(editor, text);
                cx.notify();
            }
        },
        window,
        cx,
    )
}

/// The editor section (`SettingsSection title='Action' | 'Global Action'`).
pub(crate) fn render_editor(
    tab: &mut ActionsTab,
    p: &SettingsPalette,
    existing: &[CommandButton],
    window: &mut Window,
    cx: &mut Context<ActionsTab>,
) -> Option<AnyElement> {
    let (
        scope,
        locked,
        action_type,
        name,
        command,
        url,
        icon,
        links,
        flags,
        focus_name,
        focus_link,
    ) = {
        let editor = tab.editor.as_ref()?;
        (
            editor.scope,
            editor.locked,
            editor.action_type,
            editor.name.clone(),
            editor.command.clone(),
            editor.url.clone(),
            editor.icon.clone(),
            editor.links.clone(),
            (
                editor.close_terminal_on_exit,
                editor.play_completion_sound,
                editor.show_on_project_row,
            ),
            editor.focus_name,
            editor.focus_link,
        )
    };
    let duplicate = tab
        .editor
        .as_ref()
        .is_some_and(|editor| has_duplicate_title(editor, existing));
    let target_empty = tab.editor.as_ref().is_some_and(ActionEditor::target_empty);
    let has_command_id = tab
        .editor
        .as_ref()
        .is_some_and(|editor| editor.command_id.is_some());
    let mut rows: Vec<AnyElement> = Vec::new();

    if locked.is_none() {
        let select = settings_select(
            tab,
            p,
            "action-editor-type",
            &type_options(),
            action_type.id(),
            Some(SELECT_WIDTH),
            false,
            None,
            |tab: &mut ActionsTab, value, _window, cx| {
                if let Some(editor) = editor_mut(tab) {
                    let next = ActionType::parse(&value);
                    editor.action_type = next;
                    if next == ActionType::Browser && editor.url.trim().is_empty() {
                        editor.url = default_browser_url();
                    }
                }
                cx.notify();
            },
            window,
            cx,
        );
        rows.push(setting_row(
            p,
            "action-editor-type-row",
            RowSpec::new("Type"),
            None,
            select,
            cx,
        ));
    }

    let placeholder = if action_type == ActionType::Browser {
        "Docs"
    } else {
        "Dev"
    };
    let name_state = draft_input(
        tab,
        "action-editor-name",
        &name,
        placeholder,
        |editor, text| editor.name = text,
        window,
        cx,
    );
    if let Some(editor) = tab.editor.as_mut() {
        if editor.name_placeholder != Some(action_type) {
            editor.name_placeholder = Some(action_type);
            name_state.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx)
            });
        }
        if focus_name {
            editor.focus_name = false;
            name_state.update(cx, |input, cx| input.focus(window, cx));
        }
    }
    let mut name_spec = RowSpec::new("Text");
    if duplicate {
        name_spec = name_spec.description("Another action already uses this title.");
    }
    rows.push(setting_row(
        p,
        "action-editor-name-row",
        name_spec,
        None,
        name_input(p, &name_state, duplicate, window, cx),
        cx,
    ));

    rows.push(card_inset(command_icon_picker_field(
        tab,
        p,
        "action-editor-icon",
        icon_picker,
        &icon,
        |tab: &mut ActionsTab, icon, _window, cx| {
            if let Some(editor) = editor_mut(tab) {
                editor.icon = icon.to_string();
            }
            cx.notify();
        },
        window,
        cx,
    )));

    if action_type == ActionType::Browser {
        let default_url = default_browser_url();
        let state = FieldStates::textarea_state(
            tab,
            &SharedString::from("action-editor-url"),
            &url,
            Some(&default_url),
            (1, 12),
            |tab: &mut ActionsTab, text, _window, cx| {
                if let Some(editor) = editor_mut(tab) {
                    editor.url = text;
                }
                cx.notify();
            },
            window,
            cx,
        );
        rows.push(setting_row(
            p,
            "action-editor-url-row",
            RowSpec::new("URL").wide(),
            None,
            settings_textarea(p, &state, 64.0, false, false, window, cx),
            cx,
        ));
    } else {
        let state = FieldStates::textarea_state(
            tab,
            &SharedString::from("action-editor-command"),
            &command,
            Some("vp dev"),
            (1, 12),
            |tab: &mut ActionsTab, text, _window, cx| {
                if let Some(editor) = editor_mut(tab) {
                    editor.command = text;
                }
                cx.notify();
            },
            window,
            cx,
        );
        rows.push(setting_row(
            p,
            "action-editor-command-row",
            RowSpec::new("Command").wide(),
            None,
            settings_textarea(p, &state, 64.0, false, false, window, cx),
            cx,
        ));
        rows.push(switch_row(
            p,
            "action-editor-close-terminal",
            "Close terminal after the command finishes",
            flags.0,
            |editor, next| editor.close_terminal_on_exit = next,
            cx,
        ));
        rows.push(switch_row(
            p,
            "action-editor-sound",
            "Play completion sound",
            flags.1,
            |editor, next| editor.play_completion_sound = next,
            cx,
        ));
        rows.push(links_row(tab, p, &links, focus_link, window, cx));
    }
    // CDXC:Projects 2026-08-01 SEE-ALSO: both terminal and browser actions can opt into the project's sidebar row.
    rows.push(switch_row(
        p,
        "action-editor-project-row",
        "Show on the project's sidebar row",
        flags.2,
        |editor, next| editor.show_on_project_row = next,
        cx,
    ));
    rows.push(card_inset(footer(
        p,
        scope,
        has_command_id,
        duplicate,
        target_empty,
        action_type,
        cx,
    )));

    let title = match scope {
        CommandScope::Global => "Global Action",
        CommandScope::Project => "Action",
    };
    settings_section(p, title, None, None, rows).map(IntoElement::into_any_element)
}

/// `Open links when this action runs`: a URL field, an Integrated/External select and a remove
/// button per link, right-aligned, then Add link.
///
/// CDXC:Projects 2026-07-31-12:00 SEE-ALSO: terminal actions can open saved links whenever they run (packages/core-ui/settings-modal/tabs/actions.tsx (deleted 2026-10-01)).
fn links_row(
    tab: &mut ActionsTab,
    p: &SettingsPalette,
    links: &[CommandLink],
    focus_link: Option<usize>,
    window: &mut Window,
    cx: &mut Context<ActionsTab>,
) -> AnyElement {
    let default_url = default_browser_url();
    let mut link_rows: Vec<AnyElement> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let url_state = draft_input(
            tab,
            &format!("action-editor-link-url-{index}"),
            &link.url,
            &default_url,
            move |editor, text| {
                if let Some(link) = editor.links.get_mut(index) {
                    link.url = text;
                }
            },
            window,
            cx,
        );
        if focus_link == Some(index) {
            if let Some(editor) = tab.editor.as_mut() {
                editor.focus_link = None;
            }
            url_state.update(cx, |input, cx| input.focus(window, cx));
        }
        let focused = url_state.read(cx).focus_handle(cx).is_focused(window);
        let url_field = div()
            .w(px(LINK_URL_WIDTH))
            .min_w_0()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
            .bg(hsla(p.input_background()))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&url_state)
                        .with_size(ComponentSize::Small)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .w_full()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(14.0))
                        .text_color(hsla(p.foreground)),
                ),
            );
        let target = settings_select(
            tab,
            p,
            format!("action-editor-link-target-{index}"),
            &link_target_options(),
            &link.target,
            Some(SELECT_WIDTH),
            false,
            None,
            move |tab: &mut ActionsTab, value, _window, cx| {
                if let Some(link) = editor_mut(tab).and_then(|editor| editor.links.get_mut(index)) {
                    link.target = if value == "external" {
                        "external".to_string()
                    } else {
                        "integrated".to_string()
                    };
                }
                cx.notify();
            },
            window,
            cx,
        );
        let remove = settings_icon_button(
            p,
            SharedString::from(format!("action-editor-link-remove-{index}")),
            ICON_X,
            16.0,
            28.0,
            ButtonVariant::Ghost,
            None,
            false,
            move |tab: &mut ActionsTab, _window, cx| {
                if let Some(editor) = editor_mut(tab)
                    && index < editor.links.len()
                {
                    editor.links.remove(index);
                }
                cx.notify();
            },
            cx,
        );
        link_rows.push(
            h_flex()
                .w_full()
                .justify_end()
                .items_center()
                .gap(px(8.0))
                .child(url_field)
                .child(target)
                .child(remove)
                .into_any_element(),
        );
    }
    let add = settings_button(
        p,
        "action-editor-add-link",
        "Add link",
        Some(ICON_PLUS),
        ButtonVariant::Outline,
        false,
        None,
        |tab: &mut ActionsTab, _window, cx| {
            if let Some(editor) = editor_mut(tab) {
                editor.links.push(CommandLink {
                    target: "integrated".to_string(),
                    url: String::new(),
                });
                editor.focus_link = Some(editor.links.len() - 1);
            }
            cx.notify();
        },
        cx,
    );
    let control = v_flex()
        .w_full()
        .gap(px(8.0))
        .when(!link_rows.is_empty(), |this| {
            this.child(v_flex().w_full().gap(px(8.0)).children(link_rows))
        })
        .child(h_flex().child(add))
        .into_any_element();
    setting_row(
        p,
        "action-editor-links-row",
        RowSpec::new("Open links when this action runs")
            .description("Open saved URLs, like your dev server's localhost address, alongside the command. Each link can open in the project's integrated browser or your default browser.")
            .wide(),
        None,
        control,
        cx,
    )
}

/// Delete (on a saved action) on the left, Cancel and Save on the right.
///
/// CDXC:AgentLauncher 2026-06-18-10:11 SEE-ALSO: Delete on the edit surface uses the same delete message as a row's trash button, for default and custom actions alike.
fn footer(
    p: &SettingsPalette,
    scope: CommandScope,
    has_command_id: bool,
    duplicate: bool,
    target_empty: bool,
    action_type: ActionType,
    cx: &mut Context<ActionsTab>,
) -> AnyElement {
    let delete = if has_command_id {
        settings_button(
            p,
            "action-editor-delete",
            "Delete",
            Some(ICON_TRASH),
            ButtonVariant::Destructive,
            false,
            None,
            move |tab: &mut ActionsTab, _window, cx| {
                let Some(command_id) = tab
                    .editor
                    .as_ref()
                    .and_then(|editor| editor.command_id.clone())
                else {
                    return;
                };
                tab.delete_command(scope, &command_id, cx);
            },
            cx,
        )
    } else {
        div().into_any_element()
    };
    let reason = if duplicate {
        "Choose a unique action title."
    } else if action_type == ActionType::Browser {
        "Enter a URL first."
    } else {
        "Enter a command first."
    };
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(delete)
        .child(
            h_flex()
                .items_center()
                .justify_end()
                .gap(px(12.0))
                .child(settings_button(
                    p,
                    "action-editor-cancel",
                    "Cancel",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    |tab: &mut ActionsTab, _window, cx| {
                        tab.editor = None;
                        cx.notify();
                    },
                    cx,
                ))
                .child(settings_button(
                    p,
                    "action-editor-save",
                    "Save",
                    None,
                    ButtonVariant::Default,
                    target_empty || duplicate,
                    Some(reason.into()),
                    move |tab: &mut ActionsTab, _window, cx| {
                        let Some(editor) = tab.editor.as_ref() else {
                            return;
                        };
                        let message = save_message(
                            scope,
                            editor.action_type,
                            editor.close_terminal_on_exit,
                            &editor.command,
                            editor.command_id.as_deref(),
                            &editor.icon,
                            &editor.links,
                            &editor.name,
                            editor.play_completion_sound,
                            editor.show_on_project_row,
                            &editor.url,
                        );
                        tab.post(message, cx);
                        tab.editor = None;
                        cx.notify();
                    },
                    cx,
                )),
        )
        .into_any_element()
}

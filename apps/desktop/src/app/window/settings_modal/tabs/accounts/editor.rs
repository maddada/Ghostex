//! `AccountEditor` (accounts/manager.tsx (deleted 2026-10-01)): an expanded saved account's name, indicator, session
//! icon preview, automatic switching, slot swap, and its actions (Remove, Sign in again), with the
//! reconnect flow and the remove confirmation under them.
//!
//! CDXC:AgentProviders 2026-10-01 DECISION:
//! User: drop the Save changes button and apply edits right away. The name and indicator save when
//! their field loses focus or takes Enter (and when the editor closes), automatic switching saves
//! when it is flipped, so Cancel, which only discarded unsaved edits, is gone too. Removing an
//! account and swapping slots still ask for their own confirmation click. Every button here is the
//! 32px outlined Settings button, the height and corners of the editor's fields.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    FieldStates, ListItemStatus, RowSpec, SizedButtonSize, SizedButtonVariant, setting_row,
    settings_list_item, settings_select, settings_sized_button, settings_text_input,
    switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::AccountsTab;
use super::data::{Account, normalize_account_indicator_input, provider_label};
use super::manager::account_inset;
use super::widgets::{account_logo, account_text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

/// The editor's own state (its React `useState`s).
#[derive(Clone, Debug)]
pub(crate) struct EditorDraft {
    pub(crate) name: String,
    pub(crate) indicator: String,
    pub(crate) eligible: bool,
    pub(crate) remove: bool,
    pub(crate) reconnect: bool,
    pub(crate) swap_target: String,
}

impl AccountsTab {
    fn editor_draft(&mut self, account: &Account) -> EditorDraft {
        self.editors
            .entry(account.id())
            .or_insert_with(|| EditorDraft {
                name: account.name(),
                indicator: account.indicator(),
                eligible: account.eligible(),
                remove: false,
                reconnect: account.status() != "ready",
                swap_target: String::new(),
            })
            .clone()
    }

    fn update_editor(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut EditorDraft),
    ) {
        if let Some(draft) = self.editors.get_mut(id) {
            apply(draft);
            cx.notify();
        }
    }

    /// Saves the editor's name, indicator and automatic switching when they differ from the saved
    /// account. A blank name goes back to the saved one instead.
    pub(crate) fn commit_editor(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(draft) = self.editors.get(id).cloned() else {
            return;
        };
        let Some(account) = self.client.read(cx).data.as_ref().and_then(|data| {
            data.accounts()
                .into_iter()
                .find(|account| account.id() == id)
        }) else {
            return;
        };
        if draft.name.trim().is_empty() {
            self.update_editor(id, cx, |draft| draft.name = account.name());
            return;
        }
        if draft.name.trim() == account.name()
            && draft.indicator == account.indicator()
            && draft.eligible == account.eligible()
        {
            return;
        }
        self.account_request(
            json!({
                "operation": "update",
                "id": id,
                "name": draft.name.trim(),
                "color": account.color(),
                "eligible": draft.eligible,
                "indicator": draft.indicator,
            }),
            None,
            cx,
        );
    }

    /// Saves the editor when `input` loses focus or takes Enter (subscribed once per input).
    fn commit_on_blur(
        &mut self,
        input_id: &SharedString,
        input: &Entity<InputState>,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.commit_inputs.insert(input_id.clone()) {
            return;
        }
        let id = id.to_string();
        let subscription = cx.subscribe_in(
            input,
            window,
            move |page: &mut Self, _input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    page.commit_editor(&id, cx);
                }
            },
        );
        self.fields.subscriptions.push(subscription);
    }

    /// Closes the editor and forgets its draft (`close()`).
    fn close_editor(&mut self, id: &str, cx: &mut Context<Self>) {
        self.editors.remove(id);
        if self.editing.as_deref() == Some(id) {
            self.editing = None;
        }
        cx.notify();
    }

    pub(crate) fn render_account_editor(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        accounts: &[Account],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        let busy = self.client.read(cx).busy;
        let id = account.id();
        let draft = self.editor_draft(account);
        let mut rows: Vec<AnyElement> = Vec::new();
        // Account name.
        let name_id = SharedString::from(format!("account-editor-{id}-name"));
        let name_input = FieldStates::text_state(
            self,
            &name_id,
            &draft.name,
            None,
            {
                let id = id.clone();
                move |page: &mut Self, text, _window, cx| {
                    let text: String = text.chars().take(80).collect();
                    page.update_editor(&id, cx, |draft| draft.name = text)
                }
            },
            window,
            cx,
        );
        self.commit_on_blur(&name_id, &name_input, &id, window, cx);
        let masked = hide && draft.name.contains('@');
        super::widgets::sync_masked(
            &mut self.masked_inputs,
            &name_id,
            &name_input,
            masked,
            window,
            cx,
        );
        rows.push(setting_row(
            p,
            SharedString::from(format!("account-editor-{id}-name-row")),
            RowSpec::new("Account name"),
            None,
            settings_text_input(p, &name_input, Some(224.0), false, window, cx),
            cx,
        ));
        // Account indicator.
        let indicator_id = SharedString::from(format!("account-editor-{id}-indicator"));
        let indicator_input = FieldStates::text_state(
            self,
            &indicator_id,
            &draft.indicator,
            Some(&account.selector()),
            {
                let id = id.clone();
                let indicator_id = indicator_id.clone();
                move |page: &mut Self, text, window, cx| {
                    let normalized = normalize_account_indicator_input(&text);
                    page.update_editor(&id, cx, |draft| draft.indicator = normalized.clone());
                    if normalized != text
                        && let Some(state) = page.fields.texts.get(&indicator_id)
                    {
                        let input = state.input.clone();
                        input.update(cx, |input, cx| input.set_value(normalized, window, cx));
                    }
                }
            },
            window,
            cx,
        );
        self.commit_on_blur(&indicator_id, &indicator_input, &id, window, cx);
        rows.push(setting_row(
            p,
            SharedString::from(format!("account-editor-{id}-indicator-row")),
            RowSpec::new("Account indicator").description(format!(
                "Up to two letters or numbers, such as cw for Claude work. Enter - to hide the indicator, or leave blank to use slot {}.",
                account.selector()
            )),
            None,
            settings_text_input(p, &indicator_input, Some(96.0), false, window, cx),
            cx,
        ));
        // Session icon preview.
        let indicator = if draft.indicator.is_empty() {
            account.selector()
        } else {
            draft.indicator.clone()
        };
        rows.push(setting_row(
            p,
            SharedString::from(format!("account-editor-{id}-preview")),
            RowSpec::new("Session icon preview")
                .description("How sessions using this account appear in the sidebar."),
            None,
            h_flex()
                .items_center()
                .gap(px(8.0))
                .child(account_mark(p, &account.provider(), &indicator))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(hsla(p.muted))
                        .child(format!(
                            "{} · {}",
                            provider_label(&account.provider()),
                            account_text(&draft.name, hide)
                        )),
                )
                .into_any_element(),
            cx,
        ));
        // Available for automatic switching.
        rows.push(setting_row(
            p,
            SharedString::from(format!("account-editor-{id}-eligible")),
            RowSpec::new("Available for automatic switching").description(
                "When automatic account switching is enabled, Ghostex may switch to this account when another reaches its usage limit.",
            ),
            None,
            switch_control(
                p,
                SharedString::from(format!("account-editor-{id}-eligible-switch")),
                "Available for automatic switching",
                draft.eligible,
                false,
                None,
                {
                    let id = id.clone();
                    move |page: &mut Self, next, _window, cx| {
                        page.update_editor(&id, cx, |draft| draft.eligible = next);
                        page.commit_editor(&id, cx);
                    }
                },
                cx,
            ),
            cx,
        ));
        // Swap slot.
        if accounts.len() > 1 {
            let others: Vec<&Account> = accounts.iter().filter(|other| other.id() != id).collect();
            let options: Vec<SettingOption> = others
                .iter()
                .map(|other| SettingOption {
                    label: account_text(
                        &format!("Slot {} {}", other.selector(), other.name()),
                        hide,
                    ),
                    value: other.id(),
                })
                .collect();
            let select = settings_select(
                self,
                p,
                SharedString::from(format!("account-editor-{id}-swap")),
                &options,
                &draft.swap_target,
                Some(super::super::super::fields::SELECT_WIDTH),
                false,
                None,
                {
                    let id = id.clone();
                    move |page: &mut Self, next, _window, cx| {
                        page.update_editor(&id, cx, |draft| draft.swap_target = next)
                    }
                },
                window,
                cx,
            );
            let select = if draft.swap_target.is_empty() {
                placeholder_select(p, select)
            } else {
                select
            };
            let swap = {
                let id = id.clone();
                let target = draft.swap_target.clone();
                settings_sized_button(
                    p,
                    SharedString::from(format!("account-editor-{id}-swap-button")),
                    "Swap slots",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Default,
                    busy || draft.swap_target.is_empty(),
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.account_request(
                            json!({ "operation": "swapSlots", "firstId": id, "secondId": target }),
                            None,
                            cx,
                        )
                    },
                    cx,
                )
            };
            rows.push(setting_row(
                p,
                SharedString::from(format!("account-editor-{id}-swap-row")),
                RowSpec::new(format!("Swap slot {} with", account.selector())).description(
                    if account.provider() == "claude" {
                        "Stop sessions using either account before swapping Claude slots."
                    } else {
                        "Exchange the slot numbers of two saved accounts."
                    },
                ),
                None,
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(select)
                    .child(swap)
                    .into_any_element(),
                cx,
            ));
        }
        // Account actions.
        let sessions = account.session_count();
        let sessions_note = format!(
            "{sessions} session{} use this account.",
            if sessions == 1 { "" } else { "s" }
        );
        let outline = |label: &'static str,
                       action: Box<dyn Fn(&mut Self, &mut Window, &mut Context<Self>)>,
                       cx: &mut Context<Self>| {
            settings_sized_button(
                p,
                SharedString::from(format!("account-editor-{id}-{label}")),
                label,
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Default,
                false,
                None,
                move |page: &mut Self, window, cx| action(page, window, cx),
                cx,
            )
        };
        let remove = {
            let id = id.clone();
            outline(
                "Remove",
                Box::new(move |page, _window, cx| {
                    page.update_editor(&id, cx, |draft| draft.remove = !draft.remove)
                }),
                cx,
            )
        };
        let sign_in = {
            let id = id.clone();
            outline(
                "Sign in again",
                Box::new(move |page, _window, cx| {
                    page.update_editor(&id, cx, |draft| draft.reconnect = !draft.reconnect)
                }),
                cx,
            )
        };
        rows.push(settings_list_item(
            p,
            None,
            None,
            "Account actions",
            Some(div().child(sessions_note).into_any_element()),
            Some(
                h_flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.0))
                    .child(remove)
                    .child(sign_in)
                    .into_any_element(),
            ),
        ));
        if draft.reconnect {
            let flow = self.render_connect_flow(
                p,
                format!("reconnect:{id}"),
                if account.provider() == "claude" {
                    "claude"
                } else {
                    "codex"
                },
                Some(account.clone()),
                None,
                window,
                cx,
            );
            rows.push(account_inset(flow));
        }
        if draft.remove {
            let remove_id = id.clone();
            let confirm = settings_sized_button(
                p,
                SharedString::from(format!("account-editor-{id}-remove-confirm")),
                "Remove from Ghostex",
                None,
                None,
                SizedButtonVariant::Destructive,
                SizedButtonSize::Default,
                busy,
                None,
                move |page: &mut Self, _window, cx| {
                    let this = cx.weak_entity();
                    let close_id = remove_id.clone();
                    page.account_request(
                        json!({ "operation": "remove", "id": remove_id }),
                        Some(Box::new(move |ok, cx| {
                            if ok {
                                let _ =
                                    this.update(cx, |page, cx| page.close_editor(&close_id, cx));
                            }
                        })),
                        cx,
                    );
                },
                cx,
            );
            rows.push(settings_list_item(
                p,
                Some(ListItemStatus::Warning),
                None,
                format!("Remove {} from Ghostex?", account_text(&account.name(), hide)),
                Some(
                    div()
                        .whitespace_normal()
                        .child("The saved helper login and shared conversations remain. Sessions using this account need a different account before their next resume.")
                        .into_any_element(),
                ),
                Some(confirm),
            ));
        }
        let hairline = hsla(p.hairline);
        v_flex()
            .w_full()
            .children(rows.into_iter().enumerate().map(|(index, row)| {
                div()
                    .w_full()
                    .when(index > 0, |this| this.border_t_1().border_color(hairline))
                    // `.settings-list-panel > * { padding-inline: 0 }`: the rows drop their inset.
                    .child(div().mx(px(-20.0)).child(row))
            }))
            .into_any_element()
    }
}

/// `.gx-account-mark` with its indicator: the provider logo at 30% under a 9.9px label (Codex in
/// `#7db8fb`); a `-` indicator shows the plain logo.
///
/// CDXC:AgentProviders 2026-09-09 DECISION:
/// User wants the same account-label font centered over a larger provider icon as its background. Account icons always use original provider colors; labels and adjacent usage figures share the chat indicator's monospace font. Claude keeps its label color; Codex uses #7db8fb. The background icon is 19.2px (20% smaller) and the label is 9.9px (10% bigger). This replaces the label-underneath design. Sidebar icons and account menus have no indicator. Labels accept up to two letters or digits; - hides the label and an empty setting uses the slot number.
fn account_mark(p: &SettingsPalette, provider: &str, indicator: &str) -> AnyElement {
    if indicator == "-" {
        return account_logo(p, provider, 21.0);
    }
    div()
        .relative()
        .size(px(19.2))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .inset_0()
                .opacity(0.3)
                .child(account_logo(p, provider, 19.2)),
        )
        .child(
            div()
                .font_family(MODAL_MONO_FONT)
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(9.9))
                .line_height(px(9.9))
                .text_color(hsla(if provider == "codex" {
                    gpui::rgb(0x7db8fb)
                } else {
                    p.muted
                }))
                .child(indicator.to_string()),
        )
        .into_any_element()
}

/// A select with no value shows its placeholder ("Choose an account") in the muted tone; the
/// label sits over the empty trigger and lets clicks through to it.
fn placeholder_select(p: &SettingsPalette, select: AnyElement) -> AnyElement {
    div()
        .relative()
        .child(select)
        .child(
            div()
                .absolute()
                .left(px(13.0))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .text_size(px(14.0))
                .text_color(hsla(p.muted))
                .child("Choose an account"),
        )
        .into_any_element()
}

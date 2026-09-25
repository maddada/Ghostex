/*!
The Switch Account panel: More actions > Switch Account, and the Switch account
button on sign-in and usage-limit notices. A port of `SessionAccountsPanel`
(packages/core-ui/accounts/session-panel.tsx, styled by `.gx-account-submenu` in
accounts.css) drawn from the core's `accountPanel` projection
(packages/gx-chat-core/src/menus/native_accounts.rs), which already
carries the shared copy and Hide emails masking.

The panel is laid out as a list of blocks so the popup window can be sized to the
same heights the renderer draws, the way the context panel is.
*/

use super::super::{appearance::ChatAppearance, new_session_welcome::brand_logo_color};
use super::window::ChatOptionMenuPanel;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Bounds, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, TextRun, Window,
    WindowTextSystem, div, px, relative, rgb, svg,
};
use serde_json::{Value, json};

/// `.gx-account-submenu`: `min(420px, 100vw - 24px)`.
pub(in crate::app::native_chat) const ACCOUNT_PANEL_WIDTH: f32 = 420.0;
/// Inside the menu's own 6px, so content sits 14px in like `.gx-account-submenu .gx-account-panel`.
const PADDING: f32 = 8.0;
/// The menu's padding and border around the panel row.
const MENU_CHROME: f32 = 6.0 * 2.0 + 2.0;

const PARAGRAPH: (f32, f32) = (11.0, 17.6);
const STRONG: (f32, f32) = (12.0, 19.2);

enum Action {
    /// Close the whole menu and run the command.
    Close(Value),
    /// Run the command and keep the panel open.
    Keep(Value),
    /// Drop the local Customize state and clear the session override.
    UseDefaults,
}

enum Block {
    Gap(f32),
    Header {
        busy: bool,
    },
    Alert(String),
    Text {
        text: String,
        strong: bool,
        dim: bool,
    },
    Recovery {
        reason: String,
        next: Option<String>,
        busy: bool,
    },
    Current {
        identity: Value,
        name: String,
        email: Option<String>,
    },
    Meter {
        label: String,
        percent: f32,
        left: String,
        right: Option<String>,
    },
    Heading(String),
    Account {
        row: Value,
        busy: bool,
    },
    Divider,
    PolicyHeading {
        customize: bool,
    },
    Toggle {
        label: &'static str,
        checked: bool,
        disabled: bool,
        next: Value,
    },
    Legend {
        dim: bool,
    },
    Segmented {
        policy: Value,
        disabled: bool,
    },
    Label {
        dim: bool,
    },
    Select {
        policy: Value,
        priorities: Value,
        label: String,
        disabled: bool,
    },
    Button {
        id: &'static str,
        label: &'static str,
        icon: Option<&'static str>,
        outline: bool,
        disabled: bool,
        action: Action,
    },
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

fn policy_command(policy: Value) -> Value {
    json!({"type":"accounts","request":{"operation":"sessionPolicy","policy":policy}})
}

fn with(policy: &Value, key: &str, value: Value) -> Value {
    let mut next = policy.clone();
    next[key] = value;
    next
}

fn blocks(panel: &Value, customize: bool) -> Vec<Block> {
    use Block::*;
    let mut out = Vec::new();
    if panel["kind"] == "noAccounts" {
        out.push(Text {
            text: "Current CLI login".into(),
            strong: true,
            dim: false,
        });
        out.push(Gap(7.0));
        out.push(Text {
            text: "Add your account to see usage and reset times in Ghostex, even if you only use one account.".into(),
            strong: false,
            dim: false,
        });
        out.push(Gap(7.0));
        out.push(Button {
            id: "account-add",
            label: "Add account",
            icon: None,
            outline: true,
            disabled: false,
            action: Action::Close(json!({"type":"openAccountsSettings"})),
        });
        return out;
    }
    let busy = panel["busy"] == true;
    out.push(Header { busy });
    out.push(Gap(14.0));
    if let Some(error) = panel["error"].as_str() {
        out.push(Alert(error.to_owned()));
        out.push(Gap(12.0));
    }
    if let Some(message) = panel["message"].as_str() {
        out.push(Text {
            text: message.to_owned(),
            strong: false,
            dim: false,
        });
        return out;
    }
    let session = &panel["session"];
    if !session.is_object() {
        return out;
    }
    if session["recovery"].is_object() {
        out.push(Recovery {
            reason: text(&session["recovery"], "reason"),
            next: session["recovery"]["next"].as_str().map(str::to_owned),
            busy,
        });
        out.push(Gap(12.0));
    }
    out.push(Current {
        identity: session["current"].clone(),
        name: text(session, "name"),
        email: session["email"].as_str().map(str::to_owned),
    });
    out.push(Gap(12.0));
    if let Some(error) = session["usageError"].as_str() {
        out.push(Text {
            text: error.to_owned(),
            strong: false,
            dim: false,
        });
        out.push(Gap(7.0));
    }
    let usage = session["usage"].as_array().cloned().unwrap_or_default();
    if usage.is_empty() {
        out.push(Text {
            text: "Usage is unavailable for this login.".into(),
            strong: false,
            dim: false,
        });
        out.push(Gap(7.0));
    } else {
        out.push(Gap(8.0));
        for (index, window) in usage.iter().enumerate() {
            if index > 0 {
                out.push(Gap(18.0));
            }
            out.push(Meter {
                label: text(window, "label"),
                percent: window["percent"].as_f64().unwrap_or(0.0) as f32,
                left: text(window, "reset"),
                right: None,
            });
        }
        out.push(Gap(20.0));
    }
    if session["context"].is_object() {
        let context = &session["context"];
        out.push(Meter {
            label: "Conversation context".into(),
            percent: context["percent"].as_f64().unwrap_or(0.0) as f32,
            left: text(context, "value"),
            right: Some(text(context, "tokens")),
        });
        out.push(Gap(20.0));
    }
    out.push(Heading(text(session, "switchHeading")));
    out.push(Gap(4.0));
    for row in session["others"].as_array().into_iter().flatten() {
        out.push(Account {
            row: row.clone(),
            busy,
        });
        out.push(Gap(4.0));
    }
    out.push(Gap(3.0));
    out.push(Text {
        text: "Switching resumes the same conversation. Stop an active turn before switching."
            .into(),
        strong: false,
        dim: false,
    });
    out.push(Gap(18.0));
    out.push(Divider);
    out.push(Gap(18.0));
    let policy_state = &session["policy"];
    let custom = policy_state["custom"] == true;
    out.push(PolicyHeading {
        customize: !customize && !custom,
    });
    out.push(Gap(4.0));
    out.push(Text {
        text: text(policy_state, "summary"),
        strong: false,
        dim: false,
    });
    if customize || custom {
        let policy = &policy_state["value"];
        let enabled = policy["enabled"] == true;
        let dim = busy || !enabled;
        out.push(Gap(14.0));
        out.push(Toggle {
            label: "Continue automatically",
            checked: enabled,
            disabled: busy,
            next: with(policy, "enabled", json!(!enabled)),
        });
        out.push(Gap(18.0));
        out.push(Legend { dim });
        out.push(Gap(9.0));
        out.push(Segmented {
            policy: policy.clone(),
            disabled: dim,
        });
        out.push(Gap(7.0));
        out.push(Text {
            text: text(policy_state, "atLimitDescription"),
            strong: false,
            dim,
        });
        if policy["atLimit"] == "switch" {
            out.push(Gap(15.0));
            out.push(Label { dim });
            out.push(Gap(7.0));
            out.push(Select {
                policy: policy.clone(),
                priorities: policy_state["priorities"].clone(),
                label: text(policy_state, "priorityLabel"),
                disabled: dim,
            });
            out.push(Gap(15.0));
        } else {
            out.push(Gap(14.0));
        }
        let retry = policy["retryErrors"] == true;
        out.push(Toggle {
            label: "Recover from temporary errors",
            checked: retry,
            disabled: dim,
            next: with(policy, "retryErrors", json!(!retry)),
        });
        out.push(Gap(14.0));
        out.push(Text {
            text: text(policy_state, "retryDescription"),
            strong: false,
            dim,
        });
        out.push(Gap(15.0));
        out.push(Button {
            id: "account-policy-reset",
            label: "Use session defaults",
            icon: Some("titlebar/arrow-back-up.svg"),
            outline: false,
            disabled: busy,
            action: Action::UseDefaults,
        });
    }
    out
}

struct Measure {
    system: WindowTextSystem,
    font: gpui::Font,
    scale: f32,
}

impl Measure {
    fn lines(&self, text: &str, (size, line): (f32, f32), width: f32) -> anyhow::Result<f32> {
        if text.is_empty() {
            return Ok(line);
        }
        let run = TextRun {
            len: text.len(),
            font: self.font.clone(),
            ..Default::default()
        };
        let lines = self.system.shape_text(
            text.to_owned().into(),
            px(size * self.scale),
            &[run],
            Some(px(width.max(1.0) * self.scale)),
            None,
        )?;
        Ok(lines
            .iter()
            .map(|wrapped| f32::from(wrapped.size(px(line * self.scale)).height) / self.scale)
            .sum::<f32>()
            .max(line))
    }
}

impl Block {
    fn height(&self, width: f32, measure: &Measure) -> anyhow::Result<f32> {
        use Block::*;
        Ok(match self {
            Gap(height) => *height,
            Header { .. } | PolicyHeading { .. } | Button { .. } => 28.0,
            Alert(message) => 26.0 + measure.lines(message, STRONG, width - 26.0)?,
            Text { text, strong, .. } => {
                measure.lines(text, if *strong { STRONG } else { PARAGRAPH }, width)?
            }
            Recovery { reason, next, .. } => {
                let next = match next {
                    Some(next) => 12.0 + measure.lines(next, PARAGRAPH, width - 26.0)?,
                    None => 0.0,
                };
                26.0 + measure.lines(reason, STRONG, width - 26.0)? + next + 12.0 + 28.0
            }
            Current { email, .. } => (STRONG.1
                + if email.is_some() {
                    4.0 + PARAGRAPH.1
                } else {
                    0.0
                })
            .max(26.0),
            Meter { .. } => 52.0,
            Heading(_) => 20.0,
            Account { row, .. } => {
                24.0 + (18.0_f32 + if row["detail"].is_string() { 16.0 } else { 0.0 }).max(26.0)
            }
            Divider => 1.0,
            Toggle { .. } => 20.0,
            Legend { .. } => 16.0,
            Label { .. } => 18.0,
            Segmented { .. } | Select { .. } => 32.0,
        })
    }
}

/// The panel row's height for a menu window `width` wide (both unscaled).
pub(super) fn height(
    panel: &Value,
    width: f32,
    customize: bool,
    appearance: &ChatAppearance,
    cx: &App,
) -> anyhow::Result<f32> {
    let measure = Measure {
        system: WindowTextSystem::new(cx.text_system().clone()),
        font: gpui::font(appearance.font.clone()),
        scale: appearance.scale,
    };
    let inner = width - MENU_CHROME - PADDING * 2.0;
    let mut total = PADDING * 2.0;
    for block in blocks(panel, customize) {
        total += block.height(inner, &measure)?;
    }
    Ok(total)
}

struct Colors {
    foreground: Hsla,
    muted: Hsla,
    border: Hsla,
    hover: Hsla,
    track: Hsla,
    fill: Hsla,
    switch_on: Hsla,
    switch_off: Hsla,
    thumb: Hsla,
    segment_track: Hsla,
    segment_text: Hsla,
    segment_pressed: Hsla,
    segment_pressed_text: Hsla,
}

impl Colors {
    fn new(appearance: &ChatAppearance) -> Self {
        let light = appearance.light;
        let foreground: Hsla = rgb(if light { 0x292929 } else { 0xfcfcfc }).into();
        let background = appearance.menu_surface();
        Self {
            foreground,
            muted: appearance.muted,
            border: gpui::rgba(if light { 0x0000001f } else { 0xffffff1f }).into(),
            hover: foreground.opacity(0.05),
            // The React chat's `--gx-account-meter-*` on light chat (chat.css until 2026-09-25);
            // accounts.css defaults on dark.
            track: if light {
                foreground.opacity(0.12)
            } else {
                rgb(0x363636).into()
            },
            fill: if light {
                rgb(0x7a7a7a).into()
            } else {
                rgb(0xb9b9b9).into()
            },
            switch_on: rgb(if light { 0x262626 } else { 0xe5e5e5 }).into(),
            switch_off: if light {
                gpui::Hsla::from(rgb(0x000000)).opacity(0.144)
            } else {
                gpui::Hsla::from(rgb(0xffffff)).opacity(0.135)
            },
            thumb: background,
            segment_track: rgb(if light { 0xededed } else { 0x202020 }).into(),
            segment_text: rgb(if light { 0x525252 } else { 0xb8b8b8 }).into(),
            segment_pressed: rgb(if light { 0xffffff } else { 0x363636 }).into(),
            segment_pressed_text: rgb(if light { 0x262626 } else { 0xf5f5f5 }).into(),
        }
    }
}

fn identity(value: &Value, appearance: &ChatAppearance) -> AnyElement {
    let s = appearance.scale;
    let provider = value["provider"].as_str().unwrap_or("claude");
    let figures: Vec<String> = value["figures"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|figure| figure.as_str().unwrap_or_default().to_owned())
        .collect();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(7.0 * s))
        .min_w(px(62.0 * s))
        .child(
            svg()
                .path(format!("agent-icons/{provider}.svg"))
                .flex_shrink_0()
                .size(px(21.0 * s))
                .text_color(brand_logo_color(provider, appearance)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .font_family("Menlo")
                .text_size(px(10.0 * s))
                .line_height(px(13.0 * s))
                .children(figures.into_iter().enumerate().map(|(index, figure)| {
                    div()
                        .when(index > 0, |this| this.opacity(0.6))
                        .child(figure)
                })),
        )
        .into_any_element()
}

fn icon(path: &'static str, size: f32, color: Hsla) -> gpui::Svg {
    svg()
        .path(path)
        .flex_shrink_0()
        .size(px(size))
        .text_color(color)
}

impl ChatOptionMenuPanel {
    fn account_action(&mut self, action: &Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::Close(command) => {
                let command = command.clone();
                self.menu
                    .update(cx, |menu, cx| menu.close(Some(command), cx));
            }
            Action::Keep(command) => {
                let command = command.clone();
                self.menu.update(cx, |menu, cx| menu.dispatch(command, cx));
            }
            Action::UseDefaults => {
                self.accounts_customize = false;
                self.menu.update(cx, |menu, cx| {
                    menu.dispatch(policy_command(Value::Null), cx)
                });
                self.refit_accounts(window, cx);
            }
        }
    }

    /// Resizes the panel window after its content changed height (new data, Customize).
    pub(super) fn refit_accounts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.rows.first().map(|row| row["accounts"].clone()) else {
            return;
        };
        let appearance = self.menu.read(cx).appearance.clone();
        let width = window.bounds().size.width.as_f32() / appearance.scale;
        let height = match height(&panel, width, self.accounts_customize, &appearance, cx) {
            Ok(height) => height,
            Err(error) => {
                let chat = self.menu.read(cx).chat.clone();
                cx.defer(move |cx| {
                    let _ = chat.update(cx, |chat, cx| {
                        chat.error = Some(error.to_string());
                        cx.notify();
                    });
                });
                return;
            }
        };
        let available = self.menu.read(cx).source_bounds;
        let desired = px((height + MENU_CHROME) * appearance.scale);
        let room = available.bottom() - window.bounds().top() - px(12.0 * appearance.scale);
        if desired > room && window.bounds().size.height < desired {
            // Like the React popover's collision handling: re-place the taller panel so it
            // shifts up instead of hiding the new controls below the fold.
            let row = json!({"accounts": panel, "customize": self.accounts_customize});
            let depth = self.depth;
            self.menu
                .update(cx, |menu, cx| menu.reopen(depth, vec![row], cx));
            return;
        }
        self.heights = vec![height];
        window.resize(gpui::size(window.bounds().size.width, desired.min(room)));
        cx.notify();
    }

    fn open_priority_select(&mut self, policy: &Value, priorities: &Value, cx: &mut Context<Self>) {
        let rows: Vec<Value> = priorities
            .as_array()
            .into_iter()
            .flatten()
            .map(|option| {
                json!({
                    "label": option["label"],
                    "checked": option["value"] == policy["priority"],
                    "keepOpen": true,
                    "command": policy_command(with(policy, "priority", option["value"].clone())),
                })
            })
            .collect();
        let anchor = self.select_bounds.get();
        let scale = self.menu.read(cx).appearance.scale;
        let depth = self.depth + 1;
        self.menu.update(cx, |menu, cx| {
            menu.open_dropdown(
                rows,
                anchor,
                f32::from(anchor.size.width) / scale,
                depth,
                cx,
            )
        });
    }

    pub(super) fn render_accounts(
        &self,
        panel: &Value,
        window_origin: gpui::Point<Pixels>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let appearance = self.menu.read(cx).appearance.clone();
        let s = appearance.scale;
        let colors = Colors::new(&appearance);
        let mut content = div()
            .id("account-panel")
            .flex_shrink_0()
            .flex()
            .flex_col()
            .p(px(PADDING * s))
            .text_size(px(12.0 * s))
            .text_color(colors.foreground);
        for (index, block) in blocks(panel, self.accounts_customize)
            .into_iter()
            .enumerate()
        {
            content = content.child(self.render_block(
                index,
                block,
                &appearance,
                &colors,
                window_origin,
                cx,
            ));
        }
        content.into_any_element()
    }

    fn button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &'static str,
        icon_path: Option<&'static str>,
        outline: bool,
        disabled: bool,
        action: Action,
        colors: &Colors,
        s: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let muted = !outline;
        div()
            .flex()
            .child(
                div()
                    .id(id.into())
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .h(px(28.0 * s))
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .rounded(px(8.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(if muted {
                        colors.muted
                    } else {
                        colors.foreground
                    })
                    .when(outline, |this| this.border_1().border_color(colors.border))
                    .when(disabled, |this| this.opacity(0.5))
                    .when(!disabled, |this| {
                        this.chat_cursor_pointer()
                            .hover(|style| style.bg(colors.hover))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.account_action(&action, window, cx)
                            }))
                    })
                    .when_some(icon_path, |this, path| {
                        this.child(icon(
                            path,
                            14.0 * s,
                            if muted {
                                colors.muted
                            } else {
                                colors.foreground
                            },
                        ))
                    })
                    .child(label),
            )
            .into_any_element()
    }

    fn switch(&self, checked: bool, colors: &Colors, s: f32) -> AnyElement {
        div()
            .flex_shrink_0()
            .w(px(32.0 * s))
            .h(px(20.0 * s))
            .rounded(px(6.0 * s))
            .border_2()
            .border_color(if checked {
                colors.switch_on
            } else {
                gpui::transparent_black()
            })
            .bg(if checked {
                colors.switch_on
            } else {
                colors.switch_off
            })
            .child(
                div()
                    .size(px(16.0 * s))
                    .ml(px(if checked { 12.0 } else { 0.0 } * s))
                    .rounded(px(4.0 * s))
                    .bg(colors.thumb)
                    .shadow_sm(),
            )
            .into_any_element()
    }

    fn render_block(
        &self,
        index: usize,
        block: Block,
        appearance: &ChatAppearance,
        colors: &Colors,
        window_origin: gpui::Point<Pixels>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = appearance.scale;
        let paragraph = |text: String, dim: bool| {
            div()
                .text_size(px(PARAGRAPH.0 * s))
                .line_height(px(PARAGRAPH.1 * s))
                .text_color(colors.muted)
                .when(dim, |this| this.opacity(0.45))
                .child(text)
        };
        let strong = |text: String| {
            div()
                .text_size(px(STRONG.0 * s))
                .line_height(px(STRONG.1 * s))
                .font_weight(FontWeight::BOLD)
                .child(text)
        };
        match block {
            Block::Gap(height) => div().flex_shrink_0().h(px(height * s)).into_any_element(),
            Block::Header { busy } => {
                let icon_button = |id: &'static str,
                                   path: &'static str,
                                   label: &'static str,
                                   disabled: bool,
                                   action: Action| {
                    div()
                        .id(id)
                        .role(gpui::Role::Button)
                        .aria_label(label)
                        .size(px(28.0 * s))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.0 * s))
                        .when(disabled, |this| this.opacity(0.5))
                        .when(!disabled, |this| {
                            this.chat_cursor_pointer()
                                .hover(|style| style.bg(colors.hover))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.account_action(&action, window, cx)
                                }))
                        })
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                        })
                        .child(icon(path, 15.0 * s, colors.foreground))
                };
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(28.0 * s))
                    .child(
                        div()
                            .text_size(px(14.0 * s))
                            .line_height(px(20.0 * s))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Accounts & limits"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0 * s))
                            .child(icon_button(
                                "account-refresh",
                                "titlebar/refresh.svg",
                                "Refresh accounts and usage",
                                busy,
                                Action::Keep(json!({"type":"accounts","request":{"operation":"session","refresh":true}})),
                            ))
                            .child(icon_button(
                                "account-settings",
                                "titlebar/settings.svg",
                                "Manage accounts in Settings",
                                false,
                                Action::Close(json!({"type":"openAccountsSettings"})),
                            )),
                    )
                    .into_any_element()
            }
            Block::Alert(message) => div()
                .id("account-error")
                .role(gpui::Role::Alert)
                .p(px(12.0 * s))
                .border_1()
                .border_color(colors.border)
                .rounded(px(8.0 * s))
                .text_size(px(STRONG.0 * s))
                .line_height(px(STRONG.1 * s))
                .child(message)
                .into_any_element(),
            Block::Text {
                text, strong: true, ..
            } => strong(text).into_any_element(),
            Block::Text { text, dim, .. } => paragraph(text, dim).into_any_element(),
            Block::Recovery { reason, next, busy } => div()
                .id("account-recovery")
                .role(gpui::Role::Status)
                .flex()
                .flex_col()
                .gap(px(12.0 * s))
                .p(px(12.0 * s))
                .border_1()
                .border_color(colors.border)
                .rounded(px(8.0 * s))
                .child(strong(reason))
                .when_some(next, |this, next| this.child(paragraph(next, false)))
                .child(self.button(
                    "account-stop-recovery",
                    "Stop automatic recovery",
                    None,
                    true,
                    busy,
                    Action::Keep(json!({"type":"accounts","request":{"operation":"stopRecovery"}})),
                    colors,
                    s,
                    cx,
                ))
                .into_any_element(),
            Block::Current {
                identity: account,
                name,
                email,
            } => div()
                .flex()
                .items_center()
                .gap(px(12.0 * s))
                .when(account.is_object(), |this| {
                    this.child(identity(&account, appearance))
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .gap(px(4.0 * s))
                        .child(strong(name).truncate())
                        .when_some(email, |this, email| {
                            this.child(paragraph(email, false).truncate())
                        }),
                )
                .into_any_element(),
            Block::Meter {
                label,
                percent,
                left,
                right,
            } => div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(16.0 * s))
                        .text_size(px(11.0 * s))
                        .line_height(px(16.0 * s))
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                )
                .child(
                    div()
                        .my(px(8.0 * s))
                        .h(px(5.0 * s))
                        .w_full()
                        .rounded(px(3.0 * s))
                        .overflow_hidden()
                        .bg(colors.track)
                        .child(
                            div()
                                .h_full()
                                .w(relative((percent / 100.0).clamp(0.0, 1.0)))
                                .rounded(px(3.0 * s))
                                .bg(colors.fill),
                        ),
                )
                .child(
                    div()
                        .h(px(15.0 * s))
                        .flex()
                        .justify_between()
                        .gap(px(12.0 * s))
                        .text_size(px(10.0 * s))
                        .line_height(px(15.0 * s))
                        .text_color(colors.muted)
                        .child(left)
                        .when_some(right, |this, right| this.child(right)),
                )
                .into_any_element(),
            Block::Heading(heading) => div()
                .h(px(20.0 * s))
                .text_size(px(14.0 * s))
                .line_height(px(20.0 * s))
                .font_weight(FontWeight::SEMIBOLD)
                .child(heading)
                .into_any_element(),
            Block::Account { row, busy } => {
                let ready = row["ready"] == true;
                let action = if ready {
                    Action::Close(
                        json!({"type":"accounts","request":{"operation":"select","accountId":row["id"]}}),
                    )
                } else {
                    Action::Close(json!({"type":"openAccountsSettings"}))
                };
                let detail = row["detail"].as_str().map(str::to_owned);
                div()
                    .id(("account-row", index))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{} {}", text(&row, "name"), text(&row, "action")))
                    .flex()
                    .items_center()
                    .gap(px(12.0 * s))
                    .px(px(10.0 * s))
                    .py(px(12.0 * s))
                    .rounded(px(6.0 * s))
                    .when(busy, |this| this.opacity(0.45))
                    .when(!busy, |this| {
                        this.chat_cursor_pointer()
                            .hover(|style| style.bg(colors.hover))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.account_action(&action, window, cx)
                            }))
                    })
                    .when(!ready, |this| {
                        this.tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new(
                                "Reconnect this account in Settings",
                            )
                            .build(window, cx)
                        })
                    })
                    .child(identity(&row, appearance))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.0 * s))
                                    .line_height(px(18.0 * s))
                                    .font_weight(FontWeight::BOLD)
                                    .truncate()
                                    .child(text(&row, "name")),
                            )
                            .when_some(detail, |this, detail| {
                                this.child(
                                    div()
                                        .text_size(px(11.0 * s))
                                        .line_height(px(16.0 * s))
                                        .text_color(colors.muted)
                                        .truncate()
                                        .child(detail),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .child(text(&row, "action")),
                    )
                    .into_any_element()
            }
            Block::Divider => div().h(px(1.0)).bg(colors.border).into_any_element(),
            Block::PolicyHeading { customize } => div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0 * s))
                .h(px(28.0 * s))
                .child(
                    div()
                        .text_size(px(14.0 * s))
                        .line_height(px(20.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Keep going at a limit"),
                )
                .when(customize, |this| {
                    this.child(
                        div()
                            .id("account-customize")
                            .role(gpui::Role::Button)
                            .aria_label("Customize this session")
                            .h(px(28.0 * s))
                            .px(px(10.0 * s))
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .rounded(px(8.0 * s))
                            .text_color(colors.muted)
                            .chat_cursor_pointer()
                            .hover(|style| style.bg(colors.hover))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.accounts_customize = true;
                                this.refit_accounts(window, cx);
                            }))
                            .child(icon(
                                "titlebar/adjustments-horizontal.svg",
                                14.0 * s,
                                colors.muted,
                            ))
                            .child("Customize"),
                    )
                })
                .into_any_element(),
            Block::Toggle {
                label,
                checked,
                disabled,
                next,
            } => div()
                .id(("account-toggle", index))
                .role(gpui::Role::Switch)
                .aria_label(label)
                .aria_toggled(if checked {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .h(px(20.0 * s))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0 * s))
                .when(disabled, |this| this.opacity(0.45))
                .when(!disabled, |this| {
                    let command = policy_command(next);
                    this.chat_cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let command = command.clone();
                            this.menu.update(cx, |menu, cx| menu.dispatch(command, cx));
                        }))
                })
                .child(label)
                .child(self.switch(checked, colors, s))
                .into_any_element(),
            Block::Legend { dim } => div()
                .h(px(16.0 * s))
                .text_size(px(11.0 * s))
                .line_height(px(16.0 * s))
                .text_color(colors.muted)
                .when(dim, |this| this.opacity(0.45))
                .child("When the session's account runs out")
                .into_any_element(),
            Block::Segmented { policy, disabled } => {
                let wait = policy["atLimit"] != "switch";
                let segment = |value: &'static str, label: &'static str, pressed: bool| {
                    let command = policy_command(with(&policy, "atLimit", json!(value)));
                    div()
                        .id(("account-at-limit", if value == "wait" { 0usize } else { 1 }))
                        .role(gpui::Role::RadioButton)
                        .aria_label(label)
                        .flex_1()
                        .flex_basis(px(0.0))
                        .min_w_0()
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0 * s))
                        .text_size(px(12.0 * s))
                        .whitespace_nowrap()
                        .text_color(if pressed {
                            colors.segment_pressed_text
                        } else {
                            colors.segment_text
                        })
                        .when(pressed, |this| this.bg(colors.segment_pressed).shadow_sm())
                        .when(!pressed && !disabled, |this| {
                            this.chat_cursor_pointer()
                                .hover(|style| style.text_color(colors.segment_pressed_text))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let command = command.clone();
                                    this.menu.update(cx, |menu, cx| menu.dispatch(command, cx));
                                }))
                        })
                        .child(label)
                };
                div()
                    .id("account-at-limit-group")
                    .role(gpui::Role::RadioGroup)
                    .h(px(32.0 * s))
                    .flex()
                    .gap(px(3.0 * s))
                    .p(px(3.0 * s))
                    .rounded(px(8.0 * s))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.segment_track)
                    .when(disabled, |this| this.opacity(0.45))
                    .child(segment("wait", "Wait for reset", wait))
                    .child(segment("switch", "Use another account", !wait))
                    .into_any_element()
            }
            Block::Label { dim } => div()
                .h(px(18.0 * s))
                .line_height(px(18.0 * s))
                .when(dim, |this| this.opacity(0.45))
                .child("Account preference")
                .into_any_element(),
            Block::Select {
                policy,
                priorities,
                label,
                disabled,
            } => {
                let bounds = self.select_bounds.clone();
                div()
                    .id("account-priority")
                    .role(gpui::Role::ComboBox)
                    .aria_label("This session account preference")
                    .relative()
                    .h(px(32.0 * s))
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(8.0 * s))
                    .border_1()
                    .border_color(colors.border)
                    .when(disabled, |this| this.opacity(0.45))
                    .when(!disabled, |this| {
                        this.chat_cursor_pointer()
                            .hover(|style| style.bg(colors.hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_priority_select(&policy, &priorities, cx)
                            }))
                    })
                    .child(label)
                    .child(icon("titlebar/chevron-down.svg", 14.0 * s, colors.muted))
                    .child(
                        gpui::canvas(
                            move |rect: Bounds<Pixels>, _, _| {
                                bounds.set(Bounds::new(window_origin + rect.origin, rect.size))
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .into_any_element()
            }
            Block::Button {
                id,
                label,
                icon: icon_path,
                outline,
                disabled,
                action,
            } => self.button(
                id, label, icon_path, outline, disabled, action, colors, s, cx,
            ),
        }
    }
}

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

use super::super::{
    appearance::{ChatAppearance, ChatButtonFills, ChatFilledButton as _},
    new_session_welcome::brand_logo_color,
};
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
        /// The "Active" tag beside the name.
        active: Option<String>,
        /// The button at the end of the row for a problem Settings can fix.
        action: Option<String>,
        account_id: Option<String>,
    },
    /// An account's limits as compact columns, three to a line.
    Usage(Vec<UsageColumn>),
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
        disabled: bool,
        action: Action,
    },
}

/// One compact usage column: label and percent over a thin bar, the reset in its tooltip.
#[derive(Debug, PartialEq)]
struct UsageColumn {
    name: String,
    value: String,
    percent: f32,
    tooltip: String,
}

/// Columns per line in a usage row.
const USAGE_COLUMNS: usize = 3;
/// A usage line: the 14px label line, 4px, the 3px bar.
const USAGE_LINE: f32 = 21.0;
const USAGE_LINE_GAP: f32 = 6.0;

fn usage_columns(windows: &Value) -> Vec<UsageColumn> {
    windows
        .as_array()
        .into_iter()
        .flatten()
        .map(|window| UsageColumn {
            name: text(window, "name"),
            value: text(window, "value"),
            percent: window["percent"].as_f64().unwrap_or(0.0) as f32,
            tooltip: format!("{} \u{b7} {}", text(window, "label"), text(window, "reset")),
        })
        .collect()
}

fn usage_height(columns: usize) -> f32 {
    let lines = columns.div_ceil(USAGE_COLUMNS) as f32;
    lines * USAGE_LINE + (lines - 1.0).max(0.0) * USAGE_LINE_GAP
}

/// An other account's row: the name line, then its usage error (`error_height` tall, it wraps) or
/// its usage row.
fn account_row_height(row: &Value, error_height: f32) -> f32 {
    let usage = row["usage"].as_array().map_or(0, Vec::len);
    let below = if row["usageError"].is_string() {
        4.0 + error_height
    } else if usage > 0 {
        4.0 + usage_height(usage)
    } else {
        0.0
    };
    ACCOUNT_ROW_PADDING * 2.0 + 18.0 + below
}

const ACCOUNT_ROW_PADDING: f32 = 6.0;
const ACCOUNT_ROW_PADDING_X: f32 = 8.0;

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

/// Settings > Accounts, on `account`'s sign-in when it names one
/// (CDXC:AgentProviders 2026-10-31 on `open_gpui_settings_accounts_reconnect`).
fn reconnect_command(account: Option<&str>) -> Value {
    match account {
        Some(account) => json!({"type":"openAccountsSettings","reconnectAccountId":account}),
        None => json!({"type":"openAccountsSettings"}),
    }
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
            disabled: false,
            action: Action::Close(json!({"type":"openAccountsSettings"})),
        });
        return out;
    }
    let busy = panel["busy"] == true;
    out.push(Header { busy });
    out.push(Gap(8.0));
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
        out.push(Gap(8.0));
    }
    // CDXC:AgentProviders 2026-10-28 DECISION: User: "this menu is too huge please make it
    // smaller". Each account is a name line over one row of compact 5h / 7d / Fable columns (resets
    // in their tooltips) instead of a full-width bar per limit.
    // CDXC:AgentProviders 2026-10-31 DECISION: User marked up the panel: the current account says
    // it is active and shows its error on that row (`current_account_problem` in the core), and the
    // Context row and the "Switching resumes the same conversation" sentence go.
    let problem = &session["problem"];
    out.push(Current {
        identity: session["current"].clone(),
        name: text(session, "name"),
        email: session["email"].as_str().map(str::to_owned),
        active: session["activeLabel"].as_str().map(str::to_owned),
        action: problem["action"].as_str().map(str::to_owned),
        account_id: session["accountId"].as_str().map(str::to_owned),
    });
    out.push(Gap(6.0));
    if let Some(message) = problem["message"].as_str() {
        out.push(Text {
            text: message.to_owned(),
            strong: false,
            dim: false,
        });
        out.push(Gap(6.0));
    }
    let usage = usage_columns(&session["usage"]);
    if !usage.is_empty() {
        out.push(Usage(usage));
        out.push(Gap(12.0));
    } else if problem.is_null() {
        out.push(Text {
            text: "Usage is unavailable for this login.".into(),
            strong: false,
            dim: false,
        });
        out.push(Gap(12.0));
    } else {
        out.push(Gap(6.0));
    }
    out.push(Heading(text(session, "switchHeading")));
    out.push(Gap(2.0));
    for row in session["others"].as_array().into_iter().flatten() {
        out.push(Account {
            row: row.clone(),
            busy,
        });
    }
    out.push(Gap(8.0));
    out.push(Divider);
    out.push(Gap(8.0));
    let policy_state = &session["policy"];
    let custom = policy_state["custom"] == true;
    out.push(PolicyHeading {
        customize: !customize && !custom,
    });
    out.push(Gap(2.0));
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
            Header { .. } | Button { .. } => 28.0,
            PolicyHeading { .. } => 24.0,
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
            Current { .. } => 20.0,
            Usage(columns) => usage_height(columns.len()),
            Heading(_) => 16.0,
            Account { row, .. } => {
                let error = match row["usageError"].as_str() {
                    Some(error) => {
                        measure.lines(error, PARAGRAPH, width - ACCOUNT_ROW_PADDING_X * 2.0)?
                    }
                    None => 0.0,
                };
                account_row_height(row, error)
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
    /// The panel's labelled buttons (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
    button: ChatButtonFills,
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
        // The menu is frosted under window glass, so its segmented track and pressed segment
        // take the modal kit's rail washes instead of solid fills.
        let rail = crate::app::window::native_modal_kit::raised_rail_colors(
            light,
            crate::app::helpers::window_glass_active(),
        );
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
            button: appearance.neutral_button_fills(),
            segment_track: rail.track.into(),
            segment_text: rail.text.into(),
            segment_pressed: rail.pressed.into(),
            segment_pressed_text: rail.active_text.into(),
        }
    }
}

/// The provider logo in front of an account's name line.
fn logo(value: &Value, appearance: &ChatAppearance) -> AnyElement {
    let provider = value["provider"].as_str().unwrap_or("claude");
    svg()
        .path(format!("agent-icons/{provider}.svg"))
        .flex_shrink_0()
        .size(px(16.0 * appearance.scale))
        .text_color(brand_logo_color(provider, appearance))
        .into_any_element()
}

/// The account's name line: logo, bold name, and the muted email after it.
fn name_line(
    account: Option<&Value>,
    name: String,
    active: Option<String>,
    email: Option<String>,
    appearance: &ChatAppearance,
    colors: &Colors,
) -> gpui::Div {
    let s = appearance.scale;
    div()
        .flex()
        .items_center()
        .gap(px(8.0 * s))
        .h(px(18.0 * s))
        .min_w_0()
        .when_some(account, |this, account| {
            this.child(logo(account, appearance))
        })
        .child(
            div()
                .flex_shrink(1.0)
                .min_w_0()
                .text_size(px(12.0 * s))
                .line_height(px(18.0 * s))
                .font_weight(FontWeight::BOLD)
                .truncate()
                .child(name),
        )
        .when_some(active, |this, active| {
            this.child(
                div()
                    .flex_shrink_0()
                    .h(px(16.0 * s))
                    .px(px(5.0 * s))
                    .flex()
                    .items_center()
                    .rounded(px(4.0 * s))
                    .border_1()
                    .border_color(colors.border)
                    .text_size(px(10.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted)
                    .child(active),
            )
        })
        .when_some(email, |this, email| {
            this.child(
                div()
                    .flex_shrink(1.0)
                    .min_w_0()
                    .text_size(px(11.0 * s))
                    .line_height(px(18.0 * s))
                    .text_color(colors.muted)
                    .truncate()
                    .child(email),
            )
        })
}

/// A thin meter bar `percent` full.
fn bar(percent: f32, colors: &Colors, s: f32) -> gpui::Div {
    div()
        .h(px(3.0 * s))
        .w_full()
        .rounded(px(2.0 * s))
        .overflow_hidden()
        .bg(colors.track)
        .child(
            div()
                .h_full()
                .w(relative((percent / 100.0).clamp(0.0, 1.0)))
                .rounded(px(2.0 * s))
                .bg(colors.fill),
        )
}

/// The compact usage row: up to three columns a line, each a label and percent over a bar.
fn usage_row(
    id: impl Into<gpui::ElementId>,
    columns: Vec<UsageColumn>,
    colors: &Colors,
    s: f32,
) -> AnyElement {
    let mut lines = Vec::new();
    let mut columns = columns.into_iter().enumerate().peekable();
    while columns.peek().is_some() {
        let mut line = div().flex().gap(px(16.0 * s));
        let mut count = 0;
        for (index, column) in columns.by_ref().take(USAGE_COLUMNS) {
            count += 1;
            let tooltip = column.tooltip;
            line = line.child(
                div()
                    .id(("account-usage", index))
                    .flex_1()
                    .flex_basis(px(0.0))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0 * s))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .child(
                        div()
                            .h(px(14.0 * s))
                            .flex()
                            .justify_between()
                            .gap(px(6.0 * s))
                            .text_size(px(11.0 * s))
                            .line_height(px(14.0 * s))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.muted)
                                    .child(column.name),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(column.value),
                            ),
                    )
                    .child(bar(column.percent, colors, s)),
            );
        }
        // A short last line keeps its columns as wide as the full lines above.
        for _ in count..USAGE_COLUMNS {
            line = line.child(div().flex_1().flex_basis(px(0.0)));
        }
        lines.push(line);
    }
    div()
        .id(id.into())
        .flex()
        .flex_col()
        .gap(px(USAGE_LINE_GAP * s))
        .children(lines)
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
        disabled: bool,
        action: Action,
        colors: &Colors,
        s: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
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
                    .text_color(colors.foreground)
                    .chat_filled(colors.button, !disabled)
                    .when(disabled, |this| this.opacity(0.5))
                    .when(!disabled, |this| {
                        this.chat_cursor_pointer().on_click(cx.listener(
                            move |this, _, window, cx| this.account_action(&action, window, cx),
                        ))
                    })
                    .when_some(icon_path, |this, path| {
                        this.child(icon(path, 14.0 * s, colors.foreground))
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
            // CDXC:AgentProviders 2026-09-12 DECISION: Give the recovery message and Stop automatic recovery button clear spacing, and keep that button rounded.
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
                active,
                action,
                account_id,
            } => name_line(
                account.is_object().then_some(&account),
                name,
                active,
                email,
                appearance,
                colors,
            )
            .h(px(20.0 * s))
            .when_some(action, |this, action| {
                let open_settings = Action::Close(reconnect_command(
                    account_id.as_deref().filter(|_| action == "Reconnect"),
                ));
                this.child(div().flex_1()).child(
                    div()
                        .id("account-current-action")
                        .role(gpui::Role::Button)
                        .aria_label(action.clone())
                        .flex_shrink_0()
                        .h_full()
                        .px(px(8.0 * s))
                        .flex()
                        .items_center()
                        .rounded(px(6.0 * s))
                        .whitespace_nowrap()
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(colors.hover))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.account_action(&open_settings, window, cx)
                        }))
                        .child(action),
                )
            })
            .into_any_element(),
            Block::Usage(columns) => usage_row(("account-usage-row", index), columns, colors, s),
            Block::Heading(heading) => div()
                .h(px(16.0 * s))
                .text_size(px(11.0 * s))
                .line_height(px(16.0 * s))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.muted)
                .child(heading)
                .into_any_element(),
            Block::Account { row, busy } => {
                let ready = row["ready"] == true;
                let action = if ready {
                    Action::Close(
                        json!({"type":"accounts","request":{"operation":"select","accountId":row["id"]}}),
                    )
                } else {
                    Action::Close(reconnect_command(row["id"].as_str()))
                };
                let usage = usage_columns(&row["usage"]);
                let usage_error = row["usageError"].as_str().map(str::to_owned);
                let action_label = div()
                    .flex_shrink_0()
                    .pl(px(8.0 * s))
                    .whitespace_nowrap()
                    .child(text(&row, "action"))
                    .into_any_element();
                div()
                    .id(("account-row", index))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{} {}", text(&row, "name"), text(&row, "action")))
                    .flex()
                    .flex_col()
                    .gap(px(4.0 * s))
                    .px(px(ACCOUNT_ROW_PADDING_X * s))
                    .py(px(ACCOUNT_ROW_PADDING * s))
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
                    .child(
                        name_line(
                            Some(&row),
                            text(&row, "name"),
                            None,
                            row["email"].as_str().map(str::to_owned),
                            appearance,
                            colors,
                        )
                        .child(div().flex_1())
                        .child(action_label),
                    )
                    .map(|this| match usage_error {
                        Some(error) => this.child(
                            div()
                                .text_size(px(PARAGRAPH.0 * s))
                                .line_height(px(PARAGRAPH.1 * s))
                                .text_color(colors.muted)
                                .child(error),
                        ),
                        None if !usage.is_empty() => {
                            this.child(usage_row(("account-usage-row", index), usage, colors, s))
                        }
                        None => this,
                    })
                    .into_any_element()
            }
            Block::Divider => div().h(px(1.0)).bg(colors.border).into_any_element(),
            Block::PolicyHeading { customize } => div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0 * s))
                .h(px(24.0 * s))
                .child(
                    div()
                        .text_size(px(13.0 * s))
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
                            .h(px(24.0 * s))
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
                disabled,
                action,
            } => self.button(id, label, icon_path, disabled, action, colors, s, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(name: &str, percent: f64, reset: &str) -> Value {
        json!({
            "label": format!("{name}: {percent}%"),
            "name": name,
            "value": format!("{percent}%"),
            "percent": percent,
            "reset": reset,
            "resetIn": null,
        })
    }

    fn panel() -> Value {
        let usage = json!([
            window("5h", 16.0, "Resets 2h 36m"),
            window("7d", 40.0, "Resets 5d 4h"),
            window("Fable", 0.0, "Resets 5d 4h"),
        ]);
        json!({
            "kind": "panel",
            "busy": false,
            "session": {
                "current": {"provider": "claude", "figures": ["40%", "16%"]},
                "name": "ST",
                "email": "s•••m@•••••.•••",
                "activeLabel": "Active",
                "problem": null,
                "usage": usage,
                "switchHeading": "Switch account",
                "others": [
                    {"provider": "claude", "name": "SP", "email": "y•••a@•••••.•••", "usage": usage, "ready": true, "action": "Use account →"},
                    {"provider": "claude", "name": "MS", "usageError": "Usage unavailable", "usage": [], "ready": true, "action": "Use account →"},
                ],
                "policy": {"summary": "Session defaults · Switch when eligible", "custom": false},
            },
        })
    }

    #[test]
    fn current_usage_is_one_row_of_columns_with_resets_in_tooltips() {
        let blocks = blocks(&panel(), false);
        let usage: Vec<&Vec<UsageColumn>> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Usage(columns) => Some(columns),
                _ => None,
            })
            .collect();
        assert_eq!(usage.len(), 1);
        let columns: Vec<(&str, &str, &str)> = usage[0]
            .iter()
            .map(|column| {
                (
                    column.name.as_str(),
                    column.value.as_str(),
                    column.tooltip.as_str(),
                )
            })
            .collect();
        assert_eq!(
            columns,
            [
                ("5h", "16%", "5h: 16% · Resets 2h 36m"),
                ("7d", "40%", "7d: 40% · Resets 5d 4h"),
                ("Fable", "0%", "Fable: 0% · Resets 5d 4h"),
            ]
        );
        assert!(matches!(
            &blocks[2],
            Block::Current { active: Some(active), action: None, .. } if active == "Active"
        ));
        // The panel's note is gone, so nothing but the policy summary is text in a healthy panel.
        let texts: Vec<&str> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["Session defaults · Switch when eligible"]);
    }

    #[test]
    fn an_expired_login_shows_its_words_and_action_on_the_current_row() {
        let mut panel = panel();
        panel["session"]["usage"] = json!([]);
        panel["session"]["problem"] = json!({
            "message": "The saved login expired. Reconnect this account.",
            "action": "Reconnect",
        });
        let blocks = blocks(&panel, false);
        assert!(matches!(
            &blocks[2],
            Block::Current { action: Some(action), .. } if action == "Reconnect"
        ));
        let texts: Vec<&str> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        // Its own words replace "Usage is unavailable for this login."
        assert_eq!(
            texts,
            [
                "The saved login expired. Reconnect this account.",
                "Session defaults · Switch when eligible",
            ]
        );
    }

    #[test]
    fn usage_rows_wrap_three_columns_a_line() {
        assert_eq!(usage_height(3), USAGE_LINE);
        assert_eq!(usage_height(4), USAGE_LINE * 2.0 + USAGE_LINE_GAP);
    }

    #[test]
    fn other_account_rows_are_no_taller_than_the_old_two_line_rows() {
        let panel = panel();
        let others = panel["session"]["others"].as_array().unwrap();
        // The old row: 12px padding around the 26px identity or the name and detail lines.
        let old = 24.0 + 34.0;
        assert!(account_row_height(&others[0], 0.0) <= old);
        // An error wraps, so the row is as tall as its measured lines.
        assert_eq!(
            account_row_height(&others[1], PARAGRAPH.1 * 2.0),
            ACCOUNT_ROW_PADDING * 2.0 + 18.0 + 4.0 + PARAGRAPH.1 * 2.0
        );
    }
}

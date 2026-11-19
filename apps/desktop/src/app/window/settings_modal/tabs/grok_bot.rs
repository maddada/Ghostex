//! Settings › Grok Bot: the Grok Bot Integration built-in extension's page. At the top, the switch that
//! turns the extension on or off (`grokBotHidden`); while it is off, that is all the page shows.
//! While it is on and Grok Bot isn't connected, the setup checklist of Q4206 = A: four steps,
//! each ticked off as Ghostex sees it happen (the desktop app found, the bot's first
//! `ghostex grokbot brief`, the webhook connected, the user's yes to "Did the test note show up?"),
//! every one with its fix in plain words. Once connected: notes sent, Send a test note, Change,
//! Copy instructions and Disconnect.
//!
//! Everything goes through `/api/grokbot` (server/src/grokbot/api.rs), the same calls
//! `ghostex grokbot` makes: the key typed or pasted here goes to gxserver once, which keeps it in
//! its owner-only file; no answer ever carries it back, and this page never shows it again.
//!
//! CDXC:GrokBot 2026-11-19 DECISION:
//! User, after setting it up from the terminal: "the instructions here are good and worked but please add a user experience for this that helps the user go through it without even opening terminal should hold the users hand fully. also we should tell the agent to create the web hook right away from 1st message." Q4206 = A (a checklist that keeps every step in view and ticks each off by itself, so the user can switch to Grok Bot and back), with: "don't put this under integrations put this as a new page in settings below integrations make it an enabled by default extension but allow people to disable this extension with a button at the top of the settings page". The webhook form shows at once rather than after step 2, so someone whose bot already exists isn't held until it runs `brief` again; the test question shows only right after connecting here (or after a No), so a connection made from the terminal opens on the Connected view.
//! SEE-ALSO: packages/settings-catalog/src/pages/grok_bot.rs (the rows Settings search finds), packages/settings-catalog/src/data/official_extensions.rs (the extension), server/src/grokbot/instructions.md (the bot makes the "Ghostex reports" routine in its first reply).
use super::super::super::native_modal_kit::*;
use super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, RowSpec, SettingsPage, card_inset, setting_row,
    settings_button, settings_list_item, settings_note_row, settings_section, settings_text_input,
    switch_control,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{
    SettingsStore, post_store_message, store_copy_to_clipboard, store_gxserver_rpc,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Task, Window, div, px,
};
use gpui_component::input::InputState;
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

const ENDPOINT: &str = "/api/grokbot";
/// `.settings-page-width grid gap-6 px-5 py-5`: 20px above the first section on top of its own 12px.
const PAGE_TOP_PADDING: f32 = 20.0;
const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// Connect and test wait for Grok Bot's answer (up to 20 seconds) inside the call.
const SEND_TIMEOUT: Duration = Duration::from_secs(45);
/// How often the checklist looks again while it waits for something to happen in Grok Bot.
const REFRESH_EVERY: Duration = Duration::from_secs(3);
const SWITCH_KEY: &str = "grokBotHidden";
const DOWNLOAD_URL: &str = "https://x.ai/bot";
/// The published Ghostex bot, ready to add to Grok Bot.
const TEMPLATE_URL: &str = "https://grok-bot.directory/bots/ghostex";

pub(crate) fn grok_bot_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| GrokBotTab::new(store.clone(), window, cx))
        .into()
}

/// What `/api/grokbot` `setup` answered.
#[derive(Clone, Default)]
struct Setup {
    connected: bool,
    url: String,
    sent: u64,
    last_sent_at: Option<i64>,
    last_error: Option<String>,
    /// `running`, `found`, `missing` or `unknown` (not a Mac).
    app: String,
    app_confirmed: bool,
    brief_seen: bool,
    test_arrived: Option<bool>,
}

impl Setup {
    fn from_answer(answer: &Value) -> Self {
        Self {
            connected: answer["connected"] == true,
            url: text(answer, "url"),
            sent: answer["sent"].as_u64().unwrap_or_default(),
            last_sent_at: answer["lastSentAt"].as_i64(),
            last_error: answer["lastError"].as_str().map(str::to_string),
            app: text(answer, "app"),
            app_confirmed: answer["appConfirmedAt"].as_i64().is_some(),
            brief_seen: answer["briefSeenAt"].as_i64().is_some(),
            test_arrived: answer["testArrived"].as_bool(),
        }
    }

    fn app_done(&self) -> bool {
        self.app == "running" || self.app == "found" || self.app_confirmed
    }
}

/// A message under a step: plain (`false`) or the error colour (`true`).
type Note = Option<(bool, String)>;

pub(crate) struct GrokBotTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// `None` until the first read answered.
    setup: Option<Setup>,
    read_error: Option<String>,
    url: Entity<InputState>,
    key: Entity<InputState>,
    /// A call to Grok Bot is running (connect, test, disconnect).
    busy: bool,
    connect_note: Note,
    test_note: Note,
    /// Connected from this page just now: ask "Did the test note show up?" before the Connected
    /// view.
    asking_test: bool,
    /// The Connected view's Change opened the webhook form again.
    changing: bool,
    copied: bool,
    /// A `setup` read is on its way; the refresh skips its tick rather than stack another.
    reading: bool,
    _refresh: Option<Task<()>>,
}

impl SettingsPage for GrokBotTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl super::HoldsUnsavedInput for GrokBotTab {
    /// A key or address typed and not connected yet.
    fn holds_unsaved_input(&self, cx: &App) -> bool {
        !self.key.read(cx).value().trim().is_empty()
            || (!self.url.read(cx).value().trim().is_empty() && self.form_open())
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// "3 min ago" for a time in milliseconds since 1970.
fn ago(at_ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(at_ms);
    let seconds = ((now - at_ms) / 1000).max(0);
    match seconds {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86_399 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

/// The address without its secret-looking tail: host and path start, and the last four
/// characters (`api2.cursor.sh/automations/webhook/…k3f9`).
fn short_url(url: &str) -> String {
    let bare = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let chars: Vec<char> = bare.chars().collect();
    if chars.len() <= 44 {
        return bare.to_string();
    }
    let head: String = chars[..36].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// What to do when Grok Bot turned the test note down, by the `failure` kind gxserver names.
fn failure_fix(kind: &str, message: &str) -> String {
    match kind {
        "keyRefused" => "Grok Bot refused the key. In the routine, click key again, paste it here and connect again.".to_string(),
        "notFound" => "Grok Bot doesn't know that address: the routine or its webhook was deleted or made again. Ask the bot to make the Ghostex reports routine, then paste its new POST to and key.".to_string(),
        "unreachable" => "Ghostex can't reach Grok Bot. Check this computer is online, then try again.".to_string(),
        "notStarted" => "Grok Bot didn't run the routine. Check Ghostex reports isn't paused in Grok Bot, then try again.".to_string(),
        _ => message.to_string(),
    }
}

/// What's wrong with the two fields before anything is sent, in plain words.
fn check_fields(url: &str, key: &str) -> Result<(), String> {
    let looks_like_address =
        |value: &str| value.starts_with("https://") || value.starts_with("http://");
    if url.is_empty() {
        return Err("Paste the routine's POST to address.".to_string());
    }
    if !looks_like_address(url) {
        return Err(if url.contains(' ') || url.contains('.') {
            "The address must start with https://. In the routine, click POST to and paste it again.".to_string()
        } else {
            "This looks like the key, not the address. In the routine, click POST to and paste that here.".to_string()
        });
    }
    if key.is_empty() {
        return Err("Paste the routine's key.".to_string());
    }
    if looks_like_address(key) {
        return Err(
            "This is the address, not the key. In the routine, click key and paste that here."
                .to_string(),
        );
    }
    Ok(())
}

impl GrokBotTab {
    fn new(store: Entity<SettingsStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://… (POST to)"));
        let key = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("Key"));
        let refresh = cx.spawn(async move |page, cx| {
            loop {
                cx.background_executor().timer(REFRESH_EVERY).await;
                let alive = page.update(cx, |page, cx| {
                    // Not while the extension is off (the page shows only its switch) or while
                    // the last read hasn't answered.
                    if page.store.read(cx).active_tab() == SettingsTabId::GrokBot
                        && page.enabled(cx)
                        && !page.reading
                        && page.waiting()
                    {
                        page.read(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        let mut tab = Self {
            store,
            fields: FieldStates::default(),
            setup: None,
            read_error: None,
            url,
            key,
            busy: false,
            connect_note: None,
            test_note: None,
            asking_test: false,
            changing: false,
            copied: false,
            reading: false,
            _refresh: Some(refresh),
        };
        tab.read(cx);
        tab
    }

    fn enabled(&self, cx: &App) -> bool {
        self.store
            .read(cx)
            .built_in_extension_on(ghostex_settings_catalog::built_in_extensions::GROK_BOT)
    }

    /// Something on the checklist can still tick off by itself (the app, the bot's first brief).
    fn waiting(&self) -> bool {
        !self.busy
            && self
                .setup
                .as_ref()
                .is_some_and(|setup| !setup.connected && (!setup.app_done() || !setup.brief_seen))
    }

    /// The webhook form is showing: not connected yet, or Change was pressed.
    fn form_open(&self) -> bool {
        self.changing || self.setup.as_ref().is_some_and(|setup| !setup.connected)
    }

    fn call(
        &mut self,
        params: Value,
        timeout: Duration,
        reply: impl FnOnce(&mut Self, Result<Value, String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            ENDPOINT,
            params,
            timeout,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    reply(page, result, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        self.reading = true;
        self.call(
            json!({ "op": "setup" }),
            READ_TIMEOUT,
            |page, result, _cx| match result {
                Ok(answer) => {
                    page.reading = false;
                    page.read_error = None;
                    page.setup = Some(Setup::from_answer(&answer));
                }
                Err(error) => {
                    page.reading = false;
                    page.read_error = Some(error);
                }
            },
            cx,
        );
    }

    fn set_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.update_setting(SWITCH_KEY, json!(!on), cx)
        });
        self.read(cx);
    }

    fn copy_instructions(&mut self, cx: &mut Context<Self>) {
        self.call(
            json!({ "op": "instructions" }),
            READ_TIMEOUT,
            |page, result, cx| match result {
                Ok(answer) => {
                    store_copy_to_clipboard(&page.store, text(&answer, "text"), cx);
                    page.copied = true;
                    cx.spawn(async move |page, cx| {
                        cx.background_executor().timer(Duration::from_secs(2)).await;
                        let _ = page.update(cx, |page, cx| {
                            page.copied = false;
                            cx.notify();
                        });
                    })
                    .detach();
                }
                Err(error) => page.read_error = Some(error),
            },
            cx,
        );
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        post_store_message(
            &self.store,
            json!({ "type": "openExternalUrl", "url": url }),
            cx,
        );
    }

    fn open_grok_bot(&mut self, cx: &mut Context<Self>) {
        self.call(
            json!({ "op": "openApp" }),
            READ_TIMEOUT,
            |page, result, _cx| {
                if let Err(error) = result {
                    page.read_error = Some(error);
                }
            },
            cx,
        );
    }

    fn confirm_app(&mut self, cx: &mut Context<Self>) {
        self.call(
            json!({ "op": "confirmApp" }),
            READ_TIMEOUT,
            |page, _result, cx| page.read(cx),
            cx,
        );
    }

    /// Fills `field` from the clipboard (read off the UI thread).
    fn paste_into(
        &mut self,
        field: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let read = cx.read_from_clipboard_async();
        let window_handle = window.window_handle();
        cx.spawn(async move |_page, cx| {
            let Ok(Some(item)) = read.await else {
                return;
            };
            let Some(text) = item.text() else {
                return;
            };
            let text = text.trim().to_string();
            let _ = window_handle.update(cx, |_, window, cx| {
                field.update(cx, |input, cx| input.set_value(text, window, cx));
            });
        })
        .detach();
    }

    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.url.read(cx).value().trim().to_string();
        let key = self.key.read(cx).value().trim().to_string();
        if let Err(problem) = check_fields(&url, &key) {
            self.connect_note = Some((true, problem));
            cx.notify();
            return;
        }
        self.busy = true;
        self.connect_note = Some((false, "Connecting and sending the test note…".to_string()));
        let (url_field, key_field) = (self.url.clone(), self.key.clone());
        let window_handle = window.window_handle();
        self.call(
            json!({ "op": "connect", "url": url, "key": key }),
            SEND_TIMEOUT,
            move |page, result, cx| {
                page.busy = false;
                match result {
                    Ok(answer) => {
                        // The key leaves the field as soon as gxserver has it.
                        let _ = window_handle.update(cx, |_, window, cx| {
                            key_field.update(cx, |input, cx| input.set_value("", window, cx));
                            url_field.update(cx, |input, cx| input.set_value("", window, cx));
                        });
                        page.setup = Some(Setup::from_answer(&answer));
                        page.changing = false;
                        page.asking_test = true;
                        page.connect_note = None;
                        page.test_note = (answer["tested"] == false).then(|| {
                            (
                                true,
                                failure_fix(
                                    answer["failure"].as_str().unwrap_or_default(),
                                    answer["message"].as_str().unwrap_or_default(),
                                ),
                            )
                        });
                        page.read(cx);
                    }
                    Err(error) => page.connect_note = Some((true, error)),
                }
            },
            cx,
        );
        cx.notify();
    }

    fn send_test(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.test_note = Some((false, "Sending the test note…".to_string()));
        self.call(
            json!({ "op": "test" }),
            SEND_TIMEOUT,
            |page, result, cx| {
                page.busy = false;
                page.test_note = Some(match result {
                    Ok(answer) if answer["sent"] == true => (
                        false,
                        "Grok Bot took it. Look for it in the Ghostex bot's chat.".to_string(),
                    ),
                    Ok(answer) => (
                        true,
                        failure_fix(
                            answer["failure"].as_str().unwrap_or_default(),
                            answer["message"].as_str().unwrap_or_default(),
                        ),
                    ),
                    Err(error) => (true, error),
                });
                page.read(cx);
            },
            cx,
        );
        cx.notify();
    }

    fn answer_test(&mut self, arrived: bool, cx: &mut Context<Self>) {
        if arrived {
            self.asking_test = false;
            self.test_note = None;
        }
        self.call(
            json!({ "op": "confirmTest", "arrived": arrived }),
            READ_TIMEOUT,
            |page, _result, cx| page.read(cx),
            cx,
        );
        cx.notify();
    }

    fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.call(
            json!({ "op": "disconnect" }),
            READ_TIMEOUT,
            |page, result, cx| {
                page.busy = false;
                page.asking_test = false;
                page.changing = false;
                page.test_note = None;
                if let Err(error) = result {
                    page.read_error = Some(error);
                }
                page.read(cx);
            },
            cx,
        );
        cx.notify();
    }

    fn note_line(p: &SettingsPalette, note: &Note) -> Option<AnyElement> {
        note.as_ref().map(|(bad, words)| {
            div()
                .text_size(px(12.5))
                .text_color(hsla(if *bad { p.destructive } else { p.muted }))
                .child(words.clone())
                .into_any_element()
        })
    }

    fn detail(words: impl Into<SharedString>) -> Option<AnyElement> {
        Some(div().child(words.into()).into_any_element())
    }

    fn switch_row(&mut self, p: &SettingsPalette, on: bool, cx: &mut Context<Self>) -> AnyElement {
        // The page's own words: the catalog row is "Turn off Grok Bot Integration" (the key is
        // `…Hidden`), which reads backwards beside a switch that is on when the extension is.
        let title = "Grok Bot Integration".to_string();
        let subtitle = "Control your agents from a \"Ghostex\" bot in Grok Bot on your phone, and get their notes there.".to_string();
        let control = switch_control(
            p,
            "grok-bot-switch",
            title.clone(),
            on,
            false,
            None,
            |page: &mut Self, next, _window, cx| page.set_enabled(next, cx),
            cx,
        )
        .into_any_element();
        setting_row(
            p,
            SWITCH_KEY,
            RowSpec::new(title).description(subtitle),
            None,
            control,
            cx,
        )
    }

    /// The webhook form: POST to and key, each with Paste, and Connect.
    fn webhook_form(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = |words: &'static str| {
            div()
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child(words)
        };
        let field = |page: &mut Self,
                     input: Entity<InputState>,
                     id: &'static str,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            let target = input.clone();
            h_flex()
                .w_full()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(settings_text_input(p, &input, None, false, window, cx)),
                )
                .child(settings_button(
                    p,
                    id,
                    "Paste",
                    None,
                    ButtonVariant::Outline,
                    page.busy,
                    None,
                    move |page: &mut Self, window, cx| page.paste_into(target.clone(), window, cx),
                    cx,
                ))
        };
        let url_row = field(self, self.url.clone(), "grok-bot-paste-url", window, cx);
        let key_row = field(self, self.key.clone(), "grok-bot-paste-key", window, cx);
        let mut buttons = h_flex().gap(px(8.0)).child(settings_button(
            p,
            "grok-bot-connect",
            "Connect and send a test note",
            None,
            ButtonVariant::Default,
            self.busy,
            None,
            |page: &mut Self, window, cx| page.connect(window, cx),
            cx,
        ));
        if self.changing {
            buttons = buttons.child(settings_button(
                p,
                "grok-bot-change-cancel",
                "Cancel",
                None,
                ButtonVariant::Outline,
                self.busy,
                None,
                |page: &mut Self, _window, cx| {
                    page.changing = false;
                    page.connect_note = None;
                    cx.notify();
                },
                cx,
            ));
        }
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(label("POST to"))
            .child(url_row)
            .child(label("key"))
            .child(key_row)
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(hsla(p.muted))
                    .child("Click POST to and paste it in the first box, then key in the second (the header field works too). Ghostex keeps the key in a private file and never shows it again."),
            )
            .child(div().h(px(4.0)))
            .child(buttons)
            .children(Self::note_line(p, &self.connect_note))
            .into_any_element()
    }

    /// The checklist: one row a step, the next step's detail under it.
    fn checklist(
        &mut self,
        p: &SettingsPalette,
        setup: &Setup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        let busy = self.busy;

        // 1. The desktop app.
        let (status, detail, controls) = match setup.app.as_str() {
            "running" => (ListItemStatus::Success, "Running on this computer".to_string(), None),
            "found" => (
                ListItemStatus::Warning,
                "Installed but not running. Open it, sign in and leave it running: the bot reaches this computer through it.".to_string(),
                Some(settings_button(p, "grok-bot-open-app", "Open Grok Bot", None, ButtonVariant::Outline, busy, None, |page: &mut Self, _window, cx| page.open_grok_bot(cx), cx)),
            ),
            _ if setup.app_confirmed => (ListItemStatus::Success, "Installed".to_string(), None),
            "missing" => (
                ListItemStatus::Neutral,
                "Not found on this computer. Download it, sign in and leave it running; Ghostex looks again by itself.".to_string(),
                Some(settings_button(p, "grok-bot-download", "Download Grok Bot", None, ButtonVariant::Outline, false, None, |page: &mut Self, _window, cx| page.open_url(DOWNLOAD_URL, cx), cx)),
            ),
            _ => (
                ListItemStatus::Neutral,
                "Download it, sign in and leave it running, then say it's installed.".to_string(),
                Some(
                    h_flex()
                        .gap(px(8.0))
                        .child(settings_button(p, "grok-bot-download", "Download", None, ButtonVariant::Outline, false, None, |page: &mut Self, _window, cx| page.open_url(DOWNLOAD_URL, cx), cx))
                        .child(settings_button(p, "grok-bot-app-installed", "It's installed", None, ButtonVariant::Outline, busy, None, |page: &mut Self, _window, cx| page.confirm_app(cx), cx))
                        .into_any_element(),
                ),
            ),
        };
        rows.push(settings_list_item(
            p,
            Some(status),
            None,
            "1. Grok Bot desktop app",
            Self::detail(detail),
            controls,
        ));

        // 2. The bot.
        let copy_label = if self.copied {
            "Copied"
        } else {
            "Copy instructions"
        };
        if setup.brief_seen {
            rows.push(settings_list_item(
                p,
                Some(ListItemStatus::Success),
                None,
                "2. Add the Ghostex bot",
                Self::detail("The bot reached this computer"),
                None,
            ));
        } else {
            // CDXC:GrokBot 2026-11-19 DECISION: User: "Please add the link to the Ghostex Grok Bot template into the page", then "actually this is even better link" (grok-bot.directory). Adding the published Ghostex bot is step 2's main way; Copy instructions stays for building your own.
            let mut buttons = h_flex().gap(px(8.0)).child(settings_button(
                p,
                "grok-bot-add-template",
                "Add the Ghostex bot to Grok Bot",
                Some("modals/settings/external-link.svg"),
                ButtonVariant::Default,
                false,
                None,
                |page: &mut Self, _window, cx| page.open_url(TEMPLATE_URL, cx),
                cx,
            ));
            if setup.app == "running" || setup.app == "found" {
                buttons = buttons.child(settings_button(
                    p,
                    "grok-bot-open-app-2",
                    "Open Grok Bot",
                    None,
                    ButtonVariant::Outline,
                    busy,
                    None,
                    |page: &mut Self, _window, cx| page.open_grok_bot(cx),
                    cx,
                ));
            }
            let own = h_flex()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.5))
                        .text_color(hsla(p.muted))
                        .child("Building your own instead? Make a new bot named Ghostex and paste these instructions."),
                )
                .child(settings_button(
                    p,
                    "grok-bot-copy-instructions",
                    copy_label,
                    Some("modals/settings/copy.svg"),
                    ButtonVariant::Outline,
                    false,
                    None,
                    |page: &mut Self, _window, cx| page.copy_instructions(cx),
                    cx,
                ));
            rows.push(settings_list_item(
                p,
                Some(ListItemStatus::Neutral),
                None,
                "2. Add the Ghostex bot",
                Self::detail("Waiting for the bot's first command…"),
                None,
            ));
            rows.push(card_inset(
                v_flex()
                    .w_full()
                    .gap(px(8.0))
                    .child(div().text_size(px(13.0)).child("Add the ready-made Ghostex bot to Grok Bot, then send it any message: it makes its Ghostex reports routine and asks to run ghostex grokbot brief on this computer. Choose Allow once."))
                    .child(buttons)
                    .child(own)
                    .child(div().text_size(px(12.5)).text_color(hsla(p.muted)).child("Nothing happening? In Grok Bot, Settings > Computer > Execution on this computer must be Ask every time (not Never allow).")),
            ));
        }

        // 3. The webhook.
        rows.push(settings_list_item(
            p,
            Some(ListItemStatus::Neutral),
            None,
            "3. Paste the webhook",
            Self::detail(
                "In Grok Bot, open Routines > Ghostex reports and copy each field under Webhook",
            ),
            None,
        ));
        rows.push(card_inset(self.webhook_form(p, window, cx)));

        // 4. The test note, once connected.
        rows.push(settings_list_item(
            p,
            Some(ListItemStatus::Neutral),
            None,
            "4. Check the test note",
            Self::detail("Ghostex sends the bot a test note when it connects."),
            None,
        ));
        rows
    }

    /// Right after connecting here (or after a No): did the test note arrive?
    fn test_question(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let busy = self.busy;
        let mut rows = vec![
            settings_list_item(
                p,
                Some(ListItemStatus::Success),
                None,
                "1. Grok Bot desktop app",
                Self::detail("Done"),
                None,
            ),
            settings_list_item(
                p,
                Some(ListItemStatus::Success),
                None,
                "2. Add the Ghostex bot",
                Self::detail("Done"),
                None,
            ),
            settings_list_item(
                p,
                Some(ListItemStatus::Success),
                None,
                "3. Paste the webhook",
                Self::detail("Connected; the key is saved"),
                None,
            ),
        ];
        let said_no = self
            .setup
            .as_ref()
            .is_some_and(|setup| setup.test_arrived == Some(false));
        let failed = self.test_note.as_ref().is_some_and(|(bad, _)| *bad);
        let (status, detail) = if failed {
            (ListItemStatus::Warning, "The test note didn't go through")
        } else if said_no {
            (ListItemStatus::Warning, "It didn't show up")
        } else {
            (
                ListItemStatus::Neutral,
                "Did \"Ghostex is connected\" show up in the Ghostex bot's chat?",
            )
        };
        rows.push(settings_list_item(
            p,
            Some(status),
            None,
            "4. Check the test note",
            Self::detail(detail),
            None,
        ));
        let mut body = v_flex().w_full().gap(px(8.0));
        if said_no && !failed {
            body = body.child(div().text_size(px(13.0)).child("Check that Ghostex reports isn't paused in Grok Bot and that you're looking at the Ghostex bot's chat, then send it again."));
        }
        body = body.children(Self::note_line(p, &self.test_note));
        let buttons = h_flex()
            .gap(px(8.0))
            .child(settings_button(
                p,
                "grok-bot-test-yes",
                "Yes, it's there",
                None,
                ButtonVariant::Default,
                busy,
                None,
                |page: &mut Self, _window, cx| page.answer_test(true, cx),
                cx,
            ))
            .when(!said_no && !failed, |row| {
                row.child(settings_button(
                    p,
                    "grok-bot-test-no",
                    "No",
                    None,
                    ButtonVariant::Outline,
                    busy,
                    None,
                    |page: &mut Self, _window, cx| page.answer_test(false, cx),
                    cx,
                ))
            })
            .when(said_no || failed, |row| {
                row.child(settings_button(
                    p,
                    "grok-bot-test-again",
                    "Send it again",
                    None,
                    ButtonVariant::Outline,
                    busy,
                    None,
                    |page: &mut Self, _window, cx| page.send_test(cx),
                    cx,
                ))
            });
        rows.push(card_inset(body.child(buttons)));
        // Never a dead end: a wrong address or key is fixed here too, or the connection dropped.
        rows.push(settings_list_item(
            p,
            None,
            None,
            "Wrong address or key?",
            Self::detail(
                "Paste the routine's POST to and key again, or disconnect and start over.",
            ),
            (!self.changing).then(|| {
                h_flex()
                    .gap(px(8.0))
                    .child(settings_button(
                        p,
                        "grok-bot-test-change",
                        "Change",
                        None,
                        ButtonVariant::Outline,
                        busy,
                        None,
                        |page: &mut Self, _window, cx| {
                            page.changing = true;
                            page.connect_note = None;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(settings_button(
                        p,
                        "grok-bot-test-disconnect",
                        "Disconnect",
                        None,
                        ButtonVariant::Destructive,
                        busy,
                        None,
                        |page: &mut Self, _window, cx| page.disconnect(cx),
                        cx,
                    ))
                    .into_any_element()
            }),
        ));
        rows
    }

    /// Connected: notes sent, the webhook, the instructions and Disconnect.
    fn connected_rows(
        &mut self,
        p: &SettingsPalette,
        setup: &Setup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let busy = self.busy;
        let mut rows = Vec::new();
        let sent = match setup.last_sent_at {
            Some(at) => format!(
                "{} {} sent · last one {}",
                setup.sent,
                if setup.sent == 1 { "note" } else { "notes" },
                ago(at)
            ),
            None => "No note sent yet".to_string(),
        };
        let (status, detail) = match &setup.last_error {
            Some(error) => (
                ListItemStatus::Warning,
                format!("{sent} · Last problem: {error}"),
            ),
            None => (ListItemStatus::Success, sent),
        };
        rows.push(settings_list_item(
            p,
            Some(status),
            None,
            "Connected",
            Some(
                v_flex()
                    .gap(px(4.0))
                    .child(detail)
                    .children(Self::note_line(p, &self.test_note))
                    .into_any_element(),
            ),
            Some(settings_button(
                p,
                "grok-bot-send-test",
                "Send a test note",
                None,
                ButtonVariant::Outline,
                busy,
                None,
                |page: &mut Self, _window, cx| page.send_test(cx),
                cx,
            )),
        ));
        rows.push(settings_list_item(
            p,
            None,
            None,
            "Webhook",
            Self::detail(format!("{} · key saved", short_url(&setup.url))),
            (!self.changing).then(|| {
                settings_button(
                    p,
                    "grok-bot-change",
                    "Change",
                    None,
                    ButtonVariant::Outline,
                    busy,
                    None,
                    |page: &mut Self, _window, cx| {
                        page.changing = true;
                        page.connect_note = None;
                        cx.notify();
                    },
                    cx,
                )
            }),
        ));
        if self.changing {
            rows.push(card_inset(self.webhook_form(p, window, cx)));
        }
        let copy_label = if self.copied { "Copied" } else { "Copy" };
        rows.push(settings_list_item(
            p,
            None,
            None,
            "Bot instructions",
            Self::detail(
                "Paste these into the Ghostex bot again when a Ghostex update changes them.",
            ),
            Some(settings_button(
                p,
                "grok-bot-copy",
                copy_label,
                Some("modals/settings/copy.svg"),
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, _window, cx| page.copy_instructions(cx),
                cx,
            )),
        ));
        rows.push(settings_list_item(
            p,
            None,
            None,
            "Disconnect",
            Self::detail("Forgets the address and key. In Grok Bot, also delete the routine and set Execution on this computer to Never allow."),
            Some(settings_button(p, "grok-bot-disconnect", "Disconnect", None, ButtonVariant::Destructive, busy, None, |page: &mut Self, _window, cx| page.disconnect(cx), cx)),
        ));
        rows
    }
}

impl Render for GrokBotTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::GrokBot),
                matching,
            )
        };
        let on = self.enabled(cx);
        let mut blocks = Vec::new();
        // Off, the switch is the whole page, so it shows even when a search doesn't match it.
        if !on || search.row_visible("switch", SWITCH_KEY) {
            let mut rows = vec![self.switch_row(&p, on, cx)];
            if !on {
                rows.push(settings_note_row(&p, "Off: Ghostex sends nothing to Grok Bot and doesn't tell agents about it. This pauses notes only: the bot can still run ghostex commands on this computer through the Grok Bot desktop app until you set Execution on this computer to Never allow in Grok Bot (Settings > Computer)."));
            }
            if let Some(section) = settings_section(&p, "", None, None, rows) {
                let mut block = PageBlock::section("switch", section);
                block.margin_top += PAGE_TOP_PADDING;
                blocks.push(block);
            }
        }
        if on {
            let setup = self.setup.clone();
            let mut rows = Vec::new();
            let mut title = "Set up";
            let mut description: Option<SharedString> = None;
            match &setup {
                None => {
                    if self.read_error.is_none() {
                        rows.push(settings_note_row(&p, "Checking…"));
                    }
                }
                Some(setup)
                    if setup.connected
                        && (self.asking_test || setup.test_arrived == Some(false)) =>
                {
                    description = Some("Step 4 of 4".into());
                    rows = self.test_question(&p, cx);
                    if self.changing {
                        rows.push(card_inset(self.webhook_form(&p, window, cx)));
                    }
                }
                Some(setup) if setup.connected => {
                    title = "Connection";
                    rows = self.connected_rows(&p, setup, window, cx);
                }
                Some(setup) => {
                    let step = if !setup.app_done() {
                        1
                    } else if !setup.brief_seen {
                        2
                    } else {
                        3
                    };
                    description = Some(format!("Control your agents from Grok Bot on your phone, and get their notes there. Step {step} of 4.").into());
                    rows = self.checklist(&p, setup, window, cx);
                }
            }
            if let Some(error) = &self.read_error {
                rows.push(card_inset(
                    div()
                        .text_size(px(12.5))
                        .text_color(hsla(p.destructive))
                        .child(error.clone()),
                ));
            }
            let section_id = if title == "Connection" {
                "connection"
            } else {
                "setup"
            };
            // The checklist and the Connected view are one area in two states: a search that
            // finds any of their rows shows the one this page draws now, so a match never opens
            // an empty page. While the extension is off its `settingKeys` keep them out of search.
            let shown = search.row_visible("connection", "grokBotConnection")
                || [
                    "grokBotDesktopApp",
                    "grokBotMakeBot",
                    "grokBotWebhook",
                    "grokBotTestNote",
                ]
                .iter()
                .any(|key| search.row_visible("setup", key));
            if shown && let Some(section) = settings_section(&p, title, description, None, rows) {
                blocks.push(PageBlock::section(section_id, section));
            }
        }
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::GrokBot,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        settings_page(&self.store, SettingsTabId::GrokBot, &p, blocks, cx)
    }
}

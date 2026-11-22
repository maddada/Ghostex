//! Settings preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (General at the top,
//! the Storybook story's settings), `search` (the query "font", or
//! `GHOSTEX_NATIVE_MODAL_DEMO_QUERY`), `search-other` (a query only
//! other pages match: the no-matches notice), `advanced` (General, as it opens: the advanced rows
//! are folded at the bottom of each page), `terminal` (scrolled to Terminal), `pick-color` (a custom terminal background
//! with its Pick Color dialog open), `select` (the Command Pane Side dropdown open), `tags`
//! (Sidebar Tags with the New tag form, from the sidebar's New tag deep link), `hotkeys` (the
//! Hotkeys entry point), `marked-preview` (the Marked sections style Preview dialog open). Saves are
//! applied back to the modal the way the app rehydrates it.
//! `dictation`, `dictation-hex-missing`, `dictation-openai` and `voice-agent` are Settings › Dictation and
//! Voice Agent (settings_voice.rs).
//! `page-<id>` opens the rail page with that id (`page-connectors`, `page-tools`).
//! `agents-advanced` is Settings › Agents scrolled to its open Advanced fold.
//! `about-reset-confirm` is Settings › About with the Reset all settings? confirmation open.
//! `GHOSTEX_NATIVE_MODAL_DEMO_GLASS=1` previews the frosted palette.
//! `GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS_WIDTH=<points>` opens the dialog narrower or wider than its
//! base 900 (before the app's enlargement), to check a page at another window width.
//! `GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS='{"actionsHidden":false}'` merges saved settings over the
//! state's (a built-in extension switched on or off, for example).
use super::settings_modal::model::SettingsModalHost;
use super::settings_modal::*;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;

#[path = "settings_cloud_boxes.rs"]
mod settings_cloud_boxes;
#[path = "settings_e.rs"]
mod settings_e;
#[path = "settings_f1.rs"]
mod settings_f1;
#[path = "settings_f2.rs"]
mod settings_f2;
mod settings_grok_bot;
#[path = "settings_machines.rs"]
mod settings_machines;
mod settings_secrets;
#[path = "settings_sync.rs"]
mod settings_sync;
#[path = "settings_updates.rs"]
mod settings_updates;
#[path = "settings_voice.rs"]
mod settings_voice;
#[path = "settings_workspaces.rs"]
mod settings_workspaces;

type Slot = Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiSettingsModalWindow>)>>>;

fn story_settings(state: &str) -> serde_json::Map<String, Value> {
    let mut settings = serde_json::Map::new();
    // `modalSettings` of packages/core-ui/settings-modal.stories.tsx (deleted 2026-10-01).
    settings.insert("agentManagerZoomPercent".into(), json!(95));
    settings.insert(
        "sessionCardHoverButtons".into(),
        json!([
            { "enabled": false, "id": "rename" },
            { "enabled": false, "id": "pin" },
            { "enabled": false, "id": "note" },
            { "enabled": false, "id": "snooze" },
            { "enabled": false, "id": "closeAfterDone" },
            { "enabled": false, "id": "tag" },
            { "enabled": false, "id": "park" },
            { "enabled": false, "id": "sleep" },
            { "enabled": true, "id": "chevron" },
            { "enabled": true, "id": "close" }
        ]),
    );
    settings.insert("terminalFontSize".into(), json!(16));
    settings.insert("terminalFontWeight".into(), json!(400));
    settings.insert("terminalLineHeight".into(), json!(1.35));
    if state == "theme-colours" {
        settings.insert("sidebarTheme".into(), json!("system"));
        settings.insert(
            "themeRoles".into(),
            json!({ "dark": { "border": "4%", "divider": "3%" }, "light": { "mutedText": "#6b7280" } }),
        );
    }
    // Orchid Dusk's colours (THEME_GALLERY), Your messages see-through for the slider.
    if matches!(
        state,
        "theme-chat-colours" | "theme-role-picker" | "theme-orchid" | "theme-colours-orchid"
    ) {
        settings.insert("sidebarTheme".into(), json!("dark-2"));
        let user_message = if state == "theme-orchid" {
            "#33283a"
        } else {
            "#33283a9e"
        };
        settings.insert(
            "themeRoles".into(),
            json!({ "dark": {
                "accent": "#d69bb5", "focusRing": "#d69bb5", "border": "8%", "divider": "6%",
                "text": "#ebe3e8", "mutedText": "#9e939c", "userMessage": user_message,
                "chatBox": "#1d181fcc", "surface": "#211b24cc", "control": "#28202bcc",
                "menu": "#1b161df2", "selection": "9%", "hover": "5%", "success": "#9cc49a",
                "warning": "#e0bd84", "danger": "#e2727a"
            } }),
        );
    }
    if state == "pick-color" || state == "theme-terminal-colours" {
        settings.insert("terminalBackgroundMode".into(), json!("custom"));
        settings.insert("workspaceBackgroundColor".into(), json!("#1d3b53"));
    }
    // Themes' Terminal colours with every terminal background row showing: transparency on and a
    // background image picked.
    if state == "theme-terminal-colours" {
        settings.insert("windowGlass".into(), json!("always"));
        settings.insert(
            "terminalBackgroundImage".into(),
            json!("/Users/you/Pictures/background.png"),
        );
    }
    settings_f2::story_settings(state, &mut settings);
    settings_f1::story_settings(state, &mut settings);
    settings_e::story_settings(state, &mut settings);
    settings_workspaces::story_settings(state, &mut settings);
    settings_machines::story_settings(state, &mut settings);
    settings_updates::story_settings(state, &mut settings);
    if let Ok(Value::Object(extra)) = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS")
        .map_err(|_| ())
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|_| ()))
    {
        settings.extend(extra);
    }
    settings
}

fn sidebar_state(settings: &serde_json::Map<String, Value>) -> Value {
    let mut message = json!({
        "customSessionTags": {
            "order": ["custom-demo0001"],
            "tags": {
                "custom-demo0001": { "color": "#59d9ff", "icon": "rocket", "name": "Launch", "tagId": "custom-demo0001" }
            }
        },
        "hud": { "settings": Value::Object(settings.clone()) },
        "revision": 1,
        "type": "hydrate",
    });
    settings_f2::extend_sidebar_state(&mut message);
    settings_f1::extend_sidebar_state(&mut message);
    settings_e::extend_sidebar_state(&mut message);
    message
}

thread_local! {
    /// Answers to posts sent while the window was still opening (Settings > Remote asks for the
    /// machines' connection states on its first frame); `open` hands them over once it has the window.
    static EARLY_PAYLOADS: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

/// Hands a host payload (`agentHookStatus`, `remoteGxserverInstallState`, ...) to the open modal.
fn deliver(slot: &Slot, payload: Value, cx: &mut App) {
    let target = slot.borrow().clone();
    let Some((window, view)) = target else {
        EARLY_PAYLOADS.with(|early| early.borrow_mut().push(payload));
        return;
    };
    cx.defer(move |cx| {
        let _ = window.update(cx, |_, _, cx| {
            view.update(cx, |modal, cx| modal.receive_host_payload(payload, cx));
        });
    });
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let settings = Rc::new(RefCell::new(story_settings(&state)));
    let slot: Slot = Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let host_settings = settings.clone();
    let host_state = state.clone();
    let host: SettingsModalHost = Rc::new(move |command, cx: &mut App| {
        let apply = |patch: &serde_json::Map<String, Value>, replace: bool, cx: &mut App| {
            {
                let mut settings = host_settings.borrow_mut();
                if replace {
                    *settings = patch.clone();
                } else {
                    for (key, value) in patch {
                        settings.insert(key.clone(), value.clone());
                    }
                }
            }
            let message = sidebar_state(&host_settings.borrow());
            let target = host_slot.borrow().clone();
            if let Some((window, view)) = target {
                cx.defer(move |cx| {
                    let _ = window.update(cx, |_, _, cx| {
                        view.update(cx, |modal, cx| modal.receive_sidebar_state(message, cx));
                    });
                });
            }
        };
        match command {
            SettingsModalCommand::SavePatch { patch, source } => {
                eprintln!("save patch ({source}): {}", Value::Object(patch.clone()));
                apply(&patch, false, cx);
            }
            SettingsModalCommand::SaveSettings { settings, source } => {
                eprintln!("save settings ({source}): {} keys", settings.len());
                apply(&settings, true, cx);
            }
            SettingsModalCommand::PostMessage(message) => {
                eprintln!("post: {message}");
                for payload in settings_f2::answers(&message) {
                    deliver(&host_slot, payload, cx);
                }
                for payload in settings_e::answers(&message) {
                    deliver(&host_slot, payload, cx);
                }
                for payload in settings_machines::answers(&message) {
                    deliver(&host_slot, payload, cx);
                }
                for payload in settings_updates::answers(&host_state, &message) {
                    deliver(&host_slot, payload, cx);
                }
                for (delay, payload) in settings_voice::script(&host_state, &message) {
                    if delay == 0 {
                        deliver(&host_slot, payload, cx);
                        continue;
                    }
                    let slot = host_slot.clone();
                    cx.spawn(async move |cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(delay))
                            .await;
                        let _ = cx.update(|cx| deliver(&slot, payload, cx));
                    })
                    .detach();
                }
            }
            // The system colour panel belongs to the app host (settings_modal_lifecycle.rs).
            SettingsModalCommand::PickSystemColor { key, initial } => {
                eprintln!("system colour panel: {key} from {initial}")
            }
            SettingsModalCommand::ApproveRequest { id, reply } => {
                eprintln!("approve request {id} (Touch ID in the app)");
                cx.defer(move |cx| reply(Ok(serde_json::json!({ "approved": true })), cx));
            }
            SettingsModalCommand::GxserverRpc {
                path,
                params,
                reply,
                ..
            } => {
                eprintln!("gxserver rpc: {path} {params}");
                if settings_machines::forwards_accounts(&host_state, &path) {
                    let background = cx.background_executor().clone();
                    cx.spawn(async move |cx| {
                        let result = background
                            .spawn(
                                async move { settings_machines::forward_accounts(false, &params) },
                            )
                            .await;
                        let _ = cx.update(|cx| reply(result, cx));
                    })
                    .detach();
                    return;
                }
                if let Some(result) = orchestrator_playbook_rpc(&host_state, &path, &params) {
                    cx.defer(move |cx| reply(result, cx));
                    return;
                }
                if host_state == "collab"
                    && let Some(result) = settings_sync::collab_stand_in(&path, &params)
                {
                    cx.defer(move |cx| reply(Ok(result), cx));
                    return;
                }
                if settings_sync::owns(&host_state) {
                    let result = settings_sync::rpc(&path, &params);
                    cx.defer(move |cx| reply(result, cx));
                    return;
                }
                // This computer's Dictation under the machine picker stories.
                let voice_story = if host_state == "machines-dictation-open" {
                    "dictation"
                } else {
                    host_state.as_str()
                };
                if settings_voice::owns(voice_story)
                    && let Some(result) = settings_voice::rpc(voice_story, &path, &params)
                {
                    cx.defer(move |cx| reply(result, cx));
                    return;
                }
                if settings_secrets::owns(&host_state)
                    && let Some(result) = settings_secrets::rpc(&host_state, &path, &params)
                {
                    cx.defer(move |cx| reply(result, cx));
                    return;
                }
                if settings_grok_bot::owns(&host_state)
                    && let Some(result) = settings_grok_bot::rpc(&host_state, &path, &params)
                {
                    cx.defer(move |cx| reply(result, cx));
                    return;
                }
                if settings_workspaces::owns(&host_state)
                    && let Some(result) = settings_workspaces::rpc(&host_state, &path, &params)
                {
                    cx.defer(move |cx| reply(result, cx));
                    return;
                }
                if let Some(answer) = settings_cloud_boxes::rpc(&host_state, &path, &params) {
                    if let Some(result) = answer {
                        cx.defer(move |cx| reply(result, cx));
                    }
                    return;
                }
                let result = match settings_f1::rpc(&host_state, &path, &params) {
                    settings_f1::Answer::Never => return,
                    settings_f1::Answer::Reply(result) => result,
                    settings_f1::Answer::NotMine => settings_f2::rpc(&path, &params),
                };
                cx.defer(move |cx| reply(result, cx));
            }
            SettingsModalCommand::RemoteGxserverRpc {
                machine_id,
                path,
                params,
                reply,
                ..
            } => {
                eprintln!("remote gxserver rpc ({machine_id}): {path} {params}");
                if settings_machines::forwards_extensions(&host_state, &path) {
                    let background = cx.background_executor().clone();
                    cx.spawn(async move |cx| {
                        let result = background
                            .spawn(async move { settings_machines::forward_extensions(&params) })
                            .await;
                        let _ = cx.update(|cx| reply(result, cx));
                    })
                    .detach();
                    return;
                }
                if settings_machines::forwards_accounts(&host_state, &path) {
                    let background = cx.background_executor().clone();
                    cx.spawn(async move |cx| {
                        let result = background
                            .spawn(
                                async move { settings_machines::forward_accounts(true, &params) },
                            )
                            .await;
                        let _ = cx.update(|cx| reply(result, cx));
                    })
                    .detach();
                    return;
                }
                let result = settings_machines::remote_rpc(&host_state, &path, &params);
                cx.defer(move |cx| reply(result, cx));
            }
            SettingsModalCommand::CopyToClipboard(text) => eprintln!("copy: {text}"),
            SettingsModalCommand::HttpGet { url, reply } => {
                eprintln!("http get: {url}");
                let result = settings_f1::http_get(&url);
                cx.defer(move |cx| reply(result, cx));
            }
            SettingsModalCommand::HostMessage(message) => eprintln!("host message: {message}"),
            SettingsModalCommand::Toast {
                level,
                title,
                description,
            } => {
                eprintln!("toast {level}: {title} - {description}")
            }
            SettingsModalCommand::Close => {
                eprintln!("close");
                cx.quit();
            }
            SettingsModalCommand::OpenThemeEditor => eprintln!("open theme editor"),
            SettingsModalCommand::OpenThemeSettings => eprintln!("open theme settings"),
            SettingsModalCommand::OpenAccounts => eprintln!("open accounts"),
            SettingsModalCommand::SaveOpenForms(stored) => eprintln!("open forms: {stored:?}"),
        }
    });
    let open_message = match state.as_str() {
        "search" => {
            let query = super::env("GHOSTEX_NATIVE_MODAL_DEMO_QUERY");
            json!({ "initialSearchQuery": if query.is_empty() { "font".to_string() } else { query } })
        }
        "search-other" => json!({ "initialSearchQuery": "tailscale" }),
        // `orchestrator-playbook…-row`: searched for, so the Playbook overrides row shows.
        state if state.starts_with("orchestrator-playbook") && state.ends_with("-row") => {
            json!({ "initialSearchQuery": "playbook" })
        }
        state if state.starts_with("orchestrator-playbook") => {
            json!({ "initialSection": "chat" })
        }
        "terminal" | "select" => json!({ "initialSection": "terminal" }),
        // Themes' Terminal colours with the terminal background's Pick Color dialog open.
        "pick-color" | "theme-terminal-colours" => {
            json!({ "initialTab": "theme", "initialSection": "terminalColours" })
        }
        // Settings > Security scrolled to its log (with `{"securityLogsEnabled":true}`).
        "security-log" => json!({ "initialTab": "security", "initialSection": "log" }),
        // Settings > About with its Reset all settings? confirmation open.
        "about-reset-confirm" => json!({ "initialTab": "about" }),
        // Settings > Interface scrolled to Theme colours, with a few colours set and More open.
        "theme-colours" => json!({ "initialTab": "theme", "initialSection": "themeColours" }),
        // Settings > Interface with Orchid Dusk's colours: Chat colours (and its Pick Color
        // dialog), the gallery's applied line at the top, Theme colours with More open by itself.
        "theme-chat-colours" | "theme-role-picker" => {
            json!({ "initialTab": "theme", "initialSection": "chatColours" })
        }
        "theme-orchid" => json!({ "initialTab": "theme" }),
        "theme-colours-orchid" => {
            json!({ "initialTab": "theme", "initialSection": "themeColours" })
        }
        // `page-<id>`: that rail page (`page-connectors`, `page-tools`), for pages with no state of their own.
        // The state arrives lowercased (`env`), so `page-textfonts` finds `textFonts`.
        state if state.starts_with("page-") => {
            let wanted = &state["page-".len()..];
            let id = crate::settings_modal::model::SettingsTabId::RAIL_ORDER
                .iter()
                .map(|tab| tab.id())
                .find(|id| id.eq_ignore_ascii_case(wanted))
                .unwrap_or(wanted);
            json!({ "initialTab": id })
        }
        "tags" => {
            json!({ "initialSection": "sidebarTags", "initialSidebarTagsAction": "createTag" })
        }
        // Settings > Agents scrolled to its Advanced fold, opened by the deep link.
        "agents-advanced" => {
            json!({ "initialTab": "agents", "initialSection": "agentsAdvanced" })
        }
        _ => settings_updates::open_message(&state)
            .or_else(|| settings_machines::open_message(&state))
            .or_else(|| settings_workspaces::open_message(&state))
            .or_else(|| settings_f1::open_message(&state))
            .or_else(|| settings_sync::open_message(&state))
            .or_else(|| settings_cloud_boxes::open_message(&state))
            .or_else(|| settings_secrets::open_message(&state))
            .or_else(|| settings_grok_bot::open_message(&state))
            .or_else(|| settings_voice::open_message(&state))
            .or_else(|| settings_f2::open_message(&state))
            .or_else(|| settings_e::open_message(&state))
            .unwrap_or_else(|| json!({})),
    };
    let modal_id = if state == "hotkeys" {
        "hotkeys"
    } else {
        "settings"
    };
    let mut request = SettingsOpenRequest::from_open_message(modal_id, &open_message);
    if state == "pick-color" {
        request.open_color_picker = Some("workspaceBackgroundColor".to_string());
    }
    if state == "select" {
        request.open_select = Some("terminalCursorStyle".to_string());
    }
    if state == "updates-computers-open" {
        request.open_select = Some("settings-machine-picker".to_string());
    }
    if state == "machines-agents-open"
        || state == "machines-dictation-open"
        || state == "machines-extensions-open"
    {
        request.open_select = Some("settings-machine-picker".to_string());
    }
    request.gxserver_rpc_available = if settings_sync::owns(&state) {
        settings_sync::gxserver_rpc_available()
    } else if settings_workspaces::owns(&state)
        || settings_machines::owns(&state)
        || settings_secrets::owns(&state)
        || settings_grok_bot::owns(&state)
        || settings_voice::owns(&state)
    {
        true
    } else if settings_cloud_boxes::owns(&state) {
        settings_cloud_boxes::gxserver_rpc_available(&state)
    } else {
        settings_f2::gxserver_rpc_available(&state) || settings_f1::gxserver_rpc_available(&state)
    };
    request.preview_state = (!state.is_empty()).then(|| state.clone());
    // `GHOSTEX_NATIVE_MODAL_DEMO_GLASS=1`: the frosted palette the app passes under window glass
    // (a translucent fill stands in for the blurred one).
    let palette = if super::env("GHOSTEX_NATIVE_MODAL_DEMO_GLASS") == "1" {
        let fill = super::native_modal_kit::rgba_of(demo.palette.surface, 0.72);
        demo.palette.frosted(fill)
    } else {
        demo.palette
    };
    let config = SettingsModalConfig {
        palette,
        request,
        sidebar_state: sidebar_state(&settings.borrow()),
        open_forms: serde_json::Value::Null,
    };
    let width = super::env("GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS_WIDTH")
        .parse::<f32>()
        .unwrap_or(SETTINGS_MODAL_WIDTH);
    // `GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS_HEIGHT=<points>`: taller, so a whole page fits one capture.
    let height = super::env("GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS_HEIGHT")
        .parse::<f32>()
        .unwrap_or(SETTINGS_MODAL_HEIGHT);
    let (window, view) = super::open_settings_dialog_window(
        width,
        height,
        move |window, cx| cx.new(|cx| GpuiSettingsModalWindow::new(config, host, window, cx)),
        cx,
    );
    settings_f2::after_open(window, cx);
    *slot.borrow_mut() = Some((window, view));
    for payload in EARLY_PAYLOADS.with(|early| std::mem::take(&mut *early.borrow_mut())) {
        deliver(&slot, payload, cx);
    }
}

/// Standalone preview of the compact draggable floating Theme Editor (`GHOSTEX_NATIVE_MODAL_DEMO=theme-editor`),
/// rendered at its real 320x640 size floating over a 1650x962 app-window backdrop so offscreen
/// screenshots show its frosted glass surface and live layout in both dark glass and light mode.
pub(super) fn open_theme_editor(demo: &super::DemoEnv, cx: &mut App) {
    use gpui::{
        AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        Window, div, px,
    };

    struct FloatingThemeEditorStage {
        editor: Entity<GpuiThemeEditorWindow>,
        light: bool,
    }

    impl Render for FloatingThemeEditorStage {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let (bg_a, bg_b, sb_bg, wa_bg, line, fg, muted, card_bg) = if self.light {
                (
                    gpui::rgb(0xdbe3ee),
                    gpui::rgb(0xeae3ef),
                    gpui::Rgba { r: 0.92, g: 0.93, b: 0.95, a: 0.90 },
                    gpui::Rgba { r: 0.96, g: 0.97, b: 0.98, a: 0.88 },
                    gpui::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.10 },
                    gpui::rgb(0x1f232b),
                    gpui::rgb(0x5b6275),
                    gpui::Rgba { r: 1.0, g: 1.0, b: 1.0, a: 0.72 },
                )
            } else {
                (
                    gpui::rgb(0x1b2b34),
                    gpui::rgb(0x282034),
                    gpui::Rgba { r: 0.08, g: 0.10, b: 0.12, a: 0.78 },
                    gpui::Rgba { r: 0.10, g: 0.12, b: 0.14, a: 0.74 },
                    gpui::Rgba { r: 1.0, g: 1.0, b: 1.0, a: 0.08 },
                    gpui::rgb(0xe8ebf2),
                    gpui::rgb(0x98a0b2),
                    gpui::Rgba { r: 0.16, g: 0.19, b: 0.22, a: 0.72 },
                )
            };
            div()
                .relative()
                .size_full()
                .overflow_hidden()
                .bg(gpui::linear_gradient(
                    135.0,
                    gpui::linear_color_stop(bg_a, 0.0),
                    gpui::linear_color_stop(bg_b, 1.0),
                ))
                // Simulated main Ghostex window behind the floating editor
                .child(
                    div()
                        .size_full()
                        .flex()
                        .child(
                            div()
                                .w(px(300.0))
                                .h_full()
                                .bg(sb_bg)
                                .border_r_1()
                                .border_color(line)
                                .p(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .text_color(fg)
                                        .child("Ghostex"),
                                )
                                .child(
                                    div()
                                        .px(px(10.0))
                                        .py(px(8.0))
                                        .rounded(px(8.0))
                                        .bg(card_bg)
                                        .border_1()
                                        .border_color(line)
                                        .text_size(px(12.5))
                                        .text_color(fg)
                                        .child("Build floating theme editor"),
                                )
                                .child(
                                    div()
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .text_size(px(12.5))
                                        .text_color(muted)
                                        .child("Subtle dark tints"),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h_full()
                                .bg(wa_bg)
                                .p(px(40.0))
                                .flex()
                                .flex_col()
                                .justify_end()
                                .gap(px(14.0))
                                .child(
                                    div()
                                        .w(px(680.0))
                                        .p(px(16.0))
                                        .rounded(px(12.0))
                                        .bg(card_bg)
                                        .border_1()
                                        .border_color(line)
                                        .text_size(px(13.5))
                                        .text_color(fg)
                                        .child("The main Ghostex window updates live while you drag and adjust the floating Theme Editor."),
                                )
                                .child(
                                    div()
                                        .w(px(680.0))
                                        .h(px(96.0))
                                        .p(px(16.0))
                                        .rounded(px(14.0))
                                        .bg(card_bg)
                                        .border_1()
                                        .border_color(line)
                                        .text_size(px(13.0))
                                        .text_color(muted)
                                        .child("Ask an agent to build something..."),
                                ),
                        ),
                )
                // Compact floating Theme Editor window positioned at top-right
                .child(
                    div()
                        .absolute()
                        .top(px(56.0))
                        .right(px(36.0))
                        .w(px(THEME_EDITOR_WIDTH))
                        .h(px(THEME_EDITOR_HEIGHT))
                        .rounded(px(12.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(line)
                        .shadow(vec![gpui::BoxShadow {
                            color: gpui::Rgba {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.42,
                            }
                            .into(),
                            offset: gpui::point(px(0.0), px(18.0)),
                            blur_radius: px(46.0),
                            spread_radius: px(0.0),
                            inset: false,
                        }])
                        .child(self.editor.clone()),
                )
        }
    }

    let state = demo.state.clone();
    let mut initial = story_settings(&state);
    if demo.palette.light {
        initial.insert("sidebarTheme".into(), json!("plain-light"));
    } else {
        initial.insert("sidebarTheme".into(), json!("dark-2"));
        initial.entry("windowArt".to_string()).or_insert_with(|| json!("moonwater"));
        initial.entry("windowGlass".to_string()).or_insert_with(|| json!("frosted"));
    }
    let palette = if super::env("GHOSTEX_NATIVE_MODAL_DEMO_GLASS") == "1" {
        let base = if demo.palette.light {
            gpui::Rgba { r: 0.96, g: 0.97, b: 0.98, a: 0.86 }
        } else {
            gpui::Rgba { r: 0.13, g: 0.15, b: 0.18, a: 0.84 }
        };
        demo.palette.frosted_dialog(base)
    } else {
        demo.palette
    };
    let request = SettingsOpenRequest::from_open_message(
        "settings",
        &json!({ "initialTab": "theme" }),
    );
    let config = SettingsModalConfig {
        palette,
        request,
        sidebar_state: sidebar_state(&initial),
        open_forms: Value::Null,
    };
    let host: SettingsModalHost = Rc::new(|command, cx| match command {
        SettingsModalCommand::Close => cx.quit(),
        SettingsModalCommand::OpenThemeSettings => eprintln!("open theme settings"),
        _ => {}
    });
    let width = super::env("GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS_WIDTH")
        .parse::<f32>()
        .unwrap_or(1650.0);
    let height = super::env("GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS_HEIGHT")
        .parse::<f32>()
        .unwrap_or(962.0);
    let light = demo.palette.light;
    let open_picker = state == "picker" || state == "theme-editor-picker";
    let _ = super::open_modal_window(
        width,
        height,
        move |window, cx| {
            let editor = cx.new(|cx| {
                let mut view = GpuiThemeEditorWindow::new(config, host, window, cx);
                if open_picker {
                    view.open_custom_color_picker_for_demo(!light, window, cx);
                }
                view
            });
            cx.new(|_| FloatingThemeEditorStage { editor, light })
        },
        cx,
    );
}

thread_local! {
    /// The scripted playbook overrides and whether an old full copy is still stored, seeded from
    /// the state on first use (`…-old…`: an edited full playbook for all projects saved before
    /// overrides; `…-overrides…`: overrides written).
    static ORCHESTRATOR_PLAYBOOK: std::cell::RefCell<Option<(String, bool)>> =
        const { std::cell::RefCell::new(None) };
}

/// The scripted gxserver's `/api/readCoordinatorPlaybook` and `/api/setCoordinatorPlaybook`
/// (states `orchestrator-playbook`, `orchestrator-playbook-edit`,
/// `orchestrator-playbook-overrides-edit`, `orchestrator-playbook-old-edit`,
/// `orchestrator-playbook-old-armed`, `orchestrator-playbook-view`,
/// `orchestrator-playbook-overrides-view`), and `/api/readCoordinatorGuide`.
fn orchestrator_playbook_rpc(
    state: &str,
    path: &str,
    params: &Value,
) -> Option<Result<Value, String>> {
    if path == "/api/readCoordinatorGuide" {
        return Some(Ok(
            json!({ "guide": super::new_coordinator_demo::demo_guide() }),
        ));
    }
    if !matches!(
        path,
        "/api/readCoordinatorPlaybook" | "/api/setCoordinatorPlaybook"
    ) {
        return None;
    }
    ORCHESTRATOR_PLAYBOOK.with(|stored| {
        let mut stored = stored.borrow_mut();
        let (overrides, old) = stored.get_or_insert_with(|| {
            let overrides = if state.contains("overrides") {
                "Never push without asking.\nIgnore the board rules.\nThreads never restart the app."
            } else {
                ""
            };
            (overrides.to_string(), state.contains("-old"))
        });
        let mut saved = Vec::new();
        if path == "/api/setCoordinatorPlaybook" {
            if let Some(text) = params["overrides"].as_str() {
                *overrides = text.trim().to_string();
            }
            if params["retireOldCopies"] == true && *old {
                *old = false;
                saved.push("~/.ghostex/coordinators/old-playbooks/all-projects-2026-11-07.md");
            }
        }
        let mut view = json!({
            "source": if *old { "allProjects" } else { "shipped" },
            "overrides": { "text": overrides.clone() },
            "oldCopies": if *old {
                json!([{ "scope": "allProjects", "enabled": true }])
            } else {
                json!([])
            },
        });
        if path == "/api/setCoordinatorPlaybook" && params.get("retireOldCopies").is_some() {
            view["savedFiles"] = json!(saved);
        }
        Some(Ok(view))
    })
}

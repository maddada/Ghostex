//! The Grok view's logins: which Grok login's browser profile the page shows, the login bar above
//! the page, switching, and adding another login.
//!
//! CDXC:Extensions 2026-11-09 DECISION:
//! User: "add an extension to the app like the claude one that opens different claude accounts but this one is to open different grok bot logins". Claude's profiles are the saved Claude accounts of Settings > Accounts (claude_web/accounts.rs); Ghostex saves no Grok accounts, so Grok's logins are its own: `grokWebLogins`, each `{ "id", "name" }` on a disk-backed browser profile of its own (`workspace-grok-…`, its own cookies and storage), so one login's grok.com sign-in never reaches another's. An empty list is one login named Grok, so turning Grok on needs no setup. The bar above the page picks a login and adds another (named Grok 2, Grok 3, …); Settings > Files & Links > Browser renames and removes them (the bar has no text box to rename in). A removed login's profile is cleared the next time the Grok page opens, because the Settings window cannot reach the browser engine. The picked login is remembered across restarts in `grok-web-login.json` beside Claude's `claude-web-account.json`; switching reopens the page on the other login's profile. Unlike claude.ai, grok.com's page is not read, so the bar shows no signed-in dot or email.
//! SEE-ALSO: claude_web/place.rs (Claude or Grok), claude_web/view.rs (`grok_web_start_page`), app/window/settings_modal/fields/grok_logins.rs (the Settings rows).

use std::cell::RefCell;
use std::time::Duration;

use gpui::{Action, SharedString};
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use serde_json::{Map, Value, json};

use super::place::WebPlace;
use super::view::grok_web_start_page;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::*;

const SELECTION_FILE: &str = "grok-web-login.json";
const LOGINS_SETTING: &str = "grokWebLogins";
/// The login an empty `grokWebLogins` stands for.
const DEFAULT_LOGIN_ID: &str = "default";
const DEFAULT_LOGIN_NAME: &str = "Grok";

#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SelectGrokWebLogin {
    pub(crate) login_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct AddGrokWebLogin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ManageGrokWebLogins;

/// A Grok login: a name and its own browser profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GrokLogin {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// What `grok-web-login.json` holds: the picked login, and every login whose profile Ghostex has
/// opened, to clear the profiles of logins removed since.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GrokSelection {
    picked: Option<String>,
    opened: Vec<String>,
}

thread_local! {
    static SELECTION: RefCell<Option<GrokSelection>> = const { RefCell::new(None) };
    /// The profile the open page was opened on, to notice when the pick changes under it.
    static OPENED_ON: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The logins `grokWebLogins` lists: unique ids, a name for each (`Grok N` for a blank one), and
/// the one default login when it lists none.
pub(crate) fn grok_logins_from_setting(value: Option<&Value>) -> Vec<GrokLogin> {
    let mut logins: Vec<GrokLogin> = Vec::new();
    for entry in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = entry["id"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        if logins.iter().any(|login| login.id == id) {
            continue;
        }
        let name = entry["name"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| numbered_name(logins.len() + 1));
        logins.push(GrokLogin {
            id: id.to_string(),
            name,
        });
    }
    if logins.is_empty() {
        logins.push(GrokLogin {
            id: DEFAULT_LOGIN_ID.into(),
            name: DEFAULT_LOGIN_NAME.into(),
        });
    }
    logins
}

fn numbered_name(number: usize) -> String {
    if number <= 1 {
        DEFAULT_LOGIN_NAME.to_string()
    } else {
        format!("{DEFAULT_LOGIN_NAME} {number}")
    }
}

/// `logins` as `grokWebLogins` stores them.
pub(crate) fn grok_logins_setting_value(logins: &[GrokLogin]) -> Value {
    Value::Array(
        logins
            .iter()
            .map(|login| json!({ "id": login.id, "name": login.name }))
            .collect(),
    )
}

/// `logins` with one more, named after the first `Grok N` no login uses.
pub(crate) fn grok_logins_with_another(logins: &[GrokLogin], id: String) -> Vec<GrokLogin> {
    let name = (logins.len() + 1..)
        .map(numbered_name)
        .find(|name| logins.iter().all(|login| &login.name != name))
        .unwrap_or_else(|| numbered_name(logins.len() + 1));
    let mut next = logins.to_vec();
    next.push(GrokLogin { id, name });
    next
}

/// The disk-backed browser profile of one Grok login (a `workspace-` id, so its own cookie store:
/// cef/shell/workspace_browser.rs).
pub(crate) fn grok_login_browser_profile(login_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(login_id.trim().as_bytes()));
    format!("workspace-grok-{}", &digest[..24])
}

fn saved_logins() -> Vec<GrokLogin> {
    grok_logins_from_setting(
        shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get(LOGINS_SETTING),
    )
}

fn selection_file() -> std::path::PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join(SELECTION_FILE)
}

fn with_selection<T>(f: impl FnOnce(&mut GrokSelection) -> T) -> T {
    SELECTION.with(|selection| {
        let mut selection = selection.borrow_mut();
        let selection = selection.get_or_insert_with(|| {
            let saved = std::fs::read_to_string(selection_file())
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .unwrap_or(Value::Null);
            GrokSelection {
                picked: saved["loginId"].as_str().map(str::to_string),
                opened: saved["opened"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            }
        });
        f(selection)
    })
}

fn save_selection(selection: &GrokSelection) {
    let path = selection_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        path,
        json!({ "loginId": selection.picked, "opened": selection.opened }).to_string(),
    );
}

/// The login the page shows: the picked one while it is still listed, else the first.
pub(crate) fn choose_grok_login(picked: Option<&str>, logins: &[GrokLogin]) -> GrokLogin {
    picked
        .and_then(|picked| logins.iter().find(|login| login.id == picked))
        .or_else(|| logins.first())
        .cloned()
        .unwrap_or_else(|| GrokLogin {
            id: DEFAULT_LOGIN_ID.into(),
            name: DEFAULT_LOGIN_NAME.into(),
        })
}

fn current_login() -> GrokLogin {
    let logins = saved_logins();
    let picked = with_selection(|selection| selection.picked.clone());
    choose_grok_login(picked.as_deref(), &logins)
}

/// A new login id, unique enough for one person's few logins.
fn new_login_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

impl GhostexGpuiApp {
    /// The browser profile the Grok page opens on: the picked login's. Clears first the profiles of
    /// logins removed in Settings since they were last opened.
    pub(crate) fn grok_web_profile(&mut self) -> String {
        let logins = saved_logins();
        let login = current_login();
        let removed = with_selection(|selection| {
            let (kept, removed): (Vec<String>, Vec<String>) = std::mem::take(&mut selection.opened)
                .into_iter()
                .partition(|id| logins.iter().any(|login| &login.id == id));
            selection.opened = kept;
            if !selection.opened.contains(&login.id) {
                selection.opened.push(login.id.clone());
            }
            save_selection(selection);
            removed
        });
        for id in removed {
            // Signs the removed login out of everything (its cookies, storage and cache).
            if let Err(error) =
                crate::cef::clear_workspace_browser_profile(&grok_login_browser_profile(&id))
            {
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.grokWeb.clearRemovedLogin.failed",
                    json!({ "error": error.to_string() }),
                );
            }
        }
        let profile = grok_login_browser_profile(&login.id);
        OPENED_ON.with(|opened| *opened.borrow_mut() = Some(profile.clone()));
        profile
    }

    /// The open Grok page shows another login's profile than the one picked now (its login was
    /// removed or the pick changed elsewhere): reopens it, after this render.
    pub(crate) fn sync_grok_web_page_if_moved(&mut self, cx: &mut gpui::Context<Self>) {
        let wanted = grok_login_browser_profile(&current_login().id);
        let moved = OPENED_ON.with(|opened| {
            opened
                .borrow()
                .as_ref()
                .is_some_and(|opened| *opened != wanted)
        });
        if moved {
            let app = cx.entity();
            cx.defer(move |cx| app.update(cx, |this, cx| this.sync_grok_web_page(cx)));
        }
    }

    /// Reopens the page when its login changed under it.
    fn sync_grok_web_page(&mut self, cx: &mut gpui::Context<Self>) {
        let slot = ProjectWorkareaCefSurfaceSlotKey::Extension(WebPlace::Grok.view_id());
        let wanted = grok_login_browser_profile(&current_login().id);
        let opened = OPENED_ON.with(|opened| opened.borrow().clone());
        if opened.is_some_and(|opened| opened != wanted) {
            self.remove_project_workarea_runtime_cef_surface(slot, cx);
            self.parked_project_workarea_surfaces
                .retain(|parked| parked.slot_key != slot);
            OPENED_ON.with(|opened| *opened.borrow_mut() = None);
        }
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        cx.notify();
    }

    /// A row of the bar's login menu.
    pub(crate) fn select_grok_web_login(&mut self, login_id: &str, cx: &mut gpui::Context<Self>) {
        with_selection(|selection| {
            selection.picked = Some(login_id.to_string());
            save_selection(selection);
        });
        self.sync_grok_web_page(cx);
    }

    /// The menu's Add another login: saves a new login and opens the page on it, signed out.
    pub(crate) fn add_grok_web_login(&mut self, cx: &mut gpui::Context<Self>) {
        let id = new_login_id();
        let logins = grok_logins_with_another(&saved_logins(), id.clone());
        let mut patch = Map::new();
        patch.insert(
            LOGINS_SETTING.to_string(),
            grok_logins_setting_value(&logins),
        );
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({ "patch": patch, "source": "grokWeb:addLogin" }),
            cx,
        );
        self.select_grok_web_login(&id, cx);
    }

    /// The menu's Manage logins…: Settings > Files & Links, whose Browser section has the rows.
    pub(crate) fn manage_grok_web_logins(&mut self, cx: &mut gpui::Context<Self>) {
        self.open_gpui_settings_tab_from_new_thread_picker("tools", cx);
    }

    fn show_grok_web_login_menu(
        &mut self,
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let current = current_login();
        let mut menu = GpuiContextMenu::new();
        for login in saved_logins() {
            menu = menu.menu_with_check(
                login.name.clone(),
                login.id == current.id,
                Box::new(SelectGrokWebLogin {
                    login_id: login.id.clone(),
                }),
            );
        }
        menu.separator()
            .menu("Add another login", Box::new(AddGrokWebLogin))
            .menu("Manage logins…", Box::new(ManageGrokWebLogins))
            .toggle_below(trigger_bounds, window, cx);
    }

    /// The bar above the Grok page: the login picker on the left (logo, name), Home (the start
    /// page) and Reload on the right.
    pub(crate) fn render_grok_web_login_bar(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let mode = WebPlace::Grok.mode();
        let muted: Hsla = chrome_color(0xb7b7b7, 0x525252).into();
        let login = current_login();
        let trigger_bounds = std::rc::Rc::new(std::cell::Cell::new(None));
        let picker = h_flex()
            .id("ghostex-gpui-grok-web-login-picker")
            .role(gpui::Role::Button)
            .aria_label("Switch Grok login")
            .min_w_0()
            .flex_shrink(1.0)
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .px(px(8.0))
            .gap(px(7.0))
            .items_center()
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .cursor_default()
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_prepaint({
                let trigger_bounds = trigger_bounds.clone();
                move |bounds, _, _| trigger_bounds.set(Some(bounds))
            })
            .child(titlebar_svg_icon(
                WebPlace::Grok.icon(),
                13.0,
                titlebar_icon_color(),
            ))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(titlebar_active_text_color())
                    .child(login.name),
            )
            .child(titlebar_svg_icon(TITLEBAR_ICON_CHEVRON_DOWN, 12.0, muted))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if let Some(bounds) = trigger_bounds.get() {
                        this.show_grok_web_login_menu(bounds, window, cx);
                    }
                }),
            );
        let bar_button = |id: &'static str, label: &'static str, icon: &'static str| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .flex()
                .flex_shrink_0()
                .h(px(TITLEBAR_CONTROL_HEIGHT))
                .w(px(30.0))
                .items_center()
                .justify_center()
                .rounded(px(TITLEBAR_BUTTON_RADIUS))
                .cursor_default()
                .hover(|this| this.bg(titlebar_button_hover_color()))
                .child(titlebar_svg_icon(icon, 14.0, titlebar_icon_color()))
                .managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Auto,
                    Duration::from_millis(300),
                    move |window, cx| titlebar_tooltip(label, window, cx),
                )
        };
        let home = bar_button(
            "ghostex-gpui-grok-web-home",
            "Go to the start page",
            BROWSER_ICON_HOME,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                let start = grok_web_start_page(
                    shared_settings::shared_sidebar_settings_snapshot()
                        .object()
                        .get("grokWebStartPage")
                        .and_then(Value::as_str),
                );
                let slot = ProjectWorkareaCefSurfaceSlotKey::Extension(WebPlace::Grok.view_id());
                if let Some(surface) = this
                    .project_workarea_runtime_cef_surfaces
                    .get(&slot)
                    .map(|entry| entry.surface.clone())
                {
                    surface.update(cx, |surface, _| surface.load_url(&start));
                }
            }),
        );
        let reload = bar_button(
            "ghostex-gpui-grok-web-reload",
            "Reload",
            BROWSER_ICON_RELOAD,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                this.reload_titlebar_view(mode, window, cx);
            }),
        );
        h_flex()
            .id(SharedString::from("ghostex-gpui-grok-web-login-bar"))
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT))
            .w_full()
            .items_center()
            .pl(px(BROWSER_TOOLBAR_LEADING_PADDING))
            .pr(px(WORKAREA_HEADER_EDGE_PADDING + WORKAREA_HEADER_PINNED_GAP))
            .gap(px(6.0))
            .bg(browser_toolbar_background())
            .border_b_1()
            .border_color(chrome_color(0x252525, 0xd4d4d4))
            .child(picker)
            .child(div().flex_1())
            .child(home)
            .child(reload)
            .into_any_element()
    }
}

//! The Claude view's accounts: which saved Claude account's claude.ai sign-in the page shows, the
//! account bar above the page, and switching.
//!
//! CDXC:Extensions 2026-10-25 DECISION:
//! User: "we really need to have the ability to save different claude profiles for each account and switch between those". The profiles are the saved Claude accounts of Settings > Accounts, each on the claude.ai Browser profile that page already signs in for the account's cloud sessions, so one sign-in serves both (the user's answer, 2026-10-25). The switcher is a slim bar above the page with the account picker (name, email, signed-in dot) and Reload (the user's answer). The picked account is remembered across restarts; switching reopens the page on the other account's profile.
//! SEE-ALSO: CDXC:ClaudeCloud in app/claude_cloud_page.rs (the profiles and the sign-in state).

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::{Action, SharedString};
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use serde_json::{Value, json};

use super::view::claude_web_view_id;
use crate::app::claude_cloud_page::{
    claude_account_browser_profile, claude_account_signin_email, note_claude_account_signin,
};
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::app::titlebar::account_usage::account_display_text;
use crate::*;

const SELECTION_FILE: &str = "claude-web-account.json";

#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SelectClaudeWebAccount {
    pub(crate) account_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SignOutClaudeWebAccount;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ManageClaudeWebAccounts;

/// A saved Claude account the Claude view can show.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClaudeProfileAccount {
    pub(crate) id: String,
    pub(crate) name: String,
    /// Lowercased; empty when the account's login did not report one.
    pub(crate) email: String,
}

struct ClaudeAccountList {
    accounts: Vec<ClaudeProfileAccount>,
    default_id: Option<String>,
}

thread_local! {
    /// The saved Claude accounts, read when the view first opens and again whenever the bar's menu
    /// opens.
    static ACCOUNTS: RefCell<Option<ClaudeAccountList>> = const { RefCell::new(None) };
    static ACCOUNTS_LOADING: Cell<bool> = const { Cell::new(false) };
    /// The picked account id (`claude-web-account.json`); `Some(None)` once read and empty.
    static SELECTION: RefCell<Option<Option<String>>> = const { RefCell::new(None) };
    /// The profile the open page was opened on, to notice when the pick changes under it.
    static OPENED_ON: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The notice last put at the top of the page, with the page it was put on: a page opened
    /// again (another account and back) starts without it.
    static PAGE_NOTICE: RefCell<Option<(gpui::EntityId, Option<Value>)>> = const { RefCell::new(None) };
}

fn selection_file() -> std::path::PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join(SELECTION_FILE)
}

fn picked_account() -> Option<String> {
    SELECTION.with(|selection| {
        selection
            .borrow_mut()
            .get_or_insert_with(|| {
                std::fs::read_to_string(selection_file())
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .and_then(|value| value["accountId"].as_str().map(str::to_string))
            })
            .clone()
    })
}

fn save_picked_account(account_id: &str) {
    SELECTION.with(|selection| *selection.borrow_mut() = Some(Some(account_id.to_string())));
    let path = selection_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, json!({ "accountId": account_id }).to_string());
}

fn parse_claude_accounts(state: &Value) -> ClaudeAccountList {
    let accounts = state["accounts"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|account| {
            account["provider"].as_str() == Some("claude")
                && account["registered"].as_bool() == Some(true)
                && account["userDisabled"].as_bool() != Some(true)
        })
        .filter_map(|account| {
            let email = account["email"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_lowercase();
            let name = account["name"]
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| email.clone());
            Some(ClaudeProfileAccount {
                id: account["id"].as_str()?.to_string(),
                name,
                email,
            })
        })
        .collect();
    ClaudeAccountList {
        accounts,
        default_id: state["defaultAccounts"]["claude"]
            .as_str()
            .map(str::to_string),
    }
}

/// Which sign-in the Claude page opens on.
#[derive(Debug, PartialEq)]
enum AccountChoice {
    /// The account list is still loading and nothing was picked before.
    Waiting,
    /// No saved Claude account: the page uses the view's own profile, one shared sign-in.
    NoAccounts,
    Account(ClaudeProfileAccount),
}

/// The account the page shows: the remembered pick while that account is still saved, else the
/// default Claude account, else the first one. A remembered pick opens at once, before the account
/// list is in.
fn account_choice() -> AccountChoice {
    let picked = picked_account();
    ACCOUNTS.with(|accounts| choose_account(picked, accounts.borrow().as_ref()))
}

fn choose_account(picked: Option<String>, list: Option<&ClaudeAccountList>) -> AccountChoice {
    let Some(list) = list else {
        return match picked {
            Some(id) => AccountChoice::Account(ClaudeProfileAccount {
                id,
                name: String::new(),
                email: String::new(),
            }),
            None => AccountChoice::Waiting,
        };
    };
    let find = |wanted: &str| list.accounts.iter().find(|account| account.id == wanted);
    picked
        .as_deref()
        .and_then(find)
        .or_else(|| list.default_id.as_deref().and_then(find))
        .or_else(|| list.accounts.first())
        .cloned()
        .map_or(AccountChoice::NoAccounts, AccountChoice::Account)
}

/// The notice at the top of the page for the account it shows: sign in (a first visit, or a
/// claude.ai sign-in that ran out), or signed in as someone else, with the one button that signs
/// the page out so the right account can sign in (CDXC:ClaudeCloud 2026-10-23 in
/// app/claude_cloud_page.rs). `seen` is the email the page is signed in as.
fn claude_web_page_notice(
    account: &ClaudeProfileAccount,
    seen: Option<&str>,
    signed_out: bool,
) -> Option<Value> {
    // A pick made before the account list loaded has no name to sign in as yet.
    if account.name.is_empty() {
        return None;
    }
    let name = &account.name;
    let email = (!account.email.is_empty()).then_some(account.email.as_str());
    if signed_out {
        let wanted = email.unwrap_or(name);
        return Some(json!({
            "text": format!("Sign in to claude.ai as {wanted}. Ghostex keeps this sign-in for {name}, the same one its cloud sessions use."),
        }));
    }
    match (seen, email) {
        (Some(seen), Some(want)) if seen != want => Some(json!({
            "text": format!("This page is signed in as {seen}, but it is {name}'s sign-in ({want})."),
            "button": format!("Sign in as {name}"),
            "action": "signOut",
        })),
        _ => None,
    }
}

fn known_account(id: &str) -> Option<ClaudeProfileAccount> {
    ACCOUNTS.with(|accounts| {
        accounts
            .borrow()
            .as_ref()?
            .accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
    })
}

impl GhostexGpuiApp {
    /// The CEF profile the Claude page opens on: `None` while the account list loads with nothing
    /// picked yet, else the picked account's profile (`shared`, the view's own profile, when no
    /// Claude account is saved).
    pub(crate) fn claude_web_profile(
        &mut self,
        shared: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<String> {
        if ACCOUNTS.with(|accounts| accounts.borrow().is_none()) {
            self.refresh_claude_web_accounts(cx);
        }
        // The page reports which email it is signed in as (app/claude_cloud_page.rs).
        self.ensure_claude_cloud_page_sweep(cx);
        let profile = match account_choice() {
            AccountChoice::Waiting => return None,
            AccountChoice::NoAccounts => shared.to_string(),
            AccountChoice::Account(account) => claude_account_browser_profile(&account.id),
        };
        OPENED_ON.with(|opened| *opened.borrow_mut() = Some(profile.clone()));
        Some(profile)
    }

    /// The open Claude page and the account it shows, for the claude.ai page reader
    /// (app/claude_cloud_page.rs). Only a page opened on that account's profile speaks for it.
    pub(crate) fn claude_web_page(&self) -> Option<(ClaudeProfileAccount, Entity<CefSurface>)> {
        let AccountChoice::Account(account) = account_choice() else {
            return None;
        };
        let surface = self
            .project_workarea_runtime_cef_surfaces
            .get(&ProjectWorkareaCefSurfaceSlotKey::Extension(
                claude_web_view_id(),
            ))?
            .surface
            .clone();
        let account = known_account(&account.id).unwrap_or(account);
        let opened = OPENED_ON.with(|opened| opened.borrow().clone());
        (opened == Some(claude_account_browser_profile(&account.id))).then_some((account, surface))
    }

    fn refresh_claude_web_accounts(&mut self, cx: &mut gpui::Context<Self>) {
        if ACCOUNTS_LOADING.replace(true) {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/agentAccounts",
                        &json!({ "operation": "list" }),
                        Duration::from_secs(30),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                ACCOUNTS_LOADING.set(false);
                // A failed read leaves the list unread, so the page's next pass asks again.
                let Ok(state) = &result else {
                    return;
                };
                ACCOUNTS.with(|accounts| {
                    *accounts.borrow_mut() = Some(parse_claude_accounts(state));
                });
                this.sync_claude_web_page(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Reopens the page when its account changed under it (picked in the bar, or its account was
    /// removed in Settings), and lets a page that waited for the account list open.
    fn sync_claude_web_page(&mut self, cx: &mut gpui::Context<Self>) {
        let slot = ProjectWorkareaCefSurfaceSlotKey::Extension(claude_web_view_id());
        let wanted = match account_choice() {
            AccountChoice::Waiting => return,
            AccountChoice::NoAccounts => slot.cef_profile_id(),
            AccountChoice::Account(account) => claude_account_browser_profile(&account.id),
        };
        let opened = OPENED_ON.with(|opened| opened.borrow().clone());
        if opened.is_some_and(|opened| opened != wanted) {
            self.remove_project_workarea_runtime_cef_surface(slot, cx);
            self.parked_project_workarea_surfaces
                .retain(|parked| parked.slot_key != slot);
            OPENED_ON.with(|opened| *opened.borrow_mut() = None);
        }
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
    }

    /// A row of the bar's account menu.
    pub(crate) fn select_claude_web_account(
        &mut self,
        account_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        save_picked_account(account_id);
        self.sync_claude_web_page(cx);
        cx.notify();
    }

    /// The menu's Sign out: forgets the shown account's claude.ai cookies and returns the page to
    /// claude.ai's login.
    pub(crate) fn sign_out_claude_web_account(&mut self, cx: &mut gpui::Context<Self>) {
        let Some((account, surface)) = self.claude_web_page() else {
            return;
        };
        let profile = claude_account_browser_profile(&account.id);
        if self.sign_claude_page_out(&profile, Some(&account.id), &surface, cx) {
            PAGE_NOTICE.with(|shown| *shown.borrow_mut() = None);
        }
        cx.notify();
    }

    /// Asks the Claude page what it shows (from the claude.ai page sweep in
    /// app/claude_cloud_page.rs): its sign-in is kept for the account like a cloud session page's,
    /// the bar's dot follows it, and the page gets the account's notice.
    pub(crate) fn sweep_claude_web_page(&mut self, cx: &mut gpui::Context<Self>) {
        let Some((account, surface)) = self.claude_web_page() else {
            return;
        };
        let answer = surface.read(cx).take_claude_cloud_page_answer();
        surface.read(cx).ask_claude_cloud_page();
        let Some(answer) = answer.and_then(|text| serde_json::from_str::<Value>(&text).ok()) else {
            return;
        };
        let seen = answer["email"].as_str().map(str::to_lowercase);
        let signed_out = answer["signedOut"].as_bool() == Some(true);
        let before = claude_account_signin_email(&account.id);
        let email = (!account.email.is_empty()).then_some(account.email.as_str());
        note_claude_account_signin(&account.id, email, seen.as_deref(), signed_out);
        if claude_account_signin_email(&account.id) != before {
            cx.notify();
        }
        let notice = claude_web_page_notice(&account, seen.as_deref(), signed_out);
        let page = surface.entity_id();
        let changed = PAGE_NOTICE.with(|shown| {
            let mut shown = shown.borrow_mut();
            let changed = shown.as_ref() != Some(&(page, notice.clone()));
            *shown = Some((page, notice.clone()));
            changed
        });
        if changed {
            let script = crate::cef::claude_cloud_page_notice_script(notice.as_ref());
            surface.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        }
        if answer["request"].as_str() == Some("signOut") {
            let profile = claude_account_browser_profile(&account.id);
            if self.sign_claude_page_out(&profile, Some(&account.id), &surface, cx) {
                PAGE_NOTICE.with(|shown| *shown.borrow_mut() = None);
                cx.notify();
            }
        }
    }

    fn show_claude_web_account_menu(
        &mut self,
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // The list may have changed in Settings since it was read; the next menu shows that.
        self.refresh_claude_web_accounts(cx);
        let current = match account_choice() {
            AccountChoice::Account(account) => Some(account.id),
            _ => None,
        };
        let accounts = ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .as_ref()
                .map(|list| list.accounts.clone())
                .unwrap_or_default()
        });
        let mut menu = GpuiContextMenu::new();
        for account in &accounts {
            menu = menu.menu_with_check(
                account_menu_label(account),
                current.as_deref() == Some(account.id.as_str()),
                Box::new(SelectClaudeWebAccount {
                    account_id: account.id.clone(),
                }),
            );
        }
        let signed_in = current
            .as_deref()
            .and_then(claude_account_signin_email)
            .flatten()
            .is_some();
        menu = menu.separator();
        if signed_in {
            menu = menu.menu("Sign out of claude.ai", Box::new(SignOutClaudeWebAccount));
        }
        menu.menu("Manage accounts…", Box::new(ManageClaudeWebAccounts))
            .toggle_below(trigger_bounds, window, cx);
    }

    /// The bar above the Claude page: the account picker on the left (signed-in dot, name, email)
    /// and Reload on the right.
    pub(crate) fn render_claude_web_account_bar(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let id = claude_web_view_id();
        let muted: Hsla = chrome_color(0xb7b7b7, 0x525252).into();
        let choice = account_choice();
        let waiting = choice == AccountChoice::Waiting;
        let (dot, name, email, label): (Option<bool>, String, String, &str) = match choice {
            AccountChoice::Waiting => (
                None,
                "Loading accounts…".into(),
                String::new(),
                "Loading accounts",
            ),
            AccountChoice::NoAccounts => (
                None,
                "No saved Claude account".into(),
                "Add one in Settings > Accounts to keep a sign-in per account".into(),
                "Manage accounts",
            ),
            AccountChoice::Account(account) => {
                // A pick made before the list loaded has no name yet.
                let account = known_account(&account.id).unwrap_or(account);
                let signed_in = claude_account_signin_email(&account.id).flatten().is_some();
                let email = if account.email.is_empty() || account.email == account.name {
                    String::new()
                } else {
                    account_display_text(&account.email)
                };
                let name = if account.name.is_empty() {
                    "Loading account…".to_string()
                } else {
                    account_display_text(&account.name)
                };
                (Some(signed_in), name, email, "Switch account")
            }
        };
        let has_accounts = dot.is_some();
        let trigger_bounds = std::rc::Rc::new(std::cell::Cell::new(None));
        let picker = h_flex()
            .id("ghostex-gpui-claude-web-account-picker")
            .role(gpui::Role::Button)
            .aria_label(label)
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
            .when_some(dot, |this, signed_in| {
                this.child(
                    div()
                        .size(px(7.0))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(rgb(if signed_in { 0x4ade80 } else { 0x64748b })),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.0))
                    .text_color(titlebar_active_text_color())
                    .child(name),
            )
            .when(!email.is_empty(), |this| {
                this.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.0))
                        .text_color(muted)
                        .child(email),
                )
            })
            .when(has_accounts, |this| {
                this.child(titlebar_svg_icon(TITLEBAR_ICON_CHEVRON_DOWN, 12.0, muted))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if waiting {
                        // The list could not be read yet (gxserver starting): ask again.
                        this.refresh_claude_web_accounts(cx);
                        return;
                    }
                    if !has_accounts {
                        this.open_gpui_settings_tab_from_new_thread_picker("accounts", cx);
                        return;
                    }
                    if let Some(bounds) = trigger_bounds.get() {
                        this.show_claude_web_account_menu(bounds, window, cx);
                    }
                }),
            );
        let reload = div()
            .id("ghostex-gpui-claude-web-reload")
            .role(gpui::Role::Button)
            .aria_label("Reload")
            .flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(30.0))
            .items_center()
            .justify_center()
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .cursor_default()
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .child(titlebar_svg_icon(
                BROWSER_ICON_RELOAD,
                14.0,
                titlebar_icon_color(),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    // The reloaded page has lost its notice; the next answer puts it back.
                    PAGE_NOTICE.with(|shown| *shown.borrow_mut() = None);
                    this.reload_titlebar_view(TitlebarMode::Extension(id), window, cx);
                }),
            )
            .managed_discrete_tooltip_with_placement(
                ManagedTooltipPlacement::Auto,
                Duration::from_millis(300),
                |window, cx| titlebar_tooltip("Reload", window, cx),
            );
        h_flex()
            .id(SharedString::from("ghostex-gpui-claude-web-account-bar"))
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
            .child(reload)
            .into_any_element()
    }
}

/// "Name · email" in the menu, masked when Settings hides account emails.
fn account_menu_label(account: &ClaudeProfileAccount) -> String {
    if account.email.is_empty() || account.email == account.name {
        account_display_text(&account.name)
    } else {
        account_display_text(&format!("{} · {}", account.name, account.email))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        AccountChoice, ClaudeProfileAccount, choose_account, claude_web_page_notice,
        parse_claude_accounts,
    };

    fn account(id: &str, name: &str, email: &str) -> ClaudeProfileAccount {
        ClaudeProfileAccount {
            id: id.into(),
            name: name.into(),
            email: email.into(),
        }
    }

    fn accounts_state() -> serde_json::Value {
        json!({
            "accounts": [
                {"id": "codex-1", "provider": "codex", "registered": true, "name": "Codex", "email": "c@x.io"},
                {"id": "claude-1", "provider": "claude", "registered": true, "name": "Work", "email": " Work@Example.com "},
                {"id": "claude-2", "provider": "claude", "registered": true, "name": "", "email": "me@example.com"},
                {"id": "claude-3", "provider": "claude", "registered": true, "userDisabled": true, "name": "Old", "email": "old@example.com"},
                {"id": "claude-4", "provider": "claude", "registered": false, "name": "Found", "email": "found@example.com"},
            ],
            "defaultAccounts": {"claude": "claude-2"},
        })
    }

    #[test]
    fn only_saved_enabled_claude_accounts_are_offered() {
        let list = parse_claude_accounts(&accounts_state());
        assert_eq!(
            list.accounts,
            vec![
                account("claude-1", "Work", "work@example.com"),
                account("claude-2", "me@example.com", "me@example.com"),
            ]
        );
        assert_eq!(list.default_id.as_deref(), Some("claude-2"));
    }

    #[test]
    fn the_pick_wins_while_saved_then_the_default_then_the_first() {
        let list = parse_claude_accounts(&accounts_state());
        let chosen = |picked: Option<&str>| choose_account(picked.map(str::to_string), Some(&list));
        assert_eq!(
            chosen(Some("claude-1")),
            AccountChoice::Account(account("claude-1", "Work", "work@example.com"))
        );
        // A removed or disabled account falls back to the default.
        assert_eq!(
            chosen(Some("claude-3")),
            AccountChoice::Account(account("claude-2", "me@example.com", "me@example.com"))
        );
        assert_eq!(chosen(None), chosen(Some("gone")));
        let no_default = parse_claude_accounts(&json!({"accounts": accounts_state()["accounts"]}));
        assert_eq!(
            choose_account(None, Some(&no_default)),
            AccountChoice::Account(account("claude-1", "Work", "work@example.com"))
        );
    }

    #[test]
    fn no_saved_account_uses_the_shared_page_and_an_unread_list_waits_unless_picked() {
        let empty = parse_claude_accounts(&json!({"accounts": []}));
        assert_eq!(
            choose_account(Some("claude-1".into()), Some(&empty)),
            AccountChoice::NoAccounts
        );
        assert_eq!(choose_account(None, None), AccountChoice::Waiting);
        assert_eq!(
            choose_account(Some("claude-1".into()), None),
            AccountChoice::Account(account("claude-1", "", ""))
        );
    }

    #[test]
    fn the_page_asks_for_the_accounts_own_sign_in() {
        let work = account("claude-1", "Work", "work@example.com");
        let signed_out = claude_web_page_notice(&work, None, true).expect("sign-in notice");
        assert!(
            signed_out["text"]
                .as_str()
                .unwrap()
                .starts_with("Sign in to claude.ai as work@example.com.")
        );
        assert!(signed_out.get("action").is_none());
        let wrong = claude_web_page_notice(&work, Some("other@example.com"), false)
            .expect("wrong account notice");
        assert_eq!(wrong["action"], "signOut");
        assert_eq!(wrong["button"], "Sign in as Work");
        assert_eq!(
            claude_web_page_notice(&work, Some("work@example.com"), false),
            None
        );
        // Unknown email on either side is never a mismatch.
        assert_eq!(claude_web_page_notice(&work, None, false), None);
        let no_email = account("claude-2", "Home", "");
        assert_eq!(
            claude_web_page_notice(&no_email, Some("x@example.com"), false),
            None
        );
        // A pick from before the list loaded has no name yet.
        assert_eq!(
            claude_web_page_notice(&account("claude-1", "", ""), None, true),
            None
        );
    }
}

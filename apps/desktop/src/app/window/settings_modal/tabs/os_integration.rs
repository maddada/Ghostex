//! The OS Integration page (packages/core-ui/settings-modal/tabs/os-integration.tsx (deleted 2026-10-01)), shown only
//! with Enable Experimental Features: the default-app buttons, the CLI examples, and the file and
//! link handler diagnostics (`requestOSIntegrationStatus` / `setOSIntegrationDefaults`, answered
//! with an `osIntegrationStatus` payload).
use super::super::super::native_modal_kit::*;
use super::super::fields::{
    ButtonVariant, FieldStates, SettingsPage, card_inset, settings_button, settings_icon,
    settings_list_item, settings_section,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, SettingsStoreEvent, post_store_message};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, FontWeight, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

const ICON_CODE_DOTS: &str = "modals/settings/code-dots.svg";
const ICON_LINK: &str = "modals/agents-hub/link.svg";
const ICON_PLAYER_PLAY: &str = "modals/settings/player-play.svg";
const ICON_CIRCLE_CHECK_FILLED: &str = "modals/settings/circle-check-filled.svg";
const ICON_REFRESH: &str = "modals/settings/refresh.svg";
const ICON_ALERT_TRIANGLE: &str = "modals/settings/alert-triangle.svg";

pub(crate) fn os_integration_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| OsIntegrationTab::new(store.clone(), cx)).into()
}

pub(crate) struct OsIntegrationTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// `osIntegrationStatusLoading`: set when a request is posted, cleared by the answer.
    loading: bool,
}

impl OsIntegrationTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        cx.subscribe(
            &store,
            |page: &mut Self, _, event: &SettingsStoreEvent, cx| {
                let SettingsStoreEvent::HostPayload(kind) = event;
                if kind == "osIntegrationStatus" {
                    page.loading = false;
                    cx.notify();
                }
            },
        )
        .detach();
        // The page asks for the handler status the first time it opens without one (after the
        // window that is being built has been handed to its host).
        let missing = store.read(cx).host_payload("osIntegrationStatus").is_none();
        if missing {
            cx.spawn(async move |page, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.request(json!({ "type": "requestOSIntegrationStatus" }), cx);
                });
            })
            .detach();
        }
        Self {
            store,
            fields: FieldStates::default(),
            loading: missing,
        }
    }

    fn request(&mut self, message: Value, cx: &mut Context<Self>) {
        self.loading = true;
        post_store_message(&self.store, message, cx);
        cx.notify();
    }
}

impl SettingsPage for OsIntegrationTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// `getOSIntegrationStatusNoticeTitle`.
fn notice_title(items: &[Value]) -> &'static str {
    if items
        .iter()
        .any(|item| reason(item) == "unsupportedPlatform")
    {
        "System app registration is unavailable in this build."
    } else {
        "Some file and link handler updates need attention."
    }
}

/// `getOSIntegrationStatusNoticeDescription`.
fn notice_description(items: &[Value]) -> &'static str {
    if items
        .iter()
        .any(|item| reason(item) == "unsupportedPlatform")
    {
        "This platform cannot inspect or change system app defaults."
    } else {
        "Refresh after your operating system finishes updating app registration, or choose Ghostex manually in Open With or system settings."
    }
}

fn text<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key).and_then(Value::as_str)
}

fn reason(item: &Value) -> &str {
    text(item, "reason").unwrap_or_default()
}

/// `formatOSIntegrationStatusExtension`: `^[A-Za-z0-9][A-Za-z0-9_-]{0,24}$`.
fn status_extension(extension: Option<&str>) -> Option<&str> {
    let extension = extension?;
    let mut characters = extension.chars();
    let first = characters.next()?;
    let valid = first.is_ascii_alphanumeric()
        && extension.chars().count() <= 25
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        });
    valid.then_some(extension)
}

/// `formatOSIntegrationStatusItemSubject`.
fn item_subject(item: &Value) -> String {
    let extension = status_extension(text(item, "extension"));
    match text(item, "target").unwrap_or_default() {
        "editor" => extension
            .map(|extension| format!("Editor default .{extension}"))
            .unwrap_or_else(|| "Editor defaults".to_string()),
        "scriptRunner" => extension
            .map(|extension| format!("Script runner .{extension}"))
            .unwrap_or_else(|| "Script runner".to_string()),
        "ghostexLinks" => {
            if text(item, "scheme") == Some("ghostex") {
                "Ghostex links ghostex://".to_string()
            } else {
                "Ghostex links".to_string()
            }
        }
        "bundleRegistration" => {
            if text(item, "operation") == Some("registerBundle") {
                "App registration".to_string()
            } else {
                "App identity".to_string()
            }
        }
        _ => "Platform support".to_string(),
    }
}

/// `formatOSIntegrationStatusItemReason`.
fn item_reason(item: &Value) -> &'static str {
    match reason(item) {
        "bundleIdentifierMissing" => "App identity missing",
        "bundleRegistrationFailed" => "Registration failed",
        "contentTypeUnavailable" => "File type unavailable",
        "invalidTarget" => "Unsupported action",
        "launchServicesRejected" => "Default change rejected",
        "unsupportedPlatform" => "Unavailable",
        _ => "",
    }
}

/// How many sampled defaults point at Ghostex (`Object.values(defaults).filter(id => id === bundleId)`).
fn defaults_count(status: &Value, key: &str, bundle_id: Option<&str>) -> (usize, usize) {
    let defaults = status.get(key).and_then(Value::as_object);
    let total = defaults.map(|defaults| defaults.len()).unwrap_or(0);
    let Some(bundle_id) = bundle_id else {
        return (0, total);
    };
    let matching = defaults
        .map(|defaults| {
            defaults
                .values()
                .filter(|value| value.as_str() == Some(bundle_id))
                .count()
        })
        .unwrap_or(0);
    (matching, total)
}

impl OsIntegrationTab {
    /// CDXC:OsIntegration 2026-05-27 WHY:
    /// Ghostex registers as an available editor and script handler at install/build time, but Settings is the only place that changes default editor, ghostex:// link, or script-runner ownership.
    fn default_row(
        &mut self,
        title: &'static str,
        label: &'static str,
        icon: &'static str,
        variant: ButtonVariant,
        target: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.store.read(cx).palette();
        let button = settings_button(
            &p,
            format!("os-integration-{target}"),
            label,
            Some(icon),
            variant,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.request(
                    json!({ "target": target, "type": "setOSIntegrationDefaults" }),
                    cx,
                );
            },
            cx,
        );
        settings_list_item(&p, None, None, title, None, Some(button))
    }

    fn diagnostic_row(&self, title: &'static str, value: String, cx: &App) -> AnyElement {
        let p = self.store.read(cx).palette();
        settings_list_item(
            &p,
            None,
            None,
            title,
            None,
            Some(
                div()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.muted))
                    .child(value)
                    .into_any_element(),
            ),
        )
    }
}

impl Render for OsIntegrationTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching, status) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::OsIntegration),
                matching,
                store.host_payload("osIntegrationStatus").cloned(),
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::OsIntegration,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        if search.section("defaults").has_visible() {
            let rows = vec![
                self.default_row(
                    "Default editor",
                    "Set as Default Editor",
                    ICON_CODE_DOTS,
                    ButtonVariant::Outline,
                    "editor",
                    cx,
                ),
                self.default_row(
                    "Ghostex links",
                    "Set Ghostex Links",
                    ICON_LINK,
                    ButtonVariant::Outline,
                    "ghostexLinks",
                    cx,
                ),
                self.default_row(
                    "Script runner",
                    "Set Script Runner",
                    ICON_PLAYER_PLAY,
                    ButtonVariant::Outline,
                    "scriptRunner",
                    cx,
                ),
                self.default_row(
                    "All defaults",
                    "Set All",
                    ICON_CIRCLE_CHECK_FILLED,
                    ButtonVariant::Default,
                    "all",
                    cx,
                ),
            ];
            blocks.extend(
                settings_section(&p, "Defaults", None, None, rows)
                    .map(|section| PageBlock::section("defaults", section)),
            );
        }
        if search.section("cli").has_visible() {
            let commands = [
                "ghostex open ./folder",
                "ghostex edit --wait file.ts:12:3",
                "ghostex terminal --cwd /tmp --title Scratch -- echo hi",
                "ghostex ./file.txt",
            ];
            let list = v_flex()
                .gap(px(8.0))
                .font_family(MODAL_MONO_FONT)
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(p.muted))
                .children(commands.map(|command| div().child(command)));
            blocks.extend(
                settings_section(&p, "CLI", None, None, vec![card_inset(list)])
                    .map(|section| PageBlock::section("cli", section)),
            );
        }
        if search.section("diagnostics").has_visible() {
            let loading = self.loading;
            let refresh = settings_button(
                &p,
                "os-integration-refresh",
                "Refresh",
                Some(ICON_REFRESH),
                ButtonVariant::Outline,
                loading,
                Some("File and link handler status is being checked.".into()),
                |page: &mut Self, _window, cx| {
                    page.request(json!({ "type": "requestOSIntegrationStatus" }), cx);
                },
                cx,
            );
            let title = if loading && status.is_none() {
                "Checking file and link handlers..."
            } else {
                "File and link handler status"
            };
            let mut rows = vec![settings_list_item(
                &p,
                None,
                None,
                title,
                None,
                Some(refresh),
            )];
            match &status {
                Some(status) => {
                    let items: Vec<Value> = status
                        .get("statusItems")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    if !items.is_empty() {
                        rows.push(status_notice(&p, &self.store, &items));
                    }
                    let bundle_id = status.get("bundleIdentifier").and_then(Value::as_str);
                    let flag = |key: &str| status.get(key).and_then(Value::as_bool) == Some(true);
                    let registered =
                        |key: &str| if flag(key) { "Registered" } else { "Missing" }.to_string();
                    let links_default_id = status
                        .get("ghostexLinkDefaultBundleId")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty());
                    let links_default = links_default_id.is_some() && links_default_id == bundle_id;
                    let links = if !flag("registeredGhostexURLScheme") {
                        "Missing".to_string()
                    } else if links_default {
                        "Default".to_string()
                    } else {
                        format!("Default: {}", links_default_id.unwrap_or("None"))
                    };
                    let (editor_matching, editor_total) =
                        defaults_count(status, "editorDefaults", bundle_id);
                    let (script_matching, script_total) =
                        defaults_count(status, "scriptDefaults", bundle_id);
                    rows.push(self.diagnostic_row(
                        "Available editor",
                        registered("registeredEditableFiles"),
                        cx,
                    ));
                    rows.push(self.diagnostic_row(
                        "Available script runner",
                        registered("registeredScriptRunner"),
                        cx,
                    ));
                    rows.push(self.diagnostic_row("ghostex:// links", links, cx));
                    rows.push(self.diagnostic_row(
                        "Editor defaults",
                        format!("{editor_matching}/{editor_total} sampled"),
                        cx,
                    ));
                    rows.push(self.diagnostic_row(
                        "Script defaults",
                        format!("{script_matching}/{script_total} sampled"),
                        cx,
                    ));
                }
                None => rows.push(card_inset(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(p.muted))
                        .child("Ghostex has not checked Launch Services yet."),
                )),
            }
            blocks.extend(
                settings_section(&p, "Diagnostics", None, None, rows)
                    .map(|section| PageBlock::section("diagnostics", section)),
            );
        }
        settings_page(&self.store, SettingsTabId::OsIntegration, &p, blocks, cx)
    }
}

/// The notice over the diagnostics when handler updates need attention: a card child, so it takes
/// the card's 14/20 inset as its own padding (`.settings-list-card > :not(.settings-list-row)`).
///
/// CDXC:OsIntegration 2026-11-08 WHY: the notice was a red-tinted block listing "Editor default
/// .ts  Default change rejected" rows (the remote tile audit, finding 11). It is a neutral card
/// with the title and one plain sentence; the per-handler rows are under Details with Copy (User,
/// Q2021: raw text folds under Details).
fn status_notice(
    p: &super::super::palette::SettingsPalette,
    store: &Entity<SettingsStore>,
    items: &[Value],
) -> AnyElement {
    let rows = items
        .iter()
        .map(|item| format!("{}: {}", item_subject(item), item_reason(item)))
        .collect::<Vec<_>>()
        .join("\n");
    v_flex()
        .w_full()
        .gap(px(8.0))
        .px(px(20.0))
        .py(px(14.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .text_color(hsla(p.muted))
        .child(
            h_flex()
                .items_start()
                .gap(px(8.0))
                .child(div().mt(px(2.0)).flex_shrink_0().child(settings_icon(
                    ICON_ALERT_TRIANGLE,
                    16.0,
                    p.muted,
                )))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(4.0))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .child(notice_title(items)),
                        )
                        .child(div().child(notice_description(items)))
                        .child(super::super::fields::notice::raw_details(
                            p,
                            "os-integration-handler-status",
                            rows,
                            store,
                        )),
                ),
        )
        .into_any_element()
}

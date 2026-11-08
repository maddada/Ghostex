//! The Collab card in Settings > Extensions > Built-in: Collab is an extension component since
//! 11.0.1 (#495). The app ships none of Collab's working code; Install downloads the Collab
//! component on this computer's gxserver after the owner approves with Touch ID (or Windows
//! Hello, the Linux system prompt, the phone's biometrics), Update installs the newer component
//! an app update brought, and Uninstall ends every share and removes it (it never asks).
//!
//! The card reads `/api/extensionComponents` (kept in the store, `refresh_collab_component`, which
//! the rail also uses to show Settings > Collab only while Collab is installed), every 2 seconds
//! while an install waits or runs and every 15 otherwise.
//!
//! SEE-ALSO: server/src/collab_shim/install.rs (install, update, uninstall), server/collab (the
//! component).
//!
//! CDXC:Collab 2026-11-08 DECISION:
//! User (#495, Q1730): "i'm fine to not include the code of collab in the app unless you enable it in extensions (when enabled, we need to show touch id / auth so we make sure it's the user enabling this feature and installing it not an agent)".

use std::time::{Duration, Instant};

use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use serde_json::{Value, json};

use super::super::super::super::native_modal_kit::hsla;
use super::super::super::fields::{SizedButtonSize, SizedButtonVariant, settings_sized_button};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{refresh_collab_component, store_gxserver_rpc};
use super::ExtensionsTab;
use super::cards::{GridCardSpec, GridCell, card_grid, card_group, grid_card, tabler_tile};
use super::data::{ExtensionFilter, SourceFilter};

const CALL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(crate) struct CollabCardState {
    asked_at: Option<Instant>,
    /// Install was asked and waits for the owner's approval: since when.
    waiting_since: Option<Instant>,
    busy: bool,
    error: Option<String>,
}

fn collab_matches(filter: &ExtensionFilter) -> bool {
    if !matches!(filter.source, SourceFilter::All | SourceFilter::BuiltIn)
        || filter.category != "all"
        || filter.type_ != "all"
    {
        return false;
    }
    let query = filter.query.trim().to_lowercase();
    query.is_empty()
        || "collab share sharing link guest browser invite"
            .split(' ')
            .any(|word| word.contains(&query) || query.contains(word))
}

impl ExtensionsTab {
    /// The Collab group of Built-in (one card), or `None` when the filter hides it.
    pub(crate) fn render_collab_group(
        &mut self,
        p: &SettingsPalette,
        filter: &ExtensionFilter,
        columns: usize,
        first: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !collab_matches(filter) {
            return None;
        }
        let component = self.store.read(cx).collab_component();
        self.ask_collab_component(&component, cx);
        let installed = component["installed"] == true;
        let phase = component["phase"].as_str().unwrap_or("idle").to_string();
        let working = matches!(phase.as_str(), "downloading" | "installing" | "removing");
        let waiting = self
            .collab_card
            .waiting_since
            .is_some_and(|since| since.elapsed() < Duration::from_secs(11 * 60));
        if installed || working {
            self.collab_card.waiting_since = None;
        }
        let update = component["updateAvailable"].as_str().map(str::to_string);
        let can_install = component["canInstall"] == true;

        let status = if let Some(error) = &self.collab_card.error {
            error.clone()
        } else if waiting && !installed {
            "Approve on this computer or your phone to install Collab.".to_string()
        } else {
            match phase.as_str() {
                "downloading" => {
                    let done = component["doneBytes"].as_f64().unwrap_or(0.0);
                    let total = component["totalBytes"].as_f64().unwrap_or(0.0);
                    if total > 0.0 {
                        format!(
                            "Downloading Collab\u{2026} {}% of {:.0} MB",
                            ((done / total) * 100.0).floor(),
                            total / 1_000_000.0
                        )
                    } else {
                        "Downloading Collab\u{2026}".to_string()
                    }
                }
                "installing" => "Installing Collab\u{2026}".to_string(),
                "removing" => "Uninstalling Collab\u{2026}".to_string(),
                "failed" => format!(
                    "Couldn't install Collab. {}",
                    component["error"].as_str().unwrap_or_default()
                ),
                _ if installed => match component["status"].as_str() {
                    Some("running") => match &update {
                        Some(version) => format!("Running. Update {version} is ready."),
                        None => "Running. Share is in each chat's header.".to_string(),
                    },
                    Some("failed") => format!(
                        "Collab stopped. {}",
                        component["statusError"].as_str().unwrap_or_default()
                    ),
                    _ => "Starting\u{2026}".to_string(),
                },
                _ if component.is_null() => "Checking Collab\u{2026}".to_string(),
                _ if !can_install => "This build of Ghostex has no Collab component.".to_string(),
                _ if component["shareHidden"] == true => "Not installed. Share is hidden in chats; installing asks for Touch ID (or your system's equivalent).".to_string(),
                _ => "Not installed. Chats show Share, which installs it; installing asks for Touch ID (or your system's equivalent)."
                    .to_string(),
            }
        };

        let mut actions: Vec<AnyElement> = Vec::new();
        // "Don't show Share" was picked in a chat's Share popover: bring Share back.
        if !installed && component["shareHidden"] == true {
            actions.push(settings_sized_button(
                p,
                "collab-show-share",
                "Show Share in chats",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                self.collab_card.busy,
                None,
                |tab: &mut Self, _window, cx| tab.show_share_again(cx),
                cx,
            ));
        }
        let busy = self.collab_card.busy || working;
        let control = if installed {
            if update.is_some() {
                actions.push(settings_sized_button(
                    p,
                    "collab-component-update",
                    "Update",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    busy,
                    None,
                    |tab: &mut Self, _window, cx| tab.collab_component_op("update", cx),
                    cx,
                ));
            }
            settings_sized_button(
                p,
                "collab-component-uninstall",
                "Uninstall",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                busy,
                None,
                |tab: &mut Self, _window, cx| tab.collab_component_op("uninstall", cx),
                cx,
            )
        } else {
            settings_sized_button(
                p,
                "collab-component-install",
                if waiting {
                    "Waiting\u{2026}"
                } else {
                    "Install"
                },
                None,
                None,
                SizedButtonVariant::Default,
                SizedButtonSize::Sm,
                busy || waiting || !can_install,
                (!can_install)
                    .then(|| SharedString::from("This build of Ghostex has no Collab component.")),
                |tab: &mut Self, _window, cx| tab.collab_component_op("install", cx),
                cx,
            )
        };
        let extra = div()
            .mt(px(8.0))
            .w_full()
            .text_size(px(13.0))
            .line_height(px(18.57))
            .text_color(hsla(p.muted))
            .child(status)
            .into_any_element();
        let meta = match component["version"].as_str() {
            Some(version) if installed => format!("Extension \u{b7} {version}"),
            _ => "Extension \u{b7} Downloaded when installed".to_string(),
        };
        let card = grid_card(
            p,
            GridCardSpec {
                badge: None,
                id: SharedString::from("official-collab"),
                icon: tabler_tile(p, "modals/settings/users.svg", false),
                control: Some(control),
                leading: None,
                title: "Collab".into(),
                description: "Share a chat through a link that opens it in any browser. Guests type the link's code; their messages wait for your approval.".into(),
                extra: Some(extra),
                scope_summary: None,
                meta: meta.into(),
                actions,
                editing: false,
                enabled: installed,
                dragging: false,
            },
        );
        Some(card_group(
            p,
            "Sharing".to_string(),
            Some(1),
            first,
            card_grid(vec![GridCell { card, wide: None }], None, columns),
        ))
    }

    /// Reads the component again: every 2 seconds while something is under way, else every 15.
    fn ask_collab_component(&mut self, component: &Value, cx: &mut Context<Self>) {
        let under_way = self.collab_card.waiting_since.is_some()
            || matches!(
                component["phase"].as_str(),
                Some("downloading" | "installing" | "removing")
            )
            || component["status"] == "starting";
        let every = if under_way {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(15)
        };
        if self
            .collab_card
            .asked_at
            .is_some_and(|at| at.elapsed() < every)
        {
            return;
        }
        self.collab_card.asked_at = Some(Instant::now());
        refresh_collab_component(&self.store, cx);
        cx.spawn(async move |tab, cx| {
            cx.background_executor().timer(every).await;
            let _ = tab.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// Shows the chat header's Share button again (it offers Install while Collab isn't installed).
    fn show_share_again(&mut self, cx: &mut Context<Self>) {
        let tab = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/collab",
            json!({ "op": "hideShare", "hidden": false }),
            CALL_TIMEOUT,
            move |_, cx| {
                let _ = tab.update(cx, |tab, cx| {
                    tab.collab_card.asked_at = None;
                    refresh_collab_component(&tab.store, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Install, Update or Uninstall. Install answers with the owner's approval request, and
    /// gxserver installs Collab itself once they approve; the card follows it by reading again.
    fn collab_component_op(&mut self, op: &'static str, cx: &mut Context<Self>) {
        if self.collab_card.busy {
            return;
        }
        self.collab_card.busy = true;
        self.collab_card.error = None;
        let tab = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/extensionComponents",
            json!({ "op": op, "name": "collab" }),
            CALL_TIMEOUT,
            move |result, cx| {
                let _ = tab.update(cx, |tab, cx| {
                    tab.collab_card.busy = false;
                    match result {
                        Ok(_) => {}
                        // Installing waits for the owner's approval first.
                        Err(error)
                            if op == "install" && error.to_lowercase().contains("approv") =>
                        {
                            tab.collab_card.waiting_since = Some(Instant::now());
                        }
                        Err(error) => tab.collab_card.error = Some(error),
                    }
                    tab.collab_card.asked_at = None;
                    refresh_collab_component(&tab.store, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }
}

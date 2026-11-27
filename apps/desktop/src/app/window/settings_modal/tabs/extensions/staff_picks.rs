//! The Extensions page's Staff picks tab: the extensions we would turn on first, from every part
//! of Ghostex, grouped by what they are for, each card with its own switch.
//!
//! CDXC:Extensions 2026-11-27 DECISION:
//! User, on the Staff picks page (Q5223 to Q5226): Staff picks shows exactly "VS Code, Voice agent,
//! Boosts, Catch Me Up, Ghostex Apps, GitHub, Linear, Jira, Connectors"; "Grouped by what it's for";
//! "First tab, opens there until you pick another" (then the tab you had open, Q2044); Boosts is
//! in it with its own existing switch; Connectors: "it's working great, let's add it to staff picks
//! just put beta pill on it". Files ("this isn't a real extension my friend. it should be always on
//! for 99% of customers"), Search by Prompt, the Session details panel and Canvas are left out.
//! Each card is the same card the other tabs draw (its switch flips the same setting), with its
//! type as its pill, or Beta for a feature in beta. The Settings search works as on every tab: it
//! shows every tab's matches, so Staff picks adds nothing while one is typed.
//!
//! CDXC:CloudBoxes 2026-11-27 DECISION:
//! User: "cloud boxes should be a staff picks extension to show that item in the sidebar of
//! settings". Cloud Boxes joins Work on code with VS Code, as a Feature card whose switch is
//! `cloudBoxesEnabled`; Settings > Cloud Boxes is in the rail only while it is on.
use super::super::super::fields::small_switch_control;
use super::super::super::model::SettingsTabId;
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_gxserver_rpc;
use super::ExtensionsTab;
use super::cards::{
    EXTENSION_LIST_COLUMNS, GridCardSpec, GridCell, card_grid, card_group_with_note, grid_card,
    tabler_tile,
};
use super::data::{
    ExtensionFilter, FilterSubject, SourceFilter, official_category, official_extensions,
};
use super::sections::plain_section;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
};
use gpui_component::v_flex;
use serde_json::{Value, json};
use std::time::Duration;

/// gxserver's Boosts endpoint (server/src/boosts.rs), the one Settings > Boosts uses.
const BOOSTS_ENDPOINT: &str = "/api/boosts";
const BOOSTS_TIMEOUT: Duration = Duration::from_secs(15);

/// One card of Staff picks.
#[derive(Clone, Copy)]
enum Pick {
    /// A built-in extension (its `GHOSTEX_OFFICIAL_EXTENSIONS` id) and the type its pill names.
    BuiltIn(&'static str, &'static str),
    /// Boosts, whose switch is gxserver's (Settings > Boosts), not a built-in extension's.
    Boosts,
}

/// The groups, in order, each named for what its cards are for.
const GROUPS: [(&str, &[Pick]); 3] = [
    (
        "Work on code",
        &[
            Pick::BuiltIn("code", "View"),
            Pick::BuiltIn("cloudBoxes", "Feature"),
        ],
    ),
    (
        "Talk to your agents",
        &[
            Pick::BuiltIn("voiceAgent", "Feature"),
            Pick::Boosts,
            Pick::BuiltIn("catchMeUp", "Chat"),
            Pick::BuiltIn("ghostexApps", "Feature"),
        ],
    ),
    (
        "Your team's tools",
        &[
            Pick::BuiltIn("github", "Website view"),
            Pick::BuiltIn("linear", "Website view"),
            Pick::BuiltIn("jira", "Website view"),
            Pick::BuiltIn("connectors", "Feature"),
        ],
    ),
];

const BOOSTS_TITLE: &str = "Boosts";
const BOOSTS_DESCRIPTION: &str =
    "One click adds a ready prompt to your message: Live page, Map, Compare, Chart and more.";

/// Whether the Settings search (or the page filter) leaves the Boosts card in.
fn boosts_matches(filter: &ExtensionFilter) -> bool {
    filter.matches(&FilterSubject {
        categories: Vec::new(),
        search_text: vec![
            BOOSTS_DESCRIPTION.to_string(),
            "boost chat prompt".to_string(),
        ],
        source: SourceFilter::BuiltIn,
        title: BOOSTS_TITLE.to_string(),
        types: vec!["menu-item".to_string()],
    })
}

impl ExtensionsTab {
    /// Whether a Staff pick is offered on this computer and matches `filter`.
    fn pick_shown(&self, pick: Pick, filter: &ExtensionFilter, cx: &gpui::App) -> bool {
        match pick {
            Pick::BuiltIn(id, _) => official_extensions()
                .iter()
                .find(|extension| extension.id == id)
                .is_some_and(|extension| self.official_matches(extension, filter, cx)),
            // Boosts' switch is gxserver's: without it (as for the Installed cards) the card is
            // left out rather than shown with a switch that cannot be read.
            Pick::Boosts => {
                self.store.read(cx).request().gxserver_rpc_available && boosts_matches(filter)
            }
        }
    }

    /// The Staff picks tab's count in the strip: its cards under the current search.
    pub(crate) fn staff_picks_count(&self, cx: &gpui::App) -> usize {
        let filter = self.page_filter(cx);
        GROUPS
            .iter()
            .flat_map(|(_, picks)| picks.iter())
            .filter(|pick| self.pick_shown(**pick, &filter, cx))
            .count()
    }

    /// Reads whether Boosts are on (`/api/boosts` `list`), as Settings > Boosts does when it opens.
    pub(crate) fn load_boosts_on(&mut self, cx: &mut Context<Self>) {
        if !self.store.read(cx).request().gxserver_rpc_available {
            return;
        }
        self.boosts_call(json!({ "op": "list" }), cx);
    }

    /// One `/api/boosts` call; its answer says whether Boosts are on. A failed `setOn` puts the
    /// switch back where gxserver has it.
    fn boosts_call(&mut self, params: Value, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let setting = params["op"] == "setOn";
        store_gxserver_rpc(
            &self.store.clone(),
            BOOSTS_ENDPOINT,
            params,
            BOOSTS_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    match result {
                        Ok(answer) => page.boosts_on = Some(answer["on"] != false),
                        Err(_) if setting => page.load_boosts_on(cx),
                        Err(_) => {}
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// The Boosts card: its switch is Settings > Boosts' own (`setOn`); its name opens that page.
    fn boosts_cell(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> GridCell {
        let on = self.boosts_on.unwrap_or(false);
        let loading = self.boosts_on.is_none();
        let control = small_switch_control(
            p,
            SharedString::from("staff-picks-boosts-switch"),
            BOOSTS_TITLE,
            on,
            loading,
            loading.then(|| SharedString::from("Ghostex is reading whether Boosts are on.")),
            |page: &mut Self, next, _window, cx| {
                page.boosts_on = Some(next);
                page.boosts_call(json!({ "op": "setOn", "on": next }), cx);
                cx.notify();
            },
            cx,
        );
        let (control, toggle) = control.into_row_parts();
        let store = self.store.clone();
        let card = grid_card(
            p,
            GridCardSpec {
                id: SharedString::from("staff-picks-boosts"),
                icon: tabler_tile(p, "modals/settings/sparkles.svg", false),
                control: Some(control.into_any_element()),
                leading: None,
                title: BOOSTS_TITLE.into(),
                badge: Some("Chat".into()),
                description: BOOSTS_DESCRIPTION.into(),
                extra: None,
                scope_summary: None,
                meta: "Settings > Boosts edits each Boost's prompt and adds your own.".into(),
                status: None,
                actions: Vec::new(),
                editing: false,
                enabled: on,
                dragging: false,
                source_url: None,
                open: Some(Box::new(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.set_active_tab(SettingsTabId::Boosts, cx)
                    });
                })),
                toggle,
            },
        );
        GridCell::card(card)
    }

    pub(crate) fn render_staff_picks_section(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.boosts_requested {
            self.boosts_requested = true;
            self.load_boosts_on(cx);
        }
        let filter = self.page_filter(cx);
        let values = self.store.read(cx).values();
        let status = self
            .store
            .read(cx)
            .host_payload("pluginSettingsStatus")
            .cloned();
        let mut groups: Vec<AnyElement> = Vec::new();
        for (label, picks) in GROUPS {
            let mut cells: Vec<GridCell> = Vec::new();
            for pick in picks.iter().copied() {
                if !self.pick_shown(pick, &filter, cx) {
                    continue;
                }
                match pick {
                    Pick::Boosts => cells.push(self.boosts_cell(p, cx)),
                    Pick::BuiltIn(id, type_label) => {
                        let Some(extension) = official_extensions()
                            .iter()
                            .find(|extension| extension.id == id)
                        else {
                            continue;
                        };
                        // Its type as the pill, or its own (Beta) where it has one.
                        let mut shown = extension.clone();
                        shown.badge = Some(
                            extension
                                .badge
                                .clone()
                                .unwrap_or_else(|| type_label.to_string()),
                        );
                        let category_type = official_category(extension)
                            .map(|category| category.type_label.clone())
                            .unwrap_or_default();
                        cells.push(self.official_cell(
                            p,
                            &shown,
                            &category_type,
                            &values,
                            status.as_ref(),
                            window,
                            cx,
                        ));
                    }
                }
            }
            if cells.is_empty() {
                continue;
            }
            let first = groups.is_empty();
            groups.push(card_group_with_note(
                p,
                label,
                None,
                first,
                card_grid(cells, None, EXTENSION_LIST_COLUMNS),
            ));
        }
        plain_section(
            p,
            "",
            div()
                .child("The extensions we'd turn on first, from every part of Ghostex.")
                .into_any_element(),
            None,
            v_flex().w_full().children(groups).into_any_element(),
        )
    }
}

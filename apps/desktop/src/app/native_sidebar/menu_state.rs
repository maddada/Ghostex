use gpui::{Bounds, FocusHandle, Pixels, Point};
use serde_json::Value;

/// Where a panel sits relative to its anchor point.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum SidebarMenuPlacement {
    /// Centred across the sidebar at the anchor's height: submenus, and menus with nothing to hang
    /// from (a right-click on the list's empty space).
    #[default]
    Pointer,
    /// Hung from what opened it, a sidebar row or a button (`owner`): `gap` below it, and above it
    /// instead when the sidebar has no room below. Across, its left edge (or, with `align_right`,
    /// its right edge) is on `from`'s: the button that opened it, a zero-width span at a
    /// right-click's pointer, or the owner itself; the other edge of `from` when that side runs
    /// out of sidebar.
    Attached {
        owner: Bounds<Pixels>,
        from: Bounds<Pixels>,
        align_right: bool,
        gap: Pixels,
    },
}

/// The left edge of a `width`-wide menu lined up with the button `from` between `min_left` and
/// `max_right`: its left edge on the button's, or its right edge with `align_right`; the other edge
/// when that side runs out of room, and shifted inside when neither fits. The sidebar's attached
/// menus and the chat's pill strip menus (native_chat/pills_strip.rs) both place menus with it.
pub(crate) fn menu_left_at_button(
    from: Bounds<Pixels>,
    align_right: bool,
    min_left: Pixels,
    max_right: Pixels,
    width: Pixels,
) -> Pixels {
    let fits = |left: Pixels| left >= min_left && left + width <= max_right;
    let (preferred, flipped) = if align_right {
        (from.right() - width, from.left())
    } else {
        (from.left(), from.right() - width)
    };
    if fits(preferred) || !fits(flipped) {
        preferred
    } else {
        flipped
    }
    .min(max_right - width)
    .max(min_left)
}

/// The height of a searchable menu's field row above its rows, before the zoom.
pub(crate) const SIDEBAR_MENU_SEARCH_HEIGHT: f32 = 40.0;

/// A sidebar menu row's height, before the zoom.
pub(crate) const SIDEBAR_MENU_ROW_HEIGHT: f32 = 34.0;

/// How long a row's armed Remove button waits for its second click.
pub(crate) const SIDEBAR_MENU_ARM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

/// A menu narrowed by typing. Its first row asks for it with `search`.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User: "i want to be able to type here to search for a space or project please (in the filter dropdown)". The sidebar's menu draws in a frosted window that never takes keyboard focus, so it works the way Quick Access's pickers do (window/quick_access/chrome.rs, `menu_key` in its window.rs) with their field: the menu's focus takes the keys, and the field always reads as focused with a caret after the typed text. Up and Down walk the rows, Enter picks, Escape closes, Backspace takes off a letter.
#[derive(Clone)]
pub(crate) struct SidebarMenuSearch {
    pub(crate) placeholder: String,
    pub(crate) query: String,
    /// Every row; the panel's `items` hold the ones the query leaves.
    pub(crate) rows: Vec<Value>,
}

/// The rows `query` leaves: a row whose label, suffix and detail hold every typed word, with the headings
/// and separators of what is left. A row marked `searchHidden` (All sessions) leaves as soon as
/// anything is typed, so it can't take Enter from a match. When the first row's `search` has a
/// `submit`, a row that hands on the text itself comes first, so Enter takes it as typed; the
/// matches are a Down away.
pub(crate) fn searched_menu_rows(rows: &[Value], query: &str) -> Vec<Value> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut left: Vec<Value> = Vec::new();
    let mut heading: Option<&Value> = None;
    for row in rows {
        if row["separator"] == true {
            heading = None;
        } else if row["heading"] == true {
            heading = Some(row);
        } else {
            let text = format!(
                "{} {} {}",
                row["label"].as_str().unwrap_or_default(),
                row["suffix"].as_str().unwrap_or_default(),
                row["detail"].as_str().unwrap_or_default()
            )
            .to_lowercase();
            let hidden = !words.is_empty() && row["searchHidden"] == true;
            if hidden || !words.iter().all(|word| text.contains(word)) {
                continue;
            }
            if let Some(heading) = heading.take() {
                if !left.is_empty() {
                    left.push(serde_json::json!({"separator": true}));
                }
                left.push(heading.clone());
            }
            left.push(row.clone());
        }
    }
    let typed = query.trim();
    if let Some(submit) = rows
        .first()
        .and_then(|row| row["search"].get("submit"))
        .filter(|_| !typed.is_empty())
    {
        let mut command = submit["command"].clone();
        command["text"] = Value::from(typed);
        if !left.is_empty() {
            left.insert(0, serde_json::json!({"separator": true}));
        }
        left.insert(
            0,
            serde_json::json!({
                "label": format!("{} \"{typed}\"", submit["label"].as_str().unwrap_or_default()),
                "icon": submit["icon"],
                "command": command,
            }),
        );
    }
    if left.is_empty() {
        left.push(serde_json::json!({"label": "No matches", "disabled": true}));
    }
    left
}

/// The row's two-click button (a `secondary` with `confirm`), by what it runs: it stays armed by
/// that across a re-filter that moves the row.
fn confirm_id(item: &Value) -> Option<String> {
    let secondary = item.get("secondary")?;
    (secondary["confirm"] == true).then(|| secondary["command"].to_string())
}

#[derive(Clone)]
pub(crate) struct SidebarMenuPanel {
    pub(crate) items: Vec<Value>,
    pub(crate) scroll: gpui::ScrollHandle,
    pub(crate) anchor: Point<Pixels>,
    pub(crate) placement: SidebarMenuPlacement,
    pub(crate) selected: Option<usize>,
    pub(crate) pages: Vec<Vec<Value>>,
    /// Border-box height of the rendered rows, measured after layout.
    pub(crate) measured_height: Option<Pixels>,
    /// The row of this panel that opened the panel stacked on top of it, if one is open.
    pub(crate) child_item: Option<usize>,
    /// A dropdown's tallest height (scaled): it stays under what opened it, never taller than this
    /// or the room below, and its rows scroll inside it. `None` lets the panel grow to the sidebar.
    pub(crate) max_height: Option<Pixels>,
    /// Set for a menu that narrows as the user types.
    pub(crate) search: Option<SidebarMenuSearch>,
    /// The row whose two-click button took its first click (`confirm_id`), and which arming
    /// that was, so an older arming's timer doesn't disarm a newer one.
    pub(crate) armed: Option<(String, u64)>,
    /// How many times the list was sent back to its top, which a removal must not do. The handle
    /// only moves when it next lays out, so a test can't read the move from it.
    #[cfg(test)]
    pub(crate) back_to_top: u32,
}

pub(crate) struct SidebarMenuState {
    pub(crate) window: gpui::AnyWindowHandle,
    pub(crate) account_panel: Option<(String, usize)>,
    pub(crate) panels: Vec<SidebarMenuPanel>,
    pub(crate) focus: FocusHandle,
    pub(crate) previous_focus: Option<FocusHandle>,
    /// The button that opened the menu, when a button did: pressing it again closes the menu.
    pub(crate) trigger: Option<String>,
    pub(crate) scale: f32,
    /// Keeps `window`'s tooltips hidden while the menu is up (`Root::suppress_tooltips`). `None`
    /// only while the pointer is on a row's (i), whose tooltip is the one exception.
    pub(crate) tooltips: Option<gpui::TooltipSuppression>,
}

impl SidebarMenuPanel {
    pub(crate) fn new(items: Vec<Value>, anchor: Point<Pixels>) -> Self {
        Self {
            items,
            scroll: Default::default(),
            anchor,
            placement: SidebarMenuPlacement::Pointer,
            selected: None,
            pages: vec![],
            measured_height: None,
            child_item: None,
            max_height: None,
            search: None,
            armed: None,
            #[cfg(test)]
            back_to_top: 0,
        }
    }

    /// Makes the panel searchable when its first row asks for it, with the first row picked as
    /// Quick Access's pickers start.
    pub(crate) fn with_requested_search(mut self) -> Self {
        let Some(placeholder) = self
            .items
            .first()
            .and_then(|item| item["search"]["placeholder"].as_str())
            .map(str::to_owned)
        else {
            return self;
        };
        self.search = Some(SidebarMenuSearch {
            placeholder,
            query: String::new(),
            rows: self.items.clone(),
        });
        self.select_first_row();
        self
    }

    fn select_first_row(&mut self) {
        self.selected = self.items.iter().position(|item| {
            item["separator"] != true && item["heading"] != true && item["disabled"] != true
        });
    }

    /// A selectable row's height, before the zoom. A searchable menu (the Unified filter dropdown)
    /// lists like the Unified sidebar's session cards, so it shares their height and gap and
    /// follows them. The other sidebar menus keep their own.
    ///
    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User: "please make the height of the projects in the list here match the height of the sessions in the sidebar and make the buttons same height and width", then, asked which sessions: "Match Unified's tall cards". Rows take the height of a full card (`UNIFIED_CARD_HEIGHT`, the most common one), and the X the size of a card's hover buttons. Headings, separators and the search field keep their own heights.
    pub(crate) fn row_height(&self) -> f32 {
        if self.search.is_some() {
            super::unified::UNIFIED_CARD_HEIGHT
        } else {
            SIDEBAR_MENU_ROW_HEIGHT
        }
    }

    /// The gap between rows, before the zoom.
    pub(crate) fn row_gap(&self) -> f32 {
        if self.search.is_some() {
            super::unified::UNIFIED_CARD_GAP
        } else {
            0.0
        }
    }

    /// Whether this row's two-click button is waiting for its second click.
    pub(crate) fn row_is_armed(&self, item: &Value) -> bool {
        self.armed
            .as_ref()
            .is_some_and(|(armed, _)| confirm_id(item).as_ref() == Some(armed))
    }

    /// The first click of a two-click button, or `false` when `item` has none.
    pub(crate) fn arm_row(&mut self, item: &Value, arming: u64) -> bool {
        let Some(id) = confirm_id(item) else {
            return false;
        };
        self.armed = Some((id, arming));
        true
    }

    /// Takes the query to `query`: the rows narrow, the first one is picked and the list goes back
    /// to its top, for a new set of results.
    fn set_search_query(&mut self, query: String) {
        let Some(search) = &mut self.search else {
            return;
        };
        search.query = query;
        self.narrow_rows();
        self.select_first_row();
        self.scroll.scroll_to_item(0);
        #[cfg(test)]
        {
            self.back_to_top += 1;
        }
    }

    /// The rows the current query leaves. An armed button stands down because its row may have
    /// gone. The scroll position is the caller's to change.
    fn narrow_rows(&mut self) {
        let Some(search) = &self.search else {
            return;
        };
        let rows = searched_menu_rows(&search.rows, &search.query);
        self.replace_items(rows);
        self.armed = None;
    }

    /// A key for the search field; `true` when it changed the query. Backspace takes off a letter;
    /// text without a command modifier is typed, a space included.
    pub(crate) fn search_key(&mut self, keystroke: &gpui::Keystroke) -> bool {
        let Some(search) = &self.search else {
            return false;
        };
        let mut query = search.query.clone();
        if keystroke.key == "backspace" {
            if query.pop().is_none() {
                return false;
            }
        } else if !keystroke.modifiers.platform
            && !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && let Some(text) = keystroke.key_char.as_deref()
            && text.chars().all(|character| !character.is_control())
        {
            query.push_str(text);
        } else {
            return false;
        }
        self.set_search_query(query);
        true
    }

    /// A row whose project has just been removed leaves the list, and the rest stay where they
    /// are: the query, the scroll position (the next layout only pulls it back if the list now
    /// ends above it) and the keyboard cursor's place.
    ///
    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User: "pleae dont scroll me to the top when i click on [the X] to confirm closing project". A removal is one row leaving a list you are working through, not new results, so it doesn't go through `set_search_query`'s return to the top.
    pub(crate) fn drop_confirmed_row(&mut self, item: &Value) {
        self.armed = None;
        let Some(id) = confirm_id(item) else {
            return;
        };
        let Some(search) = &mut self.search else {
            return;
        };
        search
            .rows
            .retain(|row| confirm_id(row).as_ref() != Some(&id));
        let selected = self.selected;
        self.narrow_rows();
        if let Some(selected) = selected {
            let enabled = |item: &Value| {
                item["separator"] != true && item["heading"] != true && item["disabled"] != true
            };
            self.selected = self
                .items
                .iter()
                .enumerate()
                .skip(selected)
                .find(|(_, item)| enabled(item))
                .or_else(|| {
                    self.items
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, item)| enabled(item))
                })
                .map(|(index, _)| index);
        }
    }

    /// The rows the panel's width is worked out from: all of them, so typing doesn't resize it.
    fn width_rows(&self) -> &[Value] {
        self.search
            .as_ref()
            .map_or(self.items.as_slice(), |search| search.rows.as_slice())
    }

    /// Swaps the rows in place; the old selection and measured height describe rows that are gone.
    pub(crate) fn replace_items(&mut self, items: Vec<Value>) {
        self.items = items;
        self.selected = None;
        self.measured_height = None;
        self.child_item = None;
    }

    pub(crate) fn is_agent_launcher(&self) -> bool {
        self.items
            .first()
            .is_some_and(|item| item["menuStyle"] == "agentLauncher")
    }

    /// CDXC:ContextMenus 2026-09-21 WHY:
    /// Compact session menus stay at 178px (nested 204px) to match React, but Configure Machines wraps onto a second line there. Grow to the longest label so every row stays on one line without widening short menus.
    fn fitted_width(&self, nested: bool) -> f32 {
        if self.is_agent_launcher() {
            return super::agent_launcher_menu::AGENT_LAUNCHER_MENU_WIDTH;
        }
        let min = if self.search.is_some() {
            // The field's placeholder has to fit.
            260.0
        } else if nested {
            204.0
        } else {
            178.0
        };
        // Panel padding 6+6, border 1+1, row inset 10+10, icon 16, gap 8.
        const CHROME: f32 = 58.0;
        // 13px UI font; slightly wide so a label cannot wrap after rounding.
        const PX_PER_CHAR: f32 = 8.0;
        self.width_rows()
            .iter()
            .filter(|item| item["separator"] != true)
            .map(|item| {
                let label = item["label"].as_str().unwrap_or("");
                let suffix = item["suffix"].as_str().unwrap_or("");
                let mut extra = 0.0;
                if item.get("children").is_some()
                    || item["command"]["type"] == "sessionAccounts"
                        && item["command"]["action"] == "load"
                {
                    extra += 18.0;
                }
                if item.get("secondary").is_some() {
                    extra += 36.0;
                }
                if item["checked"] == true {
                    extra += 22.0;
                }
                if item["supportsChat"] == true {
                    extra += 18.0;
                }
                CHROME
                    + extra
                    + (label.chars().count() + suffix.chars().count()) as f32 * PX_PER_CHAR
            })
            .fold(min, f32::max)
            .ceil()
    }

    /// CDXC:AgentLauncher 2026-09-19 DECISION:
    /// User: the GPUI Select Agent menu and its account page must fit every account without cutting off the last row, like the React sidebar.
    /// The panel takes the measured height of its rows; the per-row estimate only places the first frame, because two-line account rows and wrapped hints outgrow any fixed row height.
    pub(crate) fn bounds(
        &self,
        sidebar: Bounds<Pixels>,
        scale: f32,
        nested: bool,
    ) -> Bounds<Pixels> {
        let margin = gpui::px(12.0 * scale);
        let launcher = self.is_agent_launcher();
        let width = gpui::px(self.fitted_width(nested) * scale)
            .min((sidebar.size.width - margin * 2.0).max(gpui::px(0.0)));
        let estimate = if launcher {
            super::agent_launcher_menu::estimated_height(&self.items, width, scale)
        } else {
            gpui::px(
                (14.0
                    + self
                        .search
                        .as_ref()
                        .map_or(0.0, |_| SIDEBAR_MENU_SEARCH_HEIGHT)
                    + self.row_gap() * self.items.len().saturating_sub(1) as f32
                    + self
                        .items
                        .iter()
                        .map(|item| {
                            if item["separator"] == true {
                                13.0
                            } else if item["heading"] == true {
                                24.0
                            } else {
                                self.row_height()
                            }
                        })
                        .sum::<f32>())
                    * scale,
            )
        };
        let height = self
            .measured_height
            .unwrap_or(estimate)
            .min((sidebar.size.height - margin * 2.0).max(gpui::px(0.0)));
        if let SidebarMenuPlacement::Attached {
            owner,
            from,
            align_right,
            gap,
        } = self.placement
        {
            // No margin against the sidebar's sides: the owner sits inside them, and the panel's
            // edge has to meet the owner's exactly.
            let left =
                menu_left_at_button(from, align_right, sidebar.left(), sidebar.right(), width);
            let below = owner.bottom() + gap;
            if let Some(max_height) = self.max_height {
                let room = (sidebar.bottom() - margin - below).max(gpui::px(0.0));
                return Bounds {
                    origin: Point::new(left, below),
                    size: gpui::size(width, height.min(max_height).min(room)),
                };
            }
            let above = owner.top() - gap - height;
            let top = if below + height <= sidebar.bottom() - margin {
                below
            } else if above >= sidebar.top() + margin {
                above
            } else if sidebar.bottom() - owner.bottom() >= owner.top() - sidebar.top() {
                below.min(sidebar.bottom() - height - margin)
            } else {
                above.max(sidebar.top() + margin)
            };
            return Bounds {
                origin: Point::new(left, top),
                size: gpui::size(width, height),
            };
        }
        // The React launcher sits right-aligned under its chevron; other sidebar menus are centered in the sidebar.
        let left = if launcher {
            (self.anchor.x - width)
                .min(sidebar.right() - width - margin)
                .max(sidebar.left() + margin)
        } else {
            sidebar.left() + (sidebar.size.width - width) / 2.0
        };
        Bounds {
            origin: Point::new(
                left,
                self.anchor
                    .y
                    .min(sidebar.bottom() - height - margin)
                    .max(sidebar.top() + margin),
            ),
            size: gpui::size(width, height),
        }
    }

    pub(crate) fn move_selection(&mut self, key: &str) {
        let enabled: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item["separator"] != true && item["heading"] != true && item["disabled"] != true
            })
            .map(|(index, _)| index)
            .collect();
        if enabled.is_empty() {
            return;
        }
        let current = self
            .selected
            .and_then(|selected| enabled.iter().position(|index| *index == selected));
        let index = match key {
            "home" => 0,
            "end" => enabled.len() - 1,
            "up" => current
                .map(|index| (index + enabled.len() - 1) % enabled.len())
                .unwrap_or(enabled.len() - 1),
            _ => current
                .map(|index| (index + 1) % enabled.len())
                .unwrap_or(0),
        };
        self.selected = Some(enabled[index]);
        self.scroll.scroll_to_item(enabled[index]);
    }
}

impl SidebarMenuState {
    /// Whether the button `trigger` opened this menu.
    pub(crate) fn opened_by(&self, trigger: &str) -> bool {
        self.trigger.as_deref() == Some(trigger)
    }

    pub(crate) fn refresh(&mut self, snapshot: &super::model::NativeSidebarSnapshot) {
        fn collect(value: &Value, items: &mut std::collections::HashMap<String, Value>) {
            match value {
                Value::Array(values) => {
                    for value in values {
                        collect(value, items);
                    }
                }
                Value::Object(object) => {
                    if let Some(command) = object.get("command") {
                        items.insert(command.to_string(), value.clone());
                    }
                    if let Some(children) = object.get("children") {
                        collect(children, items);
                    }
                }
                _ => {}
            }
        }
        let mut current = std::collections::HashMap::new();
        collect(&snapshot.more_menu, &mut current);
        for group in &snapshot.groups {
            collect(&group.menu, &mut current);
        }
        for collection in &snapshot.collections {
            collect(&collection.menu, &mut current);
        }
        for panel in &mut self.panels {
            for item in panel
                .items
                .iter_mut()
                .chain(panel.pages.iter_mut().flatten())
            {
                if let Some(updated) = item
                    .get("command")
                    .and_then(|command| current.get(&command.to_string()))
                {
                    for key in ["label", "checked", "disabled"] {
                        if let Some(value) = updated.get(key) {
                            item[key] = value.clone();
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

    fn dropdown(rows: usize) -> SidebarMenuPanel {
        let items = (0..rows)
            .map(|row| serde_json::json!({"label": format!("Project {row}")}))
            .collect();
        let owner = Bounds::new(Point::new(px(8.0), px(40.0)), size(px(200.0), px(28.0)));
        SidebarMenuPanel {
            placement: SidebarMenuPlacement::Attached {
                owner,
                from: owner,
                align_right: false,
                gap: px(3.0),
            },
            max_height: Some(px(420.0)),
            ..SidebarMenuPanel::new(items, Point::default())
        }
    }

    fn sidebar(height: f32) -> Bounds<Pixels> {
        Bounds::new(Point::default(), size(px(300.0), px(height)))
    }

    fn filter_rows() -> Vec<Value> {
        let remove = |key: &str| {
            serde_json::json!({
                "icon": "x",
                "label": "",
                "confirm": true,
                "command": {"type": "unifiedRemoveProject", "key": key},
            })
        };
        serde_json::json!([
            {
                "label": "All sessions",
                "searchHidden": true,
                "search": {"placeholder": "Search"},
                "command": {"type": "unifiedFilter", "kind": "clear"},
            },
            {"separator": true},
            {"label": "Machines", "heading": true},
            {"label": "Local", "command": {"kind": "machine", "key": "local"}},
            {"label": "Windows", "command": {"kind": "machine", "key": "windows"}},
            {"separator": true},
            {"label": "Spaces", "heading": true},
            {"label": "My Apps", "command": {"kind": "space", "key": "a"}},
            {"label": "Active", "detail": "Windows", "command": {"kind": "space", "key": "b"}},
            {"separator": true},
            {"label": "Projects", "heading": true},
            {"label": "Ghostex", "detail": "Local, Windows", "secondary": remove("ghostex"),
             "command": {"kind": "repo", "key": "ghostex"}},
            {"label": "my-apps", "secondary": remove("my-apps"),
             "command": {"kind": "repo", "key": "my-apps"}},
        ])
        .as_array()
        .unwrap()
        .clone()
    }

    fn labels(rows: &[Value]) -> Vec<&str> {
        rows.iter()
            .map(|row| {
                if row["separator"] == true {
                    "--"
                } else {
                    row["label"].as_str().unwrap_or_default()
                }
            })
            .collect()
    }

    fn searchable() -> SidebarMenuPanel {
        SidebarMenuPanel::new(filter_rows(), Point::default()).with_requested_search()
    }

    fn typed(text: &str) -> gpui::Keystroke {
        gpui::Keystroke {
            key: text.to_string(),
            key_char: Some(text.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_query_leaves_every_row() {
        assert_eq!(
            labels(&searched_menu_rows(&filter_rows(), "  ")),
            labels(&filter_rows())
        );
    }

    #[test]
    fn typing_narrows_spaces_and_projects_under_their_own_headings() {
        assert_eq!(
            labels(&searched_menu_rows(&filter_rows(), "my")),
            ["Spaces", "My Apps", "--", "Projects", "my-apps"]
        );
    }

    #[test]
    fn typing_finds_another_machines_entries_by_its_name() {
        // Words match across a row's name and its machine, in any order and case.
        assert_eq!(
            labels(&searched_menu_rows(&filter_rows(), "WINDOWS act")),
            ["Spaces", "Active"]
        );
        assert_eq!(
            labels(&searched_menu_rows(&filter_rows(), "windows")),
            [
                "Machines", "Windows", "--", "Spaces", "Active", "--", "Projects", "Ghostex"
            ]
        );
    }

    #[test]
    fn nothing_matching_says_so_and_cannot_be_picked() {
        let rows = searched_menu_rows(&filter_rows(), "zzz");
        assert_eq!(labels(&rows), ["No matches"]);
        assert_eq!(rows[0]["disabled"], true);
    }

    fn with_text_search() -> Vec<Value> {
        let mut rows = filter_rows();
        rows[0]["search"]["submit"] = serde_json::json!({
            "label": "Search sessions for",
            "icon": "search",
            "command": {"type": "unifiedFilter", "kind": "text"},
        });
        rows
    }

    #[test]
    fn a_typed_text_is_offered_first_with_the_matches_under_it() {
        let rows = searched_menu_rows(&with_text_search(), " my ");
        assert_eq!(
            labels(&rows),
            [
                "Search sessions for \"my\"",
                "--",
                "Spaces",
                "My Apps",
                "--",
                "Projects",
                "my-apps"
            ]
        );
        assert_eq!(rows[0]["command"]["text"], "my");
    }

    #[test]
    fn a_text_nothing_matches_is_still_offered() {
        let rows = searched_menu_rows(&with_text_search(), "zzz");
        assert_eq!(labels(&rows), ["Search sessions for \"zzz\""]);
        assert_ne!(rows[0]["disabled"], true);
    }

    #[test]
    fn nothing_typed_offers_no_text() {
        assert_eq!(
            labels(&searched_menu_rows(&with_text_search(), "  ")),
            labels(&filter_rows())
        );
    }

    #[test]
    fn enter_takes_the_text_and_down_reaches_the_matches() {
        let mut panel =
            SidebarMenuPanel::new(with_text_search(), Point::default()).with_requested_search();
        panel.search_key(&typed("g"));
        assert_eq!(
            panel.items[panel.selected.unwrap()]["command"]["kind"],
            "text"
        );
        panel.move_selection("down");
        assert_eq!(panel.items[panel.selected.unwrap()]["label"], "Ghostex");
    }

    #[test]
    fn the_cursor_starts_on_the_first_row_and_follows_the_matches() {
        let mut panel = searchable();
        assert_eq!(panel.selected, Some(0));
        assert!(panel.search_key(&typed("g")));
        assert_eq!(panel.items[panel.selected.unwrap()]["label"], "Ghostex");
        assert!(panel.search_key(&gpui::Keystroke {
            key: "backspace".to_string(),
            ..Default::default()
        }));
        assert_eq!(panel.search.as_ref().unwrap().query, "");
        assert_eq!(
            panel.items[panel.selected.unwrap()]["label"],
            "All sessions"
        );
        // Nothing left to take off: Backspace is not the field's.
        assert!(!panel.search_key(&gpui::Keystroke {
            key: "backspace".to_string(),
            ..Default::default()
        }));
    }

    #[test]
    fn a_space_is_typed_and_a_command_key_is_not() {
        let mut panel = searchable();
        assert!(panel.search_key(&typed("a")));
        assert!(panel.search_key(&gpui::Keystroke {
            key: "space".to_string(),
            key_char: Some(" ".to_string()),
            ..Default::default()
        }));
        assert_eq!(panel.search.as_ref().unwrap().query, "a ");
        assert!(!panel.search_key(&gpui::Keystroke {
            key: "a".to_string(),
            key_char: Some("a".to_string()),
            modifiers: gpui::Modifiers {
                platform: true,
                ..Default::default()
            },
            ..Default::default()
        }));
        assert!(!panel.search_key(&gpui::Keystroke {
            key: "enter".to_string(),
            key_char: Some("\r".to_string()),
            ..Default::default()
        }));
    }

    #[test]
    fn a_panel_that_did_not_ask_for_search_is_left_alone() {
        let items = vec![serde_json::json!({"label": "Rename"})];
        let mut panel = SidebarMenuPanel::new(items, Point::default()).with_requested_search();
        assert!(panel.search.is_none());
        assert_eq!(panel.selected, None);
        assert!(!panel.search_key(&typed("r")));
    }

    #[test]
    fn typing_stands_an_armed_button_down() {
        let mut panel = searchable();
        let row = panel.items[11].clone();
        assert!(panel.arm_row(&row, 1));
        assert!(panel.row_is_armed(&row));
        assert!(!panel.row_is_armed(&panel.items[12].clone()));
        panel.search_key(&typed("g"));
        assert!(!panel.row_is_armed(&row));
    }

    #[test]
    fn only_a_row_with_a_confirm_button_arms() {
        let mut panel = searchable();
        let space = panel.items[7].clone();
        assert!(!panel.arm_row(&space, 1));
        assert!(panel.armed.is_none());
    }

    #[test]
    fn a_removed_row_leaves_and_the_query_stays() {
        let mut panel = searchable();
        panel.search_key(&typed("m"));
        panel.search_key(&typed("y"));
        let row = panel
            .items
            .iter()
            .find(|row| row["label"] == "my-apps")
            .unwrap()
            .clone();
        panel.arm_row(&row, 1);
        panel.drop_confirmed_row(&row);
        assert!(panel.armed.is_none());
        assert_eq!(labels(&panel.items), ["Spaces", "My Apps"]);
        assert_eq!(panel.search.as_ref().unwrap().query, "my");
        assert_eq!(panel.items[panel.selected.unwrap()]["label"], "My Apps");
        // A later search does not bring it back.
        panel.set_search_query(String::new());
        assert!(!panel.items.iter().any(|item| item["label"] == "my-apps"));
    }

    fn row_labelled(panel: &SidebarMenuPanel, label: &str) -> Value {
        panel
            .items
            .iter()
            .find(|row| row["label"] == label)
            .unwrap()
            .clone()
    }

    #[test]
    fn a_removal_keeps_the_scroll_where_typing_goes_back_to_the_top() {
        let mut panel = searchable();
        let scrolled = Point::new(px(0.0), px(-240.0));
        panel.scroll.set_offset(scrolled);
        let row = row_labelled(&panel, "Ghostex");
        panel.arm_row(&row, 1);
        panel.drop_confirmed_row(&row);
        assert_eq!(panel.back_to_top, 0, "the removal sent the list to its top");
        assert_eq!(panel.scroll.offset(), scrolled);
        panel.search_key(&typed("m"));
        assert_eq!(panel.back_to_top, 1);
    }

    #[test]
    fn removing_the_last_row_keeps_the_scroll_and_the_cursor_on_what_is_left() {
        let mut panel = searchable();
        let scrolled = Point::new(px(0.0), px(-240.0));
        panel.scroll.set_offset(scrolled);
        let last = row_labelled(&panel, "my-apps");
        panel.selected = panel.items.iter().position(|row| row["label"] == "my-apps");
        panel.arm_row(&last, 1);
        panel.drop_confirmed_row(&last);
        assert_eq!(panel.back_to_top, 0);
        assert_eq!(panel.scroll.offset(), scrolled);
        assert_eq!(panel.items[panel.selected.unwrap()]["label"], "Ghostex");
    }

    #[test]
    fn removing_a_row_in_a_search_keeps_the_scroll_too() {
        let mut panel = searchable();
        panel.search_key(&typed("m"));
        panel.search_key(&typed("y"));
        let resets = panel.back_to_top;
        let row = row_labelled(&panel, "my-apps");
        panel.drop_confirmed_row(&row);
        assert_eq!(panel.back_to_top, resets);
    }

    #[test]
    fn the_cursor_stays_in_place_after_a_removal_above_it() {
        let mut panel = searchable();
        let first = row_labelled(&panel, "Ghostex");
        panel.selected = panel.items.iter().position(|row| row["label"] == "Ghostex");
        panel.drop_confirmed_row(&first);
        // The row that was under it moved up into its place.
        assert_eq!(panel.items[panel.selected.unwrap()]["label"], "my-apps");
    }

    #[test]
    fn the_filter_dropdowns_rows_are_a_cards_height_and_other_menus_keep_theirs() {
        let panel = searchable();
        assert_eq!(
            panel.row_height(),
            crate::app::native_sidebar::unified::UNIFIED_CARD_HEIGHT
        );
        assert_eq!(
            panel.row_gap(),
            crate::app::native_sidebar::unified::UNIFIED_CARD_GAP
        );
        let other = SidebarMenuPanel::new(
            vec![serde_json::json!({"label": "Rename"})],
            Point::default(),
        );
        assert_eq!(other.row_height(), SIDEBAR_MENU_ROW_HEIGHT);
        assert_eq!(other.row_gap(), 0.0);
    }

    #[test]
    fn the_panels_height_counts_the_field_the_rows_and_their_gaps() {
        let mut panel = searchable();
        panel.search_key(&typed("m"));
        panel.search_key(&typed("y"));
        // Spaces, My Apps, a separator, Projects, my-apps.
        let card = crate::app::native_sidebar::unified::UNIFIED_CARD_HEIGHT;
        let rows = 24.0 + card + 13.0 + 24.0 + card;
        let gaps = 4.0 * crate::app::native_sidebar::unified::UNIFIED_CARD_GAP;
        let expected = 14.0 + SIDEBAR_MENU_SEARCH_HEIGHT + rows + gaps;
        assert_eq!(
            panel.bounds(sidebar(1000.0), 1.0, false).size.height,
            px(expected)
        );
        // The zoom scales all of it.
        assert_eq!(
            panel.bounds(sidebar(1000.0), 1.5, false).size.height,
            px(expected * 1.5)
        );
    }

    #[test]
    fn a_tall_filter_dropdown_stops_at_its_cap_and_never_passes_the_window() {
        let owner = Bounds::new(Point::new(px(8.0), px(40.0)), size(px(200.0), px(28.0)));
        let mut panel = searchable();
        panel.placement = SidebarMenuPlacement::Attached {
            owner,
            from: owner,
            align_right: false,
            gap: px(3.0),
        };
        panel.measured_height = Some(px(5000.0));
        panel.max_height = Some(px(675.0));
        let tall = panel.bounds(sidebar(900.0), 1.0, false);
        assert_eq!(tall.top(), px(71.0));
        assert_eq!(tall.size.height, px(675.0));
        // A short window: the room below wins.
        let short = panel.bounds(sidebar(500.0), 1.0, false);
        assert_eq!(short.bottom(), px(500.0 - 12.0));
    }

    #[test]
    fn the_width_does_not_change_as_rows_narrow() {
        let mut panel = searchable();
        let wide = panel.bounds(sidebar(1000.0), 1.0, false).size.width;
        panel.search_key(&typed("z"));
        assert_eq!(panel.bounds(sidebar(1000.0), 1.0, false).size.width, wide);
    }

    #[test]
    fn a_long_dropdown_hangs_under_its_button_and_stops_at_its_cap() {
        let bounds = dropdown(60).bounds(sidebar(1000.0), 1.0, false);
        assert_eq!(bounds.top(), px(71.0));
        assert_eq!(bounds.left(), px(8.0));
        assert_eq!(bounds.size.height, px(420.0));
    }

    #[test]
    fn a_short_dropdown_shrinks_to_its_rows() {
        let bounds = dropdown(3).bounds(sidebar(1000.0), 1.0, false);
        assert_eq!(bounds.top(), px(71.0));
        assert_eq!(bounds.size.height, px(14.0 + 3.0 * 34.0));
    }

    #[test]
    fn a_dropdown_never_runs_past_the_window() {
        let bounds = dropdown(60).bounds(sidebar(400.0), 1.0, false);
        assert_eq!(bounds.top(), px(71.0));
        assert_eq!(bounds.bottom(), px(400.0 - 12.0));
    }
}

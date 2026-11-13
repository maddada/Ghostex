//! The value types the view panel's tab strip drags around: what is being dragged, where it would
//! land, and the little tab that follows the pointer.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::*;

/// The payload GPUI carries while a view tab is being dragged along the strip.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct DraggedViewTab {
    pub(crate) mode: TitlebarMode,
}

/// The payload GPUI carries while a side chat tab is being dragged along the strip.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct DraggedSideChatTab {
    pub(crate) tab_id: SideChatTabId,
}

/// One tab of the view panel's strip, whichever kind it is: a view, one of the Browser's pages, or
/// a side chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ViewStripTabKey {
    View(TitlebarMode),
    Browser(BrowserTabId),
    SideChat(SideChatTabId),
}

/// A side chat's tab, standing for the side chat's session key (`SideChatTabId::key`). The strip's
/// keys are `Copy`, so the session key is interned once per app run and the tab carries its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SideChatTabId(pub(crate) u64);

fn side_chat_tab_keys() -> &'static std::sync::Mutex<Vec<ghostex_gx_core::SessionKey>> {
    static KEYS: std::sync::OnceLock<std::sync::Mutex<Vec<ghostex_gx_core::SessionKey>>> =
        std::sync::OnceLock::new();
    KEYS.get_or_init(Default::default)
}

impl SideChatTabId {
    /// The tab id of `key`, the same one every time this app run asks.
    pub(crate) fn for_key(key: &ghostex_gx_core::SessionKey) -> Self {
        let mut keys = side_chat_tab_keys()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let index = match keys.iter().position(|known| known == key) {
            Some(index) => index,
            None => {
                keys.push(key.clone());
                keys.len() - 1
            }
        };
        Self(index as u64 + 1)
    }

    /// The side chat's session.
    pub(crate) fn key(self) -> Option<ghostex_gx_core::SessionKey> {
        let keys = side_chat_tab_keys()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        keys.get(usize::try_from(self.0.checked_sub(1)?).ok()?)
            .cloned()
    }

    /// The saved form: the sidebar session id, which names the machine, project and session.
    fn to_shell_state_string(self) -> Option<String> {
        self.key().map(|key| key.to_sidebar_session_id())
    }

    fn from_shell_state_string(value: &str) -> Option<Self> {
        ghostex_gx_core::SessionKey::parse_sidebar_session_id(value).map(|key| Self::for_key(&key))
    }
}

impl ViewStripTabKey {
    fn to_shell_state_string(self) -> Option<String> {
        Some(match self {
            Self::View(mode) => format!("view:{}", mode.element_slug()),
            Self::Browser(tab_id) => format!("browser:{}", tab_id.0),
            Self::SideChat(tab_id) => format!("sideChat:{}", tab_id.to_shell_state_string()?),
        })
    }

    /// The Browser's own key is kept: it places the strip's Browser tab while its pages are in a
    /// row of their own (`browserPagesInViewRow`), and is skipped while they are not.
    fn from_shell_state_string(value: &str) -> Option<Self> {
        if let Some(slug) = value.strip_prefix("view:") {
            return TitlebarMode::from_slug(slug)
                .filter(|mode| !matches!(mode, TitlebarMode::Agents | TitlebarMode::SideChat))
                .map(Self::View);
        }
        if let Some(session) = value.strip_prefix("sideChat:") {
            return SideChatTabId::from_shell_state_string(session).map(Self::SideChat);
        }
        value
            .strip_prefix("browser:")
            .and_then(|id| id.parse::<u64>().ok())
            .map(|id| Self::Browser(BrowserTabId(id)))
    }
}

/// CDXC:Workarea 2026-09-21 DECISION:
/// User (ruling 1A): the strip is one row in one order, "any tab can be dragged anywhere, views and
/// pages mixed", and (ruling 4B) tabs can be pinned: icon-only, kept at the left, not closable by
/// accident. `open_views` and the Browser's panes still say which tabs exist; this only says where
/// each is drawn and which are pinned. A tab it has not seen yet lands at the end. It is
/// project-owned like the open views are, because browser tab ids are per project. With
/// `browserPagesInViewRow` on (off by default) the pages are drawn and dragged in their own row
/// (`render/browser_page_tabs.rs`) but keep their places in this one order.
///
/// CDXC:SideChats 2026-11-02 DECISION:
/// User: side chats appear in the side panel instead of using what the agent CLI offers; several can be open as tabs. A side chat's tab exists exactly while its key is in `order` (nothing else lists them, so `write_projection` never drops one), and `active_side_chat` says which one `TitlebarMode::SideChat` shows. Both are saved per project by ids only; the title is read from the store each time the tab is drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GpuiViewStripLayout {
    pub(crate) order: Vec<ViewStripTabKey>,
    pub(crate) pinned: Vec<ViewStripTabKey>,
    pub(crate) active_side_chat: Option<SideChatTabId>,
}

/// The most tabs `GpuiViewStripLayout::in_drawn_order` places by scanning the stored order; see its
/// WHY.
const RANKED_VIEW_STRIP_TABS: usize = 32;

impl GpuiViewStripLayout {
    /// `available` (the tabs to draw, in their natural order) in drawn order: pinned tabs first,
    /// then by place in the one stored order the strip and the Browser's page row share, a tab it
    /// does not name last; ties keep `available`'s own order. The one rule for the strip on screen
    /// (`in_view_strip_order`) and for a saved strip. With `browser_tab_stands_in` (the pages have
    /// a row of their own) a Browser tab the order has not placed yet stands where its first page
    /// stood.
    ///
    /// CDXC:Workarea 2026-10-25 WHY:
    /// Up to `RANKED_VIEW_STRIP_TABS` tabs each tab's place is a scan of the stored order, then one stable sort; past that the stored order takes its tabs out of the list one by one (the 2026-09-21 loop). Measured, best of 7 in a release build, ns per call for ordered/shuffled tabs with 2 pins: 4 tabs 49/49 against the loop's 95/91, 32 tabs 454/517 against 462/578, 40 tabs 594/809 against 610/748, 64 tabs 1,290/1,636 against 921/1,303, 128 tabs 4,771/5,264 against 2,427/3,470. A `HashMap` of places (std's SipHash, the only map this crate uses) was slower than the loop at every size up to 128, and finding places on a shrinking `swap_remove` list was slower than the scan up to 16 tabs and than the loop from 64.
    pub(crate) fn in_drawn_order<T>(
        &self,
        available: Vec<T>,
        browser_tab_stands_in: bool,
        key: impl Fn(&T) -> ViewStripTabKey,
    ) -> Vec<T> {
        let order = &self.order;
        let browser_tab = ViewStripTabKey::View(TitlebarMode::Browser);
        let browser_tab_unplaced = browser_tab_stands_in && !order.contains(&browser_tab);
        let names = |stored: &ViewStripTabKey, key: ViewStripTabKey| {
            *stored == key
                || browser_tab_unplaced
                    && key == browser_tab
                    && matches!(stored, ViewStripTabKey::Browser(_))
        };
        if available.len() > RANKED_VIEW_STRIP_TABS {
            let mut remaining = available;
            let mut ordered = Vec::with_capacity(remaining.len());
            for stored in order {
                if let Some(index) = remaining.iter().position(|entry| names(stored, key(entry))) {
                    ordered.push(remaining.remove(index));
                }
            }
            ordered.append(&mut remaining);
            let (pinned, unpinned): (Vec<_>, Vec<_>) = ordered
                .into_iter()
                .partition(|entry| self.pinned.contains(&key(entry)));
            return pinned.into_iter().chain(unpinned).collect();
        }
        let mut placed = available
            .into_iter()
            .map(|entry| {
                let key = key(&entry);
                let place = order.iter().position(|stored| names(stored, key));
                (
                    !self.pinned.contains(&key),
                    place.unwrap_or(usize::MAX),
                    entry,
                )
            })
            .collect::<Vec<_>>();
        placed.sort_by_key(|(unpinned, place, _)| (*unpinned, *place));
        placed.into_iter().map(|(_, _, entry)| entry).collect()
    }

    /// Writes `drawn`, a row (the strip, or the Browser's page row) in its new drawn order, back
    /// into the one stored order around the tabs that row does not draw right now (a view hidden in
    /// this project or one this project cannot show, a page that has not loaded, the other row's
    /// tabs): each stored slot that holds one of `drawn`'s keys takes the next of them, every other
    /// tab keeps its slot, and a key the order had not placed yet goes last. Keys whose tab is gone
    /// for good (`still_open` says no) are dropped here.
    ///
    /// CDXC:Workarea 2026-10-25 WHY:
    /// The order used to be stored as the drawn tabs followed by the ones not drawn, so any drag, pin or new tab while a view was hidden here sent that view to the end of the strip when it came back. Filling only the drawn tabs' slots keeps the place `open_view_tabs` promises it, and lets the strip and the Browser's page row share one order, each moving only its own tabs.
    pub(crate) fn write_projection(
        &mut self,
        drawn: &[ViewStripTabKey],
        still_open: impl Fn(&ViewStripTabKey) -> bool,
    ) {
        self.order.retain(still_open);
        let mut next = drawn.iter().copied();
        for slot in &mut self.order {
            if drawn.contains(slot)
                && let Some(key) = next.next()
            {
                *slot = key;
            }
        }
        self.order.extend(next);
    }

    /// `open_views` (a project's open-views list, views it cannot show right now included) in the
    /// order its strip draws them. While the Browser's pages are drawn in the strip (`pages` holds
    /// those drawn, `pages_in_row` off), Browser stands where its first page is drawn; with none of
    /// them drawn it keeps its own slot in `open_views`, after the view it follows there.
    pub(crate) fn open_views_in_drawn_order(
        &self,
        open_views: &[TitlebarMode],
        pages: &[BrowserTabId],
        pages_in_row: bool,
    ) -> Vec<TitlebarMode> {
        let views = open_views
            .iter()
            .filter(|mode| pages_in_row || **mode != TitlebarMode::Browser)
            .map(|mode| ViewStripTabKey::View(*mode));
        let pages = pages
            .iter()
            .filter(|_| !pages_in_row)
            .map(|tab_id| ViewStripTabKey::Browser(*tab_id));
        let mut modes = Vec::with_capacity(open_views.len());
        for key in self.in_drawn_order(views.chain(pages).collect(), pages_in_row, |key| *key) {
            let mode = match key {
                ViewStripTabKey::View(mode) => mode,
                ViewStripTabKey::Browser(_) => TitlebarMode::Browser,
                ViewStripTabKey::SideChat(_) => TitlebarMode::SideChat,
            };
            if !modes.contains(&mode) {
                modes.push(mode);
            }
        }
        if let Some(index) = open_views
            .iter()
            .position(|mode| *mode == TitlebarMode::Browser)
            && !modes.contains(&TitlebarMode::Browser)
        {
            let place = open_views[..index]
                .iter()
                .rev()
                .find_map(|before| modes.iter().position(|mode| mode == before))
                .map_or(0, |before| before + 1);
            modes.insert(place, TitlebarMode::Browser);
        }
        modes
    }

    /// The side chats open in this strip, in stored order.
    pub(crate) fn side_chat_tabs(&self) -> impl Iterator<Item = SideChatTabId> + '_ {
        self.order.iter().filter_map(|key| match key {
            ViewStripTabKey::SideChat(tab_id) => Some(*tab_id),
            _ => None,
        })
    }

    pub(crate) fn has_side_chat_tab(&self, tab_id: SideChatTabId) -> bool {
        self.order.contains(&ViewStripTabKey::SideChat(tab_id))
    }

    pub(crate) fn to_shell_state_json(&self) -> serde_json::Value {
        let keys = |keys: &[ViewStripTabKey]| {
            keys.iter()
                .filter_map(|key| key.to_shell_state_string().map(serde_json::Value::String))
                .collect::<Vec<_>>()
        };
        let mut value = serde_json::json!({
            "order": keys(&self.order),
            "pinned": keys(&self.pinned),
        });
        if let Some(session) = self
            .active_side_chat
            .and_then(SideChatTabId::to_shell_state_string)
        {
            value["activeSideChat"] = serde_json::Value::String(session);
        }
        value
    }

    pub(crate) fn from_shell_state(value: Option<&serde_json::Value>) -> Self {
        let keys = |field: &str| {
            let mut keys = Vec::new();
            let entries = value
                .and_then(|value| value.get(field))
                .and_then(serde_json::Value::as_array);
            for entry in entries.into_iter().flatten() {
                if let Some(key) = entry
                    .as_str()
                    .and_then(ViewStripTabKey::from_shell_state_string)
                    && !keys.contains(&key)
                {
                    keys.push(key);
                }
            }
            keys
        };
        let order = keys("order");
        let active_side_chat = value
            .and_then(|value| value.get("activeSideChat"))
            .and_then(serde_json::Value::as_str)
            .and_then(SideChatTabId::from_shell_state_string)
            .filter(|tab_id| order.contains(&ViewStripTabKey::SideChat(*tab_id)));
        Self {
            order,
            pinned: keys("pinned"),
            active_side_chat,
        }
    }
}

/// The tab that follows the pointer during a reorder. It is a plain label, not the live tab, because
/// the live tab keeps rendering in the strip while it is dragged.
pub(crate) struct ViewTabDragPreview {
    pub(crate) icon: &'static str,
    pub(crate) label: String,
}

impl Render for ViewTabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        drag_ghost_surface(div(), 1.0)
            .flex()
            .h(px(WORKAREA_VIEW_TAB_HEIGHT))
            .w(px(WORKAREA_VIEW_TAB_WIDTH))
            .items_center()
            .gap(px(6.0))
            .overflow_hidden()
            .px(px(WORKAREA_VIEW_TAB_HORIZONTAL_PADDING))
            .text_size(px(12.5))
            .text_color(titlebar_active_text_color())
            .child(titlebar_svg_icon(
                self.icon,
                WORKAREA_VIEW_TAB_ICON_SIZE,
                titlebar_icon_color(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.label.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(mode: TitlebarMode) -> ViewStripTabKey {
        ViewStripTabKey::View(mode)
    }

    fn page(id: u64) -> ViewStripTabKey {
        ViewStripTabKey::Browser(BrowserTabId(id))
    }

    /// The 2026-09-21 loop both strips used before they shared one rule.
    fn loop_order(
        layout: &GpuiViewStripLayout,
        available: &[ViewStripTabKey],
    ) -> Vec<ViewStripTabKey> {
        let mut remaining = available.to_vec();
        let mut ordered = Vec::new();
        for stored in &layout.order {
            if let Some(index) = remaining.iter().position(|key| key == stored) {
                ordered.push(remaining.remove(index));
            }
        }
        ordered.append(&mut remaining);
        let (pinned, unpinned): (Vec<_>, Vec<_>) = ordered
            .into_iter()
            .partition(|key| layout.pinned.contains(key));
        pinned.into_iter().chain(unpinned).collect()
    }

    #[test]
    fn the_scan_and_the_loop_draw_the_same_order() {
        for count in [4_u64, 32, 33, 40, 64] {
            let mut order: Vec<_> = (0..count).map(|id| page((id * 7) % count)).collect();
            // A stored key with no tab, and a tab with no stored place.
            order.push(page(10_000));
            let layout = GpuiViewStripLayout {
                order,
                pinned: vec![page(3), page(1)],
                active_side_chat: None,
            };
            let mut available: Vec<_> = (0..count).rev().map(page).collect();
            available.push(page(20_000));
            assert_eq!(
                layout.in_drawn_order(available.clone(), false, |key| *key),
                loop_order(&layout, &available),
                "{count} tabs"
            );
        }
    }

    #[test]
    fn an_unplaced_browser_tab_stands_where_its_first_page_stood() {
        let layout = GpuiViewStripLayout {
            order: vec![
                view(TitlebarMode::Kanban),
                page(1),
                view(TitlebarMode::Manage),
                page(2),
            ],
            pinned: Vec::new(),
            active_side_chat: None,
        };
        let available = vec![
            view(TitlebarMode::Manage),
            view(TitlebarMode::Browser),
            view(TitlebarMode::Kanban),
        ];
        assert_eq!(
            layout.in_drawn_order(available.clone(), true, |key| *key),
            vec![
                view(TitlebarMode::Kanban),
                view(TitlebarMode::Browser),
                view(TitlebarMode::Manage)
            ]
        );
        // Without its pages in a row of their own the Browser tab has no stand-in place.
        assert_eq!(
            layout.in_drawn_order(available, false, |key| *key),
            vec![
                view(TitlebarMode::Kanban),
                view(TitlebarMode::Manage),
                view(TitlebarMode::Browser)
            ]
        );
    }

    /// The pin and unpin from the bug report: only a drag rewrote the open-views list, so it still
    /// had Kanban beside Automate while the strip drew Files there.
    #[test]
    fn open_views_follow_the_strip_after_a_pin_and_an_unpin() {
        let layout = GpuiViewStripLayout {
            order: vec![
                view(TitlebarMode::Kanban),
                view(TitlebarMode::Manage),
                view(TitlebarMode::Automate),
            ],
            pinned: Vec::new(),
            active_side_chat: None,
        };
        let open_views = [
            TitlebarMode::Manage,
            TitlebarMode::Kanban,
            TitlebarMode::Automate,
        ];
        assert_eq!(
            layout.open_views_in_drawn_order(&open_views, &[], false),
            vec![
                TitlebarMode::Kanban,
                TitlebarMode::Manage,
                TitlebarMode::Automate
            ]
        );
    }

    #[test]
    fn browser_stands_where_its_pages_are_drawn() {
        let layout = GpuiViewStripLayout {
            order: vec![
                view(TitlebarMode::Manage),
                page(1),
                view(TitlebarMode::Automate),
                page(2),
            ],
            pinned: Vec::new(),
            active_side_chat: None,
        };
        let open_views = [
            TitlebarMode::Browser,
            TitlebarMode::Manage,
            TitlebarMode::Automate,
        ];
        let pages = [BrowserTabId(2), BrowserTabId(1)];
        assert_eq!(
            layout.open_views_in_drawn_order(&open_views, &pages, false),
            vec![
                TitlebarMode::Manage,
                TitlebarMode::Browser,
                TitlebarMode::Automate
            ]
        );
        // No page drawn: Browser keeps its own slot.
        assert_eq!(
            layout.open_views_in_drawn_order(&open_views, &[], false),
            vec![
                TitlebarMode::Browser,
                TitlebarMode::Manage,
                TitlebarMode::Automate
            ]
        );
        // Its pages in their own row: the strip's Browser tab stands at its first page's place.
        assert_eq!(
            layout.open_views_in_drawn_order(&open_views, &pages, true),
            vec![
                TitlebarMode::Manage,
                TitlebarMode::Browser,
                TitlebarMode::Automate
            ]
        );
    }

    #[test]
    fn a_projection_moves_only_the_drawn_tabs() {
        let mut layout = GpuiViewStripLayout {
            order: vec![
                view(TitlebarMode::Manage),
                view(TitlebarMode::Automate),
                page(9),
                view(TitlebarMode::Kanban),
            ],
            pinned: Vec::new(),
            active_side_chat: None,
        };
        layout.write_projection(
            &[
                view(TitlebarMode::Kanban),
                view(TitlebarMode::Manage),
                page(4),
            ],
            |key| *key != page(9),
        );
        assert_eq!(
            layout.order,
            vec![
                view(TitlebarMode::Kanban),
                view(TitlebarMode::Automate),
                view(TitlebarMode::Manage),
                page(4)
            ]
        );
    }
}

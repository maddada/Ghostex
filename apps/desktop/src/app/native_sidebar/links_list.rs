//! A session's links list: every PR, GitHub issue and Linear ticket the session is linked to, on
//! two lines each, with the chat's suggestions below. The header's "+N" dropdown and the floating
//! card a hovered sidebar row opens both draw it (the titlebar popup plumbing hosts the dropdown,
//! window/session_links_card.rs the card; the web build draws the card in its own page).
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "i also want the user to be able to click on an x icon button (twice to confirm) to hide the link". The first click on an entry's × arms it for three seconds (it turns red, CDXC:SessionLinks 2026-11-04 at `render_remove_button`); a second click removes the link, and it never joins the session again by itself (server/src/session_links/store.rs).
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: a PR or issue URL the chat only mentioned is offered under Suggested ("suggest") with a Link button, rather than linked by itself.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "when a link is gone i see that i can't bring it back why not?", "i mean when i hit x button twice on it". A collapsed "Removed (N)" group at the bottom lists every removed link with Restore (`/api/restoreSessionLink`). The "Removed · Undo" row that stood in for a just-removed link is gone (CDXC:SessionLinks 2026-10-27 in packages/gx-core/src/sidebar_view/links.rs, `without_removed`): the link leaves its group at once and leads Removed.
//! CDXC:SessionLinks 2026-10-27 DECISION:
//! User: "also the height of the row that has "undo" is bigger than the other rows, just wanted to note this is wrong". Every row is the height of a plain link row (one line or two); a button in it (pin, ×, Link, Restore) is sized to fit the row (`BUTTON_HEIGHT`) and never grows it.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "can u pls make all the sections in these cards colapsible with chevron? please add this for pr and github issues etc (the headers there". Every group header has the same chevron and folds on a click, keeping its count. The folded groups are one app-wide choice per group (`sessionLinksFoldedGroups`), kept across sessions and restarts in the settings file the way the chat keeps its zoom; the phone keeps its own (SessionLinksSheet.tsx).
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User picked "B. Open work first, finished work folded" for the links card's height: the list leads with an "Open N" group of the links that still need something, merged and closed links sit in one group folded until opened, named "Merged / Closed N" (the user's "also please make it "Merged / Closed" like that"), then Suggested and Removed (folded) as before. This replaces the Pull requests / GitHub issues / Linear sections, in the card, the header's "+N" dropdown and the phone's sheet alike. With it came "Fit what's shown": the list measures what is actually shown (a folded group is its header line) and its window resizes when a group folds or opens. And "Half the window's height": past half the window's height the list scrolls inside the card under its fixed header.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "make suggested and removed also have the same look as other ones (2 rows)". Suggested and Removed entries use the same two-line row as linked ones (number, title and state, then "Mentioned in the chat" or where a removed link came from, server/src/session_links/presentation.rs), with Link or Restore where the × sits and no dashed box.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "for the linked issues/prs/linear i want to be able to pin a few so they appear on top under "pinned" and they always appear in the sidebar. this is so i can pin the main pr/issue for each thread". Each linked entry has a pin button beside its × (`/api/pinSessionLink`), and a Pinned group leads the list above Open, pinned issues and Linear tickets before pinned PRs (gxserver's order, server/src/session_links/presentation.rs).
//! CDXC:SessionLinks 2026-10-24 WHY:
//! The list opens from a click or a hover the app is still handling, and opening a window draws it at once, so a list that read the app while drawing crashed Ghostex ("cannot read GhostexGpuiApp while it is already being updated"). The opener hands the list in, and later changes arrive through the observer, which runs after the app's update.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollHandle, StatefulInteractiveElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::{ElementExt as _, h_flex, v_flex};
use serde_json::json;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use super::link_chips::{
    open_session_link_command, session_link_glyph_icon, session_link_tone_color,
    sidebar_style_tooltip,
};
use super::tooltips::{SidebarTooltipSpan, sidebar_tooltip, tooltip_with_gap};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use ghostex_gx_core::{
    SUGGESTED_SHOWN_FIRST, SessionKey, SessionLink, SessionLinkTone, SessionLinks,
};

/// How long a × waits for its confirming click.
const REMOVE_ARMED_FOR: Duration = Duration::from_secs(3);
/// How long a removed link stays out of the list without gxserver publishing its removal; past it
/// the list shows what gxserver says again (the removal failed, and said so in a toast).
const REMOVAL_PENDING_FOR: Duration = Duration::from_secs(10);
pub(crate) const SESSION_LINKS_LIST_WIDTH: f32 = 440.0;
/// The floating card beside the sidebar is narrower than the header's dropdown.
pub(crate) const SESSION_LINKS_CARD_WIDTH: f32 = 380.0;
const HEADER_HEIGHT: f32 = 38.0;
const SECTION_HEIGHT: f32 = 24.0;
const ROW_HEIGHT: f32 = 46.0;
/// A row with nothing on its second line.
const ONE_LINE_ROW_HEIGHT: f32 = 30.0;
/// Every button a row carries; it sits 6px under the row's top, so it fits a one-line row.
const BUTTON_HEIGHT: f32 = 20.0;
const ROW_TOP_PADDING: f32 = 6.0;
const _: () = assert!(ROW_TOP_PADDING + BUTTON_HEIGHT <= ONE_LINE_ROW_HEIGHT);
/// "This session has no links."
const EMPTY_HEIGHT: f32 = 40.0;

/// Where a list is shown, which decides what closing it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionLinksListHost {
    /// The header's "+N" dropdown.
    Dropdown,
    /// The card a hovered sidebar row opens.
    Card,
}

/// The list's height for what it shows: a folded group is its header line, an open one its rows
/// too. The host caps it (half the window for the card).
pub(crate) fn session_links_list_height(links: Option<&SessionLinks>, folded: &[String]) -> f32 {
    session_links_list_height_showing(links, folded, false)
}

/// `session_links_list_height`, with every suggestion shown (`all_suggested`) or the first
/// `SUGGESTED_SHOWN_FIRST` and the "Show all" row.
fn session_links_list_height_showing(
    links: Option<&SessionLinks>,
    folded: &[String],
    all_suggested: bool,
) -> f32 {
    let Some(links) = links else {
        return HEADER_HEIGHT + 60.0;
    };
    let open = |group: &str| !folded.iter().any(|folded| folded == group);
    // The body's children, which sit 2px apart.
    let mut children: Vec<f32> = Vec::new();
    for (group, _) in SESSION_LINK_GROUPS {
        let rows: Vec<f32> = links
            .items
            .iter()
            .filter(|item| link_in_group(item, group))
            .map(entry_height)
            .collect();
        if rows.is_empty() {
            continue;
        }
        children.push(SECTION_HEIGHT);
        if open(group) {
            children.extend(rows);
        }
    }
    if !links.suggested.is_empty() {
        children.push(SECTION_HEIGHT);
        if open("suggested") {
            let shown = suggestions_shown(links.suggested.len(), all_suggested);
            children.extend(
                links.suggested[..shown]
                    .iter()
                    .map(|suggestion| entry_height(&suggestion.as_link())),
            );
            if links.suggested.len() > SUGGESTED_SHOWN_FIRST {
                children.push(SECTION_HEIGHT);
            }
        }
    }
    let removed: Vec<f32> = links
        .removed
        .iter()
        .map(|link| entry_height(&link.as_link()))
        .collect();
    if !removed.is_empty() {
        children.push(if open("removed") {
            SECTION_HEIGHT + removed.iter().map(|row| row + 2.0).sum::<f32>()
        } else {
            SECTION_HEIGHT
        });
    }
    if links.items.is_empty() && links.suggested.is_empty() {
        children.push(EMPTY_HEIGHT);
    }
    let gaps = children.len().saturating_sub(1) as f32 * 2.0;
    // The header, the scroll area's 6px padding top and bottom, and the 1px border.
    HEADER_HEIGHT + 12.0 + 2.0 + children.iter().sum::<f32>() + gaps
}

/// How many of `count` suggestions the list draws.
fn suggestions_shown(count: usize, all_suggested: bool) -> usize {
    if all_suggested {
        count
    } else {
        count.min(SUGGESTED_SHOWN_FIRST)
    }
}

/// The clear space a row's card needs so it opens beyond the row rather than over its buttons:
/// `(top, bottom)` padding for the card, one of them the distance from the pointer to the row's
/// near edge and the other zero. The card opens below the row when the window has room for it
/// there, else above.
///
/// CDXC:SessionLinks 2026-10-24 WHY:
/// A GPUI tooltip opens at the pointer, which on a row sits level with the row's own pin and ×, so the card started over them. The gap moves the card's start past the row's edge; the room is estimated from the text because the card's size is only known once it is laid out.
fn row_card_gaps(row: Bounds<Pixels>, text: &str, window: &Window) -> (f32, f32) {
    let pointer = f32::from(window.mouse_position().y);
    let (top, bottom) = (f32::from(row.top()), f32::from(row.bottom()));
    // About 40 characters to a line of the 340px card at 12.5px, 17px a line, its padding.
    let estimated: f32 = text
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                (line.chars().count() as f32 / 40.0).ceil().max(1.0) * 17.0
            } else {
                15.0
            }
        })
        .sum::<f32>()
        + 24.0;
    let window_height = f32::from(window.viewport_size().height);
    if bottom + estimated + 6.0 <= window_height || top < estimated + 6.0 {
        ((bottom - pointer + 2.0).max(0.0), 0.0)
    } else {
        (0.0, (pointer - top + 2.0).max(0.0))
    }
}

/// The tallest the card or the "+N" dropdown grows: half the window, past which the list scrolls
/// under its fixed header.
pub(crate) fn session_links_max_height(window_height: f32) -> f32 {
    (window_height / 2.0).max(80.0)
}

/// An entry's row: two lines, or one when its second line has nothing to say.
fn entry_height(link: &SessionLink) -> f32 {
    if link.detail_line().is_empty() {
        ONE_LINE_ROW_HEIGHT
    } else {
        ROW_HEIGHT
    }
}

/// The link groups in list order: id and title. Pinned leads (CDXC:SessionLinks 2026-10-24 in
/// server/src/session_links/store.rs); Open, Loading and Merged / Closed hold the rest. A link
/// never read yet waits under Loading, so Open only lists what is known to be open and agrees
/// with the header's "N open" (server/src/session_links/presentation.rs `entry_value`).
const SESSION_LINK_GROUPS: [(&str, &str); 4] = [
    ("pinned", "Pinned"),
    ("open", "Open"),
    ("loading", "Loading"),
    ("finished", "Merged / Closed"),
];

/// Whether a link belongs to the group `group` of `SESSION_LINK_GROUPS`.
fn link_in_group(link: &SessionLink, group: &str) -> bool {
    match group {
        "pinned" => link.pinned,
        "loading" => !link.pinned && link.loading,
        "open" => !link.pinned && !link.loading && !link.finished,
        _ => !link.pinned && !link.loading && link.finished,
    }
}

pub(crate) struct SessionLinksList {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    key: SessionKey,
    host: SessionLinksListHost,
    /// The entry whose × was clicked once, waiting for the second click.
    armed: Option<String>,
    /// The entry under the pointer: only it shows its pin and ×.
    hovered: Option<String>,
    /// Links removed here whose removal gxserver has not published yet: they are already drawn
    /// under Removed (gx-core `SessionLinks::without_removed`).
    removing: Vec<String>,
    /// The groups folded shut, by id (`open`, `finished`, `suggested`, `removed`), as
    /// `sessionLinksFoldedGroups` says.
    folded: Vec<String>,
    /// Whether Suggested shows unfolded for this open of the list, over the saved fold: set when
    /// the list opens on 2 or fewer linked links (gx-core `SUGGESTED_OPEN_UP_TO_LINKED`) and by
    /// the person's own fold or unfold, until the list closes.
    suggested_open: Option<bool>,
    /// "Show all" was clicked under Suggested: every suggestion shows until "Show fewer".
    all_suggested: bool,
    /// The tallest the list's own window may grow; past it the list scrolls.
    max_height: f32,
    /// The height the list last gave its window.
    fitted_height: Option<f32>,
    scroll: ScrollHandle,
    /// The user's Tooltip Delay, which the rows' cards wait as the sidebar's tooltips do. Read
    /// from the app by the observer, never while drawing.
    tooltip_delay: Duration,
    /// The list this draws, kept here rather than read from the app while drawing.
    links: Option<SessionLinks>,
    _observe: Option<gpui::Subscription>,
}

impl SessionLinksList {
    /// `links` is the list as the app has it when the list opens.
    pub(crate) fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        key: SessionKey,
        host: SessionLinksListHost,
        links: Option<SessionLinks>,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        // The list follows the session while it is open: a link the background pass finds, a
        // check that turns red, a removal.
        let observe = main_app.upgrade().map(|app| {
            cx.observe(&app, |list: &mut Self, app, cx| {
                let links = app.read(cx).session_links_of(&list.key);
                let folded = folded_session_link_groups();
                let delay = app.read(cx).configured_tooltip_delay();
                // A removal gxserver has published needs no holding any more.
                if let Some(links) = &links {
                    list.removing.retain(|key| {
                        links.items.iter().any(|link| &link.key == key)
                            || links.suggested.iter().any(|link| &link.key == key)
                    });
                }
                if list.suggested_open.is_none() && list.links.is_none() {
                    list.suggested_open = suggested_open_on_open(links.as_ref());
                }
                if links != list.links || folded != list.folded || delay != list.tooltip_delay {
                    list.links = links;
                    list.folded = folded;
                    list.tooltip_delay = delay;
                    cx.notify();
                }
            })
        });
        Self {
            main_app,
            key,
            host,
            armed: None,
            hovered: None,
            removing: Vec::new(),
            folded: folded_session_link_groups(),
            suggested_open: suggested_open_on_open(links.as_ref()),
            all_suggested: false,
            max_height,
            fitted_height: None,
            scroll: ScrollHandle::new(),
            tooltip_delay: Duration::from_millis(500),
            links,
            _observe: observe,
        }
    }

    fn close(&self, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.host;
        let _ = self.main_app.update(cx, |app, cx| {
            app.close_session_links_host(host, window, cx);
        });
    }

    fn send(&self, command: serde_json::Value, cx: &mut Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.dispatch_native_sidebar_ui(command, cx);
        });
    }

    fn click_remove(&mut self, key: String, cx: &mut Context<Self>) {
        if self.armed.as_deref() == Some(&key) {
            self.armed = None;
            self.hovered = None;
            let sidebar_id = self.key.to_sidebar_session_id();
            if !self.removing.contains(&key) {
                self.removing.push(key.clone());
            }
            self.send(
                json!({ "type": "removeSessionLink", "sessionId": sidebar_id, "key": key }),
                cx,
            );
            cx.spawn(async move |list, cx| {
                cx.background_executor().timer(REMOVAL_PENDING_FOR).await;
                let _ = list.update(cx, |list, cx| {
                    let before = list.removing.len();
                    list.removing.retain(|pending| pending != &key);
                    if list.removing.len() != before {
                        cx.notify();
                    }
                });
            })
            .detach();
        } else {
            self.armed = Some(key.clone());
            cx.spawn(async move |list, cx| {
                cx.background_executor().timer(REMOVE_ARMED_FOR).await;
                let _ = list.update(cx, |list, cx| {
                    if list.armed.as_deref() == Some(&key) {
                        list.armed = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// A linked entry: its row with its pin and the × that removes it.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "please don't show the x or pin icon unless i hover on that link", "please don't use this pin icon, there's another nicer one tat i like", "if we pin a thread then keep the pin icon shown in that case and make it filled". The pin and × show only while the pointer is on the row (or while its × waits for the second click); a pinned link always shows its pin, filled. The pin is the outline and filled pair the sidebar's account usage button uses (`pin-outline.svg`, `pin-filled.svg`).
    fn render_row(&self, link: &SessionLink, cx: &mut Context<Self>) -> AnyElement {
        let hovered = self.hovered.as_deref() == Some(link.key.as_str());
        let armed = self.armed.as_deref() == Some(link.key.as_str());
        let sidebar_id = self.key.to_sidebar_session_id();
        let key = link.key.clone();
        let pin = !link.pinned;
        let pin_button = Self::icon_button(
            format!("session-link-pin-{}", link.key),
            if link.pinned {
                "titlebar/pin-filled.svg"
            } else {
                "titlebar/pin-outline.svg"
            },
            if link.pinned {
                "Unpin"
            } else {
                "Pin to the top"
            },
            move |list, _, _, cx| {
                list.send(
                    json!({
                        "type": "pinSessionLink",
                        "sessionId": sidebar_id,
                        "key": key,
                        "pinned": pin,
                    }),
                    cx,
                );
            },
            cx,
        );
        // CDXC:SessionLinks 2026-10-24 DECISION: User: "please don't show x to remove link that's pinned". A pinned link has no ×; unpin it first to remove it.
        let remove =
            (!link.pinned && (hovered || armed)).then(|| self.render_remove_button(link, cx));
        let actions = h_flex()
            .flex_shrink_0()
            .gap(px(2.0))
            .when(hovered || armed || link.pinned, |actions| {
                actions.child(pin_button)
            })
            .children(remove)
            .into_any_element();
        self.render_entry(link, actions, cx)
    }

    /// Every entry's row (a linked link, a suggestion, a removed link): icon, number, title and
    /// state, then its second line when it has one, with its own action on the right.
    fn render_entry(
        &self,
        link: &SessionLink,
        action: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (glyph, tone) = link.glyph();
        let tone_color = session_link_tone_color(tone);
        let red = session_link_tone_color(SessionLinkTone::Closed);
        let open = open_session_link_command(link, &self.key.to_sidebar_session_id());
        let loading = link.loading && link.title.is_none();
        let title = if loading {
            "Loading…".to_string()
        } else {
            link.title.clone().unwrap_or_default()
        };
        let detail = link.detail_line();
        let failing = link.checks.as_deref() == Some("failing");
        let row_key = link.key.clone();
        let card_text = link.hover_card_text();
        let row_bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let row_bounds_writer = row_bounds.clone();
        h_flex()
            .id(gpui::SharedString::from(format!(
                "session-link-{}",
                link.key
            )))
            .on_prepaint(move |bounds, _, _| row_bounds_writer.set(bounds))
            .tooltip_show_delay(self.tooltip_delay)
            // The row's card, under the buttons' and the icon's own tooltips, which win where the
            // pointer is on them.
            .tooltip(move |window, cx| {
                // The sidebar's session card, spanning the row: the same bubble, look and fonts.
                let row = row_bounds.get();
                let span = SidebarTooltipSpan {
                    left: f32::from(row.left()),
                    right: f32::from(row.right()),
                };
                let card = sidebar_tooltip(card_text.clone(), span, 1.0, window, cx);
                if window.tooltip_presenter_active() {
                    // Drawn in the tooltip host window, right under the row.
                    crate::app::window::frosted_host::anchor_tooltip_below(card.entity_id(), row);
                    card
                } else {
                    // Drawn in this window at the pointer (Linux): opened beyond the row's edge.
                    let (top_gap, bottom_gap) = row_card_gaps(row, &card_text, window);
                    tooltip_with_gap(card, top_gap, bottom_gap, cx)
                }
            })
            .on_hover(cx.listener(move |list, hovered: &bool, _, cx| {
                if *hovered {
                    list.hovered = Some(row_key.clone());
                } else if list.hovered.as_deref() == Some(row_key.as_str()) {
                    list.hovered = None;
                }
                // Moving off a row lets go of its armed ×.
                if list.armed.is_some() && list.armed != list.hovered {
                    list.armed = None;
                }
                cx.notify();
            }))
            .h(px(entry_height(link)))
            .w_full()
            .flex_shrink_0()
            .items_start()
            .gap(px(8.0))
            .px(px(8.0))
            .pt(px(ROW_TOP_PADDING))
            .rounded(px(6.0))
            .when(link.finished, |row| row.opacity(0.62))
            .hover(|row| row.bg(titlebar_popup_menu_hover_color()))
            // The state is the icon's colour; hovering the icon says it (gx-core `state_tooltip`).
            .child({
                let tooltip = link.state_tooltip();
                div()
                    .id(gpui::SharedString::from(format!(
                        "session-link-state-{}",
                        link.key
                    )))
                    .pt(px(2.0))
                    .child(titlebar_svg_icon(
                        session_link_glyph_icon(glyph),
                        14.0,
                        tone_color,
                    ))
                    .when(!tooltip.is_empty(), |icon| {
                        icon.tooltip(move |window, cx| {
                            sidebar_style_tooltip(tooltip.clone(), window, cx)
                        })
                    })
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.0))
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap(px(6.0))
                            .text_size(px(12.5))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(chrome_ink().opacity(0.96))
                                    .child(link.label.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(chrome_ink().opacity(if loading {
                                        0.45
                                    } else {
                                        0.86
                                    }))
                                    .child(title),
                            ),
                    )
                    .when(!detail.is_empty(), |lines| {
                        lines.child(
                            div()
                                .truncate()
                                .text_size(px(11.5))
                                .text_color(if failing {
                                    red
                                } else {
                                    chrome_ink().opacity(0.55).into()
                                })
                                .child(detail),
                        )
                    }),
            )
            .child(action)
            .when_some(open, |row, open| {
                row.cursor_pointer()
                    .on_click(cx.listener(move |list, _, window, cx| {
                        list.send(open.clone(), cx);
                        list.close(window, cx);
                    }))
            })
            .into_any_element()
    }

    /// A linked entry's ×: a first click arms it, turning the × a soft red, a second removes the
    /// link.
    ///
    /// CDXC:SessionLinks 2026-11-04 DECISION:
    /// User: "when i click on the x button for links please don't show 'Remove?' instead make the x button turn redish to indicate i need to click again to confirm pls". The armed × keeps its size and place and turns red on a faint red fill; it lets go after three seconds or when the pointer leaves its row. This replaces the red "Remove?" word of the 2026-10-24 decision above.
    fn render_remove_button(&self, link: &SessionLink, cx: &mut Context<Self>) -> AnyElement {
        let red = session_link_tone_color(SessionLinkTone::Closed);
        let armed = self.armed.as_deref() == Some(link.key.as_str());
        let key = link.key.clone();
        h_flex()
            .id(gpui::SharedString::from(format!(
                "session-link-remove-{}",
                link.key
            )))
            .flex_shrink_0()
            .h(px(BUTTON_HEIGHT))
            .px(px(4.0))
            .items_center()
            .rounded(px(5.0))
            .cursor_pointer()
            .role(gpui::Role::Button)
            .aria_label(if armed {
                "Click again to remove the link"
            } else {
                "Remove from this session"
            })
            .when(armed, |button| button.bg(red.opacity(0.16)))
            .when(!armed, |button| {
                button.hover(|button| button.bg(chrome_ink().opacity(0.12)))
            })
            .child(titlebar_svg_icon(
                "titlebar/x.svg",
                12.0,
                if armed {
                    red
                } else {
                    chrome_ink().opacity(0.6).into()
                },
            ))
            .tooltip(move |window, cx| {
                sidebar_style_tooltip(
                    if armed {
                        "Click again to remove the link"
                    } else {
                        "Remove from this session"
                    }
                    .to_string(),
                    window,
                    cx,
                )
            })
            .on_click(cx.listener(move |list, _, _, cx| {
                cx.stop_propagation();
                list.click_remove(key.clone(), cx);
            }))
            .into_any_element()
    }

    /// A suggestion's Link or a removed link's Restore: a small icon button where a linked entry's
    /// × sits, named by its tooltip.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "for suggested i want the row to look just like the regular rows we have there". Suggested and Removed rows are drawn like linked ones; only their action differs, an icon button (link "Link", arrow-back "Restore") instead of a bordered text button.
    fn icon_button(
        id: String,
        icon: &'static str,
        tooltip: &'static str,
        on_click: impl Fn(&mut Self, &gpui::ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .id(gpui::SharedString::from(id))
            .flex_shrink_0()
            .h(px(BUTTON_HEIGHT))
            .px(px(4.0))
            .items_center()
            .rounded(px(5.0))
            .cursor_pointer()
            .hover(|button| button.bg(chrome_ink().opacity(0.12)))
            .role(gpui::Role::Button)
            .aria_label(tooltip)
            .child(titlebar_svg_icon(
                icon,
                13.0,
                chrome_ink().opacity(0.7).into(),
            ))
            .tooltip(move |window, cx| sidebar_style_tooltip(tooltip.to_string(), window, cx))
            .on_click(cx.listener(move |list, event, window, cx| {
                cx.stop_propagation();
                on_click(list, event, window, cx);
            }))
            .into_any_element()
    }

    fn restore(&mut self, key: String, cx: &mut Context<Self>) {
        self.removing.retain(|pending| pending != &key);
        let sidebar_id = self.key.to_sidebar_session_id();
        self.send(
            json!({ "type": "restoreSessionLink", "sessionId": sidebar_id, "key": key }),
            cx,
        );
        cx.notify();
    }

    /// The collapsed "Removed (N)" group: its header toggles it, each row offers Restore.
    fn render_removed(&self, links: &SessionLinks, cx: &mut Context<Self>) -> Option<AnyElement> {
        let removed: Vec<&ghostex_gx_core::SuggestedSessionLink> = links.removed.iter().collect();
        if removed.is_empty() {
            return None;
        }
        let open = !self.is_folded("removed");
        let mut group = v_flex().w_full().gap(px(2.0)).child(self.section_header(
            "removed",
            "Removed",
            removed.len(),
            cx,
        ));
        if open {
            for link in removed {
                let key = link.key.clone();
                let restore = Self::icon_button(
                    format!("session-link-restore-{}", link.key),
                    "titlebar/arrow-back-up.svg",
                    "Restore",
                    move |list, _, _, cx| list.restore(key.clone(), cx),
                    cx,
                );
                group = group.child(self.render_entry(&link.as_link(), restore, cx));
            }
        }
        Some(group.into_any_element())
    }

    /// The list's height for what it shows now, capped by `max_height`.
    pub(crate) fn fitted_height(&self) -> f32 {
        session_links_list_height_showing(
            self.shown_links().as_ref(),
            &self.shown_folded(),
            self.all_suggested,
        )
        .min(self.max_height)
        .max(80.0)
    }

    /// The list as drawn: gxserver's, with the links removed here already under Removed.
    fn shown_links(&self) -> Option<SessionLinks> {
        let links = self.links.as_ref()?;
        Some(if self.removing.is_empty() {
            links.clone()
        } else {
            links.without_removed(&self.removing)
        })
    }

    fn is_folded(&self, section: &str) -> bool {
        match (section, self.suggested_open) {
            ("suggested", Some(open)) => !open,
            _ => self.folded.iter().any(|folded| folded == section),
        }
    }

    /// The groups drawn folded: the saved folds, with this open's Suggested over them.
    fn shown_folded(&self) -> Vec<String> {
        let mut folded = self.folded.clone();
        if let Some(open) = self.suggested_open {
            folded.retain(|group| group != "suggested");
            if !open {
                folded.push("suggested".to_string());
            }
        }
        folded
    }

    fn toggle_section(&mut self, section: &'static str, cx: &mut Context<Self>) {
        let fold = !self.is_folded(section);
        if section == "suggested" {
            self.suggested_open = Some(!fold);
        }
        if fold {
            self.folded.push(section.to_string());
        } else {
            self.folded.retain(|folded| folded != section);
            // The last sections sit at the bottom of a list the window does not grow for, so
            // opening one scrolls its rows into view.
            if matches!(section, "suggested" | "removed") {
                self.scroll.scroll_to_bottom();
            }
        }
        self.send(
            json!({ "type": "setSessionLinksFolded", "section": section, "folded": fold }),
            cx,
        );
        cx.notify();
    }

    /// A section's header: the chevron, its title and its count, folding the section on a click.
    fn section_header(
        &self,
        section: &'static str,
        title: &str,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let folded = self.is_folded(section);
        h_flex()
            .id(gpui::SharedString::from(format!(
                "session-links-section-{section}"
            )))
            .h(px(SECTION_HEIGHT))
            .px(px(8.0))
            .items_end()
            .pb(px(4.0))
            .text_size(px(11.0))
            .text_color(chrome_ink().opacity(0.55))
            .cursor_pointer()
            .hover(|header| header.text_color(chrome_ink().opacity(0.85)))
            .role(gpui::Role::Button)
            .aria_label(format!("{title} {count}"))
            // The chevron centres on the text's own line: aligned to the bottom of the header it
            // sat on the line box's bottom, below the letters.
            .child(
                h_flex()
                    .gap(px(5.0))
                    .items_center()
                    .line_height(px(14.0))
                    .child(titlebar_svg_icon(
                        if folded {
                            "titlebar/chevron-right.svg"
                        } else {
                            "titlebar/chevron-down.svg"
                        },
                        11.0,
                        chrome_ink().opacity(0.55).into(),
                    ))
                    .child(title.to_string())
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |list, _, _, cx| list.toggle_section(section, cx)))
            .into_any_element()
    }

    /// The row under the first suggestions: "Show all N" shows the rest in place, "Show fewer"
    /// folds them again (CDXC:SessionLinks 2026-10-27 on `SUGGESTED_SHOWN_FIRST`). Drawn like a
    /// section header, which is the list's other line that only shows or hides rows.
    fn render_show_all_suggested(&self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let label = if self.all_suggested {
            "Show fewer".to_string()
        } else {
            format!("Show all {count}")
        };
        h_flex()
            .id("session-links-suggested-show-all")
            .h(px(SECTION_HEIGHT))
            .px(px(8.0))
            .items_center()
            .text_size(px(11.0))
            .text_color(chrome_ink().opacity(0.55))
            .cursor_pointer()
            .hover(|row| row.text_color(chrome_ink().opacity(0.85)))
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .child(label)
            .on_click(cx.listener(|list, _, _, cx| {
                list.all_suggested = !list.all_suggested;
                cx.notify();
            }))
            .into_any_element()
    }

    /// A link the chat only mentioned: its row with Link, which adds it to the session.
    fn render_suggestion(
        &self,
        suggestion: &ghostex_gx_core::SuggestedSessionLink,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // A Linear ticket not read yet has no URL; its ID links it as well.
        let link = suggestion
            .url
            .clone()
            .unwrap_or_else(|| suggestion.label.clone());
        let sidebar_id = self.key.to_sidebar_session_id();
        let add = Self::icon_button(
            format!("session-link-add-{}", suggestion.key),
            "titlebar/link.svg",
            "Link",
            move |list, _, _, cx| {
                list.send(
                    json!({
                        "type": "addSessionLink",
                        "sessionId": sidebar_id,
                        "link": link,
                    }),
                    cx,
                );
            },
            cx,
        );
        // Its × (on hover, twice to confirm, like a linked row's) dismisses it for good: it goes
        // under Removed (CDXC:SessionLinks 2026-10-27 on `dismiss_suggestion` in
        // server/src/session_links/store.rs).
        let link = suggestion.as_link();
        let hovered = self.hovered.as_deref() == Some(link.key.as_str());
        let armed = self.armed.as_deref() == Some(link.key.as_str());
        let actions = h_flex()
            .flex_shrink_0()
            .gap(px(2.0))
            .child(add)
            .when(hovered || armed, |actions| {
                actions.child(self.render_remove_button(&link, cx))
            })
            .into_any_element();
        self.render_entry(&link, actions, cx)
    }
}

impl Render for SessionLinksList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The card and the dropdown are windows of their own that follow what the list shows; the
        // web page sizes its card from `fitted_height` as it draws.
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Its rows' hover cards are often taller than this window, so they draw in the tooltip
            // host window rather than inside it (window/frosted_host.rs).
            crate::app::window::frosted_host::host_window_tooltips_always(window, cx);
            let fitted = self.fitted_height();
            if self.fitted_height != Some(fitted) {
                self.fitted_height = Some(fitted);
                window.resize(gpui::size(window.bounds().size.width, px(fitted)));
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = window;
        let links = self.shown_links().unwrap_or_default();
        // Only links read as open count: one not read yet is neither open nor finished.
        let open = links
            .items
            .iter()
            .filter(|item| !item.finished && item.state.is_some())
            .count();
        let failing = links
            .items
            .iter()
            .filter(|item| item.checks.as_deref() == Some("failing"))
            .count();
        let summary = [
            Some(format!("{open} open")),
            (failing > 0).then(|| format!("{failing} failing checks")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let mut body = v_flex().w_full().gap(px(2.0));
        for (section, title) in SESSION_LINK_GROUPS {
            let items: Vec<&SessionLink> = links
                .items
                .iter()
                .filter(|item| link_in_group(item, section))
                .collect();
            if items.is_empty() {
                continue;
            }
            body = body.child(self.section_header(section, title, items.len(), cx));
            if !self.is_folded(section) {
                for item in items {
                    body = body.child(self.render_row(item, cx));
                }
            }
        }
        if !links.suggested.is_empty() {
            body = body.child(self.section_header(
                "suggested",
                "Suggested",
                links.suggested.len(),
                cx,
            ));
            if !self.is_folded("suggested") {
                let shown = suggestions_shown(links.suggested.len(), self.all_suggested);
                for suggestion in &links.suggested[..shown] {
                    body = body.child(self.render_suggestion(suggestion, cx));
                }
                if links.suggested.len() > SUGGESTED_SHOWN_FIRST {
                    body = body.child(self.render_show_all_suggested(links.suggested.len(), cx));
                }
            }
        }
        if let Some(removed) = self.render_removed(&links, cx) {
            body = body.child(removed);
        }
        if links.items.is_empty() && links.suggested.is_empty() {
            body = body.child(
                div()
                    .p(px(12.0))
                    .text_size(px(12.0))
                    .text_color(chrome_ink().opacity(0.55))
                    .child("This session has no links."),
            );
        }
        // + opens the "Add a link" dialog, which takes the keyboard (this popup never does), and
        // closes the list (CDXC:SessionLinks 2026-10-27 in window/add_session_link_modal.rs).
        let add_link = Self::icon_button(
            "session-links-add".to_string(),
            "titlebar/plus.svg",
            "Add a link",
            |list, _, window, cx| {
                let key = list.key.clone();
                let _ = list.main_app.update(cx, |app, cx| {
                    app.open_gpui_add_session_link_modal(&key, cx);
                });
                list.close(window, cx);
            },
            cx,
        );
        let card = self.host == SessionLinksListHost::Card;
        div()
            .id("session-links-list")
            .role(gpui::Role::Dialog)
            .aria_label("Links")
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(px(8.0))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            // CDXC:SessionLinks 2026-11-06 DECISION:
            // User, of the Links panel a hovered session row opens: "this panel for links is too dark man, please fix it to be like the other panels in the sidebar like the hamburger menu one for example". The card and the dropdown both take the sidebar menus' surface (`popup_menu_surface`), hairline, row hover and header rule; the card's own chrome-coloured fill (the sidebar's raw base, near-black on a dark theme) is gone.
            .bg(popup_menu_surface())
            .text_color(chrome_ink())
            // The card stays while the pointer is on it and closes once it leaves both the card
            // and its row (window/session_links_card.rs).
            .when(card, |list| {
                list.on_hover(cx.listener(|list, hovered: &bool, _, cx| {
                    let hovered = *hovered;
                    let _ = list.main_app.update(cx, |app, cx| {
                        app.set_session_links_card_hovered(hovered, cx);
                    });
                }))
            })
            .child(
                v_flex()
                    .size_full()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .h(px(HEADER_HEIGHT))
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(8.0))
                            .pl(px(12.0))
                            .border_b_1()
                            .border_color(titlebar_popup_menu_border_color())
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(chrome_ink().opacity(0.96))
                            .child("Links")
                            .child(
                                div()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_size(px(12.0))
                                    .text_color(chrome_ink().opacity(0.55))
                                    .child(summary),
                            )
                            .child(div().flex_1())
                            .child(div().pr(px(8.0)).child(add_link)),
                    )
                    .child(
                        div().relative().w_full().min_h_0().flex_1().child(
                            v_flex()
                                .id("session-links-scroll")
                                .size_full()
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll)
                                .p(px(6.0))
                                .child(body),
                        ),
                    ),
            )
    }
}

/// Suggested's fold for a list opening on `links`: unfolded with 2 or fewer linked links (gx-core
/// `SessionLinks::suggested_opens_unfolded`), else the saved fold (`None`).
fn suggested_open_on_open(links: Option<&SessionLinks>) -> Option<bool> {
    links
        .filter(|links| links.suggested_opens_unfolded())
        .map(|_| true)
}

/// The groups folded shut, from `sessionLinksFoldedGroups`; Merged / Closed and Removed start
/// folded.
pub(crate) fn folded_session_link_groups() -> Vec<String> {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    match settings
        .object()
        .get("sessionLinksFoldedGroups")
        .and_then(serde_json::Value::as_array)
    {
        Some(groups) => groups
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect(),
        None => vec!["finished".to_string(), "removed".to_string()],
    }
}

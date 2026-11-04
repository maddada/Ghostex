use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use gpui::{
    App, Bounds, DispatchPhase, Entity, Hitbox, HitboxBehavior, MouseMoveEvent, Pixels, Window,
};

use super::model::NativeSidebarSession;
use crate::GhostexGpuiApp;

/// How long after the last wheel tick the list counts as still scrolling.
const SCROLL_SETTLE: Duration = Duration::from_millis(120);

thread_local! {
    static LAST_WHEEL: Cell<Option<Instant>> = const { Cell::new(None) };
    static SETTLE_PENDING: Cell<bool> = const { Cell::new(false) };
}

fn scroll_in_flight() -> bool {
    LAST_WHEEL
        .get()
        .is_some_and(|at| at.elapsed() < SCROLL_SETTLE)
}

/// The hovered row one session list asks for: take the row under the pointer, or let go of one of its own rows.
enum SessionHoverChange {
    Claim(String),
    Release(String),
}

/// CDXC:Sidebar 2026-09-19 DECISION:
/// The user asked to click a session card's X and, when the next card slides up under the cursor, click again to close it without moving the mouse to bring up that card's hover buttons.
/// Row `on_hover` only fires on mouse moves, so a card that moved under a still pointer (close, reorder, scroll) never learned it was hovered.
/// Each visible row gets a non-interactive hit-test probe, and the list resolves the hovered card from the frame's hit test after every paint and on every mouse move.
pub(super) struct SessionHoverProbes {
    probes: Vec<(String, Hitbox)>,
    sessions: Vec<Arc<NativeSidebarSession>>,
}

impl SessionHoverProbes {
    /// Inserted during prepaint, before the rows' own hitboxes, so the probes sit beneath the rows and block nothing.
    pub(super) fn insert(
        sessions: Vec<Arc<NativeSidebarSession>>,
        rows: impl Iterator<Item = (usize, Bounds<Pixels>)>,
        window: &mut Window,
    ) -> Self {
        let probes = rows
            .map(|(index, bounds)| {
                (
                    sessions[index].session_id.clone(),
                    window.insert_hitbox(bounds, HitboxBehavior::Normal),
                )
            })
            .collect();
        Self { probes, sessions }
    }

    /// Called during paint, once this frame's hit test is known.
    pub(super) fn track(self, view: Entity<GhostexGpuiApp>, window: &mut Window, cx: &mut App) {
        if !scroll_in_flight()
            && let Some(change) = self.change(view.read(cx), window, cx)
        {
            let view = view.clone();
            window.defer(cx, move |_, cx| {
                view.update(cx, |app, cx| app.apply_native_session_hover(change, cx))
            });
        }
        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if let Some(change) = self.change(view.read(cx), window, cx) {
                view.update(cx, |app, cx| app.apply_native_session_hover(change, cx));
            }
        });
    }

    fn change(
        &self,
        app: &GhostexGpuiApp,
        window: &Window,
        cx: &App,
    ) -> Option<SessionHoverChange> {
        // Keyboard input suppresses hit-test hover; keep whatever the pointer last showed.
        if window.last_input_was_keyboard() {
            return None;
        }
        let current = app.native_sidebar.hovered_session.as_deref();
        let under_pointer = if app.native_sidebar.pointer_inside && !cx.has_active_drag() {
            self.probes
                .iter()
                .find(|(_, hitbox)| hitbox.is_hovered(window))
                .map(|(id, _)| id.as_str())
        } else {
            None
        };
        match under_pointer {
            Some(id) => (current != Some(id)).then(|| SessionHoverChange::Claim(id.to_owned())),
            None => current
                .filter(|id| {
                    self.sessions
                        .iter()
                        .any(|session| session.session_id == *id)
                })
                .map(|id| SessionHoverChange::Release(id.to_owned())),
        }
    }
}

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// A mouse wheel tick moves the list by a row or more, so the post-paint hover resolve found a new card under the still pointer after every tick and notified the root again: two full-window frames per tick, which made wheel scrolling stall while a trackpad, which crosses a row every ten frames or so, stayed smooth.
    /// While ticks keep arriving the post-paint resolve is skipped, and one redraw after the last tick resolves the card under the pointer, which keeps the 2026-09-19 decision for a pointer that has stopped. Real pointer movement still resolves at once.
    pub(super) fn native_sidebar_scroll_wheel_moved(&mut self, cx: &mut gpui::Context<Self>) {
        LAST_WHEEL.set(Some(Instant::now()));
        if SETTLE_PENDING.replace(true) {
            return;
        }
        cx.spawn(async move |app, cx| {
            while scroll_in_flight() {
                cx.background_executor().timer(SCROLL_SETTLE).await;
            }
            SETTLE_PENDING.set(false);
            let _ = app.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn apply_native_session_hover(
        &mut self,
        change: SessionHoverChange,
        cx: &mut gpui::Context<Self>,
    ) {
        let hovered = &mut self.native_sidebar.hovered_session;
        match change {
            SessionHoverChange::Claim(id) if hovered.as_deref() != Some(&id) => {
                *hovered = Some(id.clone());
                self.schedule_session_links_card_open(id, cx);
            }
            SessionHoverChange::Release(id) if hovered.as_deref() == Some(&id) => *hovered = None,
            _ => return,
        }
        cx.notify();
    }

    /// Opens the floating links card beside a row the pointer rests on, after the tooltip delay,
    /// so sweeping over the list opens nothing (link_chips.rs, window/session_links_card.rs).
    ///
    /// User: "No links card from the floating sidebar". The floating sidebar never opens it, so the
    /// hover neither starts the timer nor asks for a links refresh; the "+N" press still opens it.
    fn schedule_session_links_card_open(&mut self, id: String, cx: &mut gpui::Context<Self>) {
        if self.floating_sidebar_shown() {
            return;
        }
        let opens_card = self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.groups.iter().any(|group| {
                    group.sessions.iter().any(|session| {
                        session.session_id == id
                            && self.native_session_links_row(session).hover_card
                    })
                })
            });
        if opens_card {
            self.open_session_links_card_after_delay(id, cx);
        }
    }

    /// Opens the links card for a row that has one once the pointer has rested on it for the
    /// tooltip delay; a Unified sidebar card starts it from its own hover (unified/cards.rs).
    pub(super) fn open_session_links_card_after_delay(
        &mut self,
        id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.floating_sidebar_shown() {
            return;
        }
        let delay = self.configured_tooltip_delay();
        cx.spawn(async move |app, cx| {
            cx.background_executor().timer(delay).await;
            let _ = app.update(cx, |app, cx| {
                if app.native_sidebar.hovered_session.as_deref() == Some(&id)
                    && !app.floating_sidebar_shown()
                    && app.native_sidebar.links_card_session.as_deref() != Some(&id)
                {
                    app.open_session_links_card(id, cx);
                }
            });
        })
        .detach();
    }

    /// While the card is open, closes it once the pointer has been off both its row and the card
    /// for two checks in a row (about half a second), so crossing the gap from the row into the
    /// card keeps it open. Called when a card opens.
    ///
    /// CDXC:SessionLinks 2026-10-24 WHY:
    /// A single close timer started when the pointer left the row relied on the card reporting the pointer entering it, and the web build's in-page card stopped receiving that once the sidebar redrew, so it closed under the pointer. The watch asks each time whether the pointer is on the row or the card (the card window's own hover on the desktop, the pointer's position in the page on the web).
    pub(crate) fn watch_session_links_card(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(watched) = self.native_sidebar.links_card_session.clone() else {
            return;
        };
        cx.spawn(async move |app, cx| {
            let mut misses = 0;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let Ok(keep) = app.update(cx, |app, cx| {
                    if app.native_sidebar.links_card_session.as_deref() != Some(&watched) {
                        return None;
                    }
                    let state = &app.native_sidebar;
                    Some(
                        state.hovered_session.as_deref() == Some(&watched)
                            || state.links_card_hovered
                            || app.session_links_card_holds_pointer(cx),
                    )
                }) else {
                    return;
                };
                match keep {
                    None => return,
                    Some(true) => misses = 0,
                    Some(false) => {
                        misses += 1;
                        if misses >= 2 {
                            let _ = app.update(cx, |app, cx| app.close_session_links_card(cx));
                            return;
                        }
                    }
                }
            }
        })
        .detach();
    }

    /// The pointer entered or left the card (native_sidebar/links_list.rs).
    pub(crate) fn set_session_links_card_hovered(
        &mut self,
        hovered: bool,
        _cx: &mut gpui::Context<Self>,
    ) {
        self.native_sidebar.links_card_hovered = hovered;
    }
}

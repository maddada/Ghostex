//! The sidebar's ask to star Ghostex on GitHub, and after a star, its one ask to post about it on X:
//! a small card docked at the bottom of the sidebar, above the workspace line and the usage meters.
//!
//! CDXC:Growth 2026-11-14 DECISION:
//! User, on the star request page (Q3091 to Q3094): the sidebar card, the "free and open source"
//! wording, asked after a number of sent prompts ("adjust values a bit … use b as gist of it"), and
//! "once they click on that then remove it because consider them starred pls. and have dismiss
//! button". Star opens the repository "on their own browser not on embedded browser (maybe not
//! logged in)", so both links go through the system's opener, never Ghostex's browser. "make it
//! also tweet about it … in another one if someone already gave us the github star, but this needs
//! to appear another day and just once": a second card, only after a star, on a later day than
//! the star, shown until it is used or dismissed, then never again.
//!
//! The state is per computer: one small file in Ghostex's state folder. A computer that cannot
//! write that file could never keep a dismissal, so it never asks (the web build has no file
//! system and never asks).

use super::appearance::SidebarAppearance;
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::{
    AnyElement, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub(crate) const GHOSTEX_REPOSITORY_URL: &str = "https://github.com/maddada/Ghostex";
/// Sent prompts before the star card can show.
const PROMPTS_BEFORE_ASKING: u32 = 8;
/// How long after the latest sent prompt the star card waits, so it never lands under the
/// pointer during a send.
const QUIET_AFTER_SEND: Duration = Duration::from_secs(20);
/// What the X compose box starts with. The reader finishes the sentence.
const POST_TEXT: &str = "I've been using Ghostex, a free, open source workspace for coding agents. What I like about it:";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct StarRequestState {
    pub(crate) prompts_sent: u32,
    /// The local day (days since 1970-01-01) Star on GitHub was pressed.
    pub(crate) starred_on_day: Option<i64>,
    /// Dismiss on the star card: neither card shows again.
    pub(crate) star_dismissed: bool,
    /// Post on X or Dismiss on the second card: it never shows again.
    pub(crate) post_done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StarRequestCard {
    Star,
    Post,
}

impl StarRequestState {
    /// The card to show today, if any. `quiet` is whether the latest send was long enough ago.
    pub(crate) fn card(&self, today: i64, quiet: bool) -> Option<StarRequestCard> {
        if let Some(starred_on) = self.starred_on_day {
            return (today > starred_on && !self.post_done).then_some(StarRequestCard::Post);
        }
        (!self.star_dismissed && self.prompts_sent >= PROMPTS_BEFORE_ASKING && quiet)
            .then_some(StarRequestCard::Star)
    }
}

struct Loaded {
    state: StarRequestState,
    last_send: Option<std::time::Instant>,
}

/// `None` until first read; `Some(None)` when this computer cannot keep the state.
static STATE: Mutex<Option<Option<Loaded>>> = Mutex::new(None);
/// A redraw is already waiting for the quiet time to pass.
static WAKE_PENDING: AtomicBool = AtomicBool::new(false);

#[cfg(not(ghostex_web))]
fn state_path() -> Option<std::path::PathBuf> {
    Some(
        crate::shared_settings::ghostex_storage_paths()
            .state_dir
            .join("github-star-request.json"),
    )
}

#[cfg(ghostex_web)]
fn state_path() -> Option<std::path::PathBuf> {
    None
}

fn read_state() -> Option<Loaded> {
    let path = state_path()?;
    let state = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => StarRequestState::default(),
        Err(error) => {
            log::warn!("Could not read the star request state: {error}");
            return None;
        }
    };
    Some(Loaded {
        state,
        last_send: None,
    })
}

/// Saves the state; false when it could not be written, and then this computer never asks again
/// this run.
fn write_state(state: &StarRequestState) -> bool {
    let Some(path) = state_path() else {
        return false;
    };
    let written = serde_json::to_vec_pretty(state)
        .map_err(std::io::Error::other)
        .and_then(|bytes| {
            path.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, bytes))
        });
    if let Err(error) = written {
        log::warn!("Could not save the star request state: {error}");
        return false;
    }
    true
}

/// Runs `change` on the loaded state and saves it when it says so.
fn with_state<R>(change: impl FnOnce(&mut Loaded) -> (R, bool)) -> Option<R> {
    let mut guard = STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let slot = guard.get_or_insert_with(read_state);
    let loaded = slot.as_mut()?;
    let (result, save) = change(loaded);
    if save && !write_state(&loaded.state) {
        *slot = None;
    }
    Some(result)
}

fn today() -> i64 {
    use chrono::Datelike as _;
    i64::from(chrono::Local::now().date_naive().num_days_from_ce())
}

/// Counts a prompt sent from a chat's composer.
pub(crate) fn note_prompt_sent() {
    with_state(|loaded| {
        // Nothing left to count once the star card has been answered.
        if loaded.state.starred_on_day.is_some() || loaded.state.star_dismissed {
            return ((), false);
        }
        loaded.state.prompts_sent = loaded.state.prompts_sent.saturating_add(1);
        loaded.last_send = Some(std::time::Instant::now());
        ((), true)
    });
}

/// The card to draw now, and how long until the star card's quiet time ends when it is the only
/// thing holding it back.
fn current_card() -> (Option<StarRequestCard>, Option<Duration>) {
    with_state(|loaded| {
        let since = loaded.last_send.map(|sent| sent.elapsed());
        let wait = since
            .filter(|since| *since < QUIET_AFTER_SEND)
            .map(|since| QUIET_AFTER_SEND - since);
        let card = loaded.state.card(today(), wait.is_none());
        let wait = wait.filter(|_| card.is_none() && loaded.state.card(today(), true).is_some());
        ((card, wait), false)
    })
    .unwrap_or((None, None))
}

fn answer(card: StarRequestCard, used: bool) {
    with_state(|loaded| {
        match (card, used) {
            (StarRequestCard::Star, true) => loaded.state.starred_on_day = Some(today()),
            (StarRequestCard::Star, false) => loaded.state.star_dismissed = true,
            (StarRequestCard::Post, _) => loaded.state.post_done = true,
        }
        ((), true)
    });
}

/// The system's own browser, never Ghostex's: the reader is signed in to GitHub and X there.
#[cfg(not(ghostex_web))]
fn open_in_system_browser(url: &str) {
    if let Err(error) = gpui_open_external_http_url(url) {
        log::warn!("Could not open {url}: {error}");
    }
}

/// The web build never shows the cards (it has no state file), so it has nothing to open.
#[cfg(ghostex_web)]
fn open_in_system_browser(_url: &str) {}

/// Settings > About's Star on GitHub: a star from there counts too, and ends the star card.
/// (The web build's Settings has no such row.)
#[cfg_attr(ghostex_web, allow(dead_code))]
pub(crate) fn note_starred() {
    with_state(|loaded| {
        if loaded.state.starred_on_day.is_some() {
            return ((), false);
        }
        loaded.state.starred_on_day = Some(today());
        ((), true)
    });
}

pub(crate) fn post_on_x_url() -> String {
    let mut url = gpui::http_client::Url::parse("https://x.com/intent/post")
        .expect("the X compose address parses");
    url.query_pairs_mut()
        .append_pair("text", POST_TEXT)
        .append_pair("url", GHOSTEX_REPOSITORY_URL);
    url.to_string()
}

/// The card's fill: a step off the sidebar, see-through on glass so it never sits on it as a slab.
fn card_fill(appearance: &SidebarAppearance) -> Hsla {
    match (appearance.light, appearance.glass) {
        (true, true) => gpui::white().opacity(0.6),
        (true, false) => rgb(0xffffff).into(),
        (false, true) => appearance.foreground.opacity(0.07),
        (false, false) => titlebar_background().blend(appearance.foreground.opacity(0.07)),
    }
}

fn card_button(
    id: &'static str,
    label: &'static str,
    icon: Option<&'static str>,
    primary: bool,
    appearance: &SidebarAppearance,
) -> gpui::Stateful<gpui::Div> {
    let scale = appearance.scale;
    let color = if primary {
        appearance.foreground
    } else {
        appearance.muted
    };
    let button = h_flex()
        .id(id)
        .h(px(24.0 * scale))
        .pl(px(if icon.is_some() { 7.0 } else { 9.0 } * scale))
        .pr(px(9.0 * scale))
        .gap(px(5.0 * scale))
        .rounded(px(6.0 * scale))
        .text_size(px(12.0 * scale))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .cursor_pointer()
        .hover(|button| button.bg(appearance.hover));
    let button = if primary {
        button
            .bg(appearance.foreground.opacity(0.08))
            .border_1()
            .border_color(appearance.foreground.opacity(0.12))
    } else {
        button
    };
    button
        .children(icon.map(|icon| titlebar_svg_icon(icon, 12.0 * scale, color)))
        .child(label)
}

impl GhostexGpuiApp {
    /// The star card or the post card, when one is due; `None` otherwise.
    pub(crate) fn render_native_sidebar_star_request(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let (card, wait) = current_card();
        if let Some(wait) = wait
            && !WAKE_PENDING.swap(true, Ordering::SeqCst)
        {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                WAKE_PENDING.store(false, Ordering::SeqCst);
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        let card = card?;
        let scale = appearance.scale;
        let (title, detail, label, icon, url) = match card {
            StarRequestCard::Star => (
                "Ghostex is free and open source",
                "If it saves you time, a star on GitHub is the best thanks.",
                "Star on GitHub",
                "titlebar/github-mark.svg",
                GHOSTEX_REPOSITORY_URL.to_string(),
            ),
            StarRequestCard::Post => (
                "Thanks for the star!",
                "Tell people what you like about Ghostex on X?",
                "Post on X",
                "titlebar/brand-x-logo.svg",
                post_on_x_url(),
            ),
        };
        let lead_icon = match card {
            StarRequestCard::Star => titlebar_svg_icon(
                "titlebar/star-filled.svg",
                13.0 * scale,
                if appearance.light {
                    rgb(0xc08a12).into()
                } else {
                    rgb(0xf5c451).into()
                },
            ),
            StarRequestCard::Post => titlebar_svg_icon(
                "titlebar/brand-x-logo.svg",
                12.0 * scale,
                appearance.foreground,
            ),
        };
        Some(
            v_flex()
                .id("native-sidebar-star-request")
                .flex_shrink_0()
                .mx(px(8.0 * scale))
                .mt(px(6.0 * scale))
                .mb(px(6.0 * scale))
                .px(px(10.0 * scale))
                .pt(px(8.0 * scale))
                .pb(px(9.0 * scale))
                .rounded(px(9.0 * scale))
                .border_1()
                .border_color(appearance.foreground.opacity(0.12))
                .bg(card_fill(appearance))
                .occlude()
                .text_size(px(12.0 * scale))
                .child(
                    h_flex().gap(px(7.0 * scale)).child(lead_icon).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.5 * scale))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(appearance.foreground)
                            .child(title),
                    ),
                )
                .child(
                    div()
                        .mt(px(3.0 * scale))
                        .pl(px(20.0 * scale))
                        .text_color(appearance.muted)
                        .child(detail),
                )
                .child(
                    h_flex()
                        .mt(px(8.0 * scale))
                        .pl(px(20.0 * scale))
                        .gap(px(6.0 * scale))
                        .flex_wrap()
                        .child(
                            card_button(
                                "native-sidebar-star-request-open",
                                label,
                                Some(icon),
                                true,
                                appearance,
                            )
                            .on_click(cx.listener(
                                move |_, _, _, cx| {
                                    cx.stop_propagation();
                                    open_in_system_browser(&url);
                                    answer(card, true);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            card_button(
                                "native-sidebar-star-request-dismiss",
                                "Dismiss",
                                None,
                                false,
                                appearance,
                            )
                            .on_click(cx.listener(
                                move |_, _, _, cx| {
                                    cx.stop_propagation();
                                    answer(card, false);
                                    cx.notify();
                                },
                            )),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_card_waits_for_enough_prompts_and_a_quiet_moment() {
        let mut state = StarRequestState {
            prompts_sent: PROMPTS_BEFORE_ASKING - 1,
            ..Default::default()
        };
        assert_eq!(state.card(100, true), None);
        state.prompts_sent = PROMPTS_BEFORE_ASKING;
        assert_eq!(state.card(100, false), None);
        assert_eq!(state.card(100, true), Some(StarRequestCard::Star));
    }

    #[test]
    fn dismissing_the_star_card_ends_both_cards() {
        let state = StarRequestState {
            prompts_sent: 50,
            star_dismissed: true,
            ..Default::default()
        };
        assert_eq!(state.card(100, true), None);
        assert_eq!(state.card(200, true), None);
    }

    #[test]
    fn a_star_brings_the_post_card_on_a_later_day_until_it_is_answered() {
        let mut state = StarRequestState {
            prompts_sent: 9,
            starred_on_day: Some(100),
            ..Default::default()
        };
        assert_eq!(state.card(100, true), None);
        assert_eq!(state.card(101, false), Some(StarRequestCard::Post));
        state.post_done = true;
        assert_eq!(state.card(102, true), None);
    }

    #[test]
    fn the_post_link_carries_the_text_and_the_repository() {
        let url = post_on_x_url();
        assert!(url.starts_with("https://x.com/intent/post?text=I%27ve+been+using+Ghostex"));
        assert!(url.ends_with("&url=https%3A%2F%2Fgithub.com%2Fmaddada%2FGhostex"));
    }

    #[test]
    fn an_old_or_partial_state_file_still_reads() {
        let state: StarRequestState = serde_json::from_str(r#"{"promptsSent":3}"#).unwrap();
        assert_eq!(state.prompts_sent, 3);
        assert_eq!(state.starred_on_day, None);
    }
}

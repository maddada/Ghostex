//! The card a published HTML page (`ghostex show`) gets in the transcript: a window bar with three
//! dots and the file name, a scaled-down picture of the page fading out at its bottom (with "Open
//! page" over it on hover), then the title, "Ghostex page · file.html", the Answered pill once
//! the reader sent the page's answers, and Open.
//!
//! The picture is taken by the desktop app from the page the first time the user opens it in the
//! floating page window (`app/visual_page_thumbnails.rs`), or, for a page published since
//! 2026-11-02, from a hidden browser as soon as its card is drawn in the chat on screen
//! (`app/visual_page_background_pictures.rs`). It is stored with the page in gxserver and read
//! through the chat core (`loadPageThumbnail`, answered as a `pageThumbnail` host action), so the web
//! build and the phone show the same one. Until it exists the window shows a plain placeholder: a
//! page icon and the file name.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The user's decision is on server/src/visual_pages/thumbnail.rs. The asking rules are packages/gx-chat-core/src/transcript/page_thumbnails.rs; the measurements are packages/gx-chat-core/visual/page-card.json; the phone draws the same card in apps/mobile/app/src/chat/native/transcript/PageCard.tsx.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use base64::Engine as _;
use ghostex_gx_chat_core::visual as gxv;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, FontWeight, ImageFormat, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, div, img, px, svg,
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap, sync::Arc, time::Duration};

/// `packages/gx-chat-core/visual/page-card.json`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageCardVisual {
    radius: f32,
    bar_height: f32,
    dot_size: f32,
    dot_gap: f32,
    preview_height: f32,
    fade_height: f32,
    footer_padding_x: f32,
    footer_padding_y: f32,
    title_size: f32,
    detail_size: f32,
    thumbnail_width: f32,
    thumbnail_height: f32,
}

static VISUAL: std::sync::LazyLock<PageCardVisual> = std::sync::LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/page-card.json"
    ))
    .expect("shared page card")
});

const CARD_GROUP: &str = "chat-page-card";
/// How long after the card opens its page in the floating window it asks for the new picture.
const PICTURE_AFTER_OPEN: Duration = Duration::from_secs(4);

/// One page's picture as this view knows it.
#[derive(Default)]
struct Thumbnail {
    image: Option<Arc<gpui::Image>>,
    /// True when the stored PNG has a transparent background composited onto the card's backdrop.
    transparent: bool,
    /// The picture is the page's current one (or there will be none); nothing more to ask.
    settled: bool,
    /// A read is out, or the wait before the next one is running.
    asking: bool,
    /// The page's address, as the card that asked shows it.
    url: String,
}

/// The pictures of the transcript's page cards, by `(page_id, answered, light)` so switching theme
/// or answering a page fetches the matching picture cleanly.
#[derive(Default)]
pub(crate) struct PageThumbnails {
    entries: RefCell<HashMap<(String, bool, bool), Thumbnail>>,
}

impl PageThumbnails {
    /// Every picture, for a chat view that is going away (`images.rs`,
    /// `release_chat_images_with_view`).
    pub(super) fn take_pictures(&self) -> Vec<Arc<gpui::Image>> {
        self.entries
            .borrow_mut()
            .drain()
            .filter_map(|(_, thumbnail)| thumbnail.image)
            .collect()
    }
}

/// What a drawn page card's Open needs.
#[derive(Clone, PartialEq)]
struct TagPage {
    page_id: String,
    url: String,
    title: String,
    file: Option<String>,
}

/// The page cards this view drew, by each Q-tag they ask (upper case): `None` once two different
/// pages asked the same tag, which only the core can tell apart.
#[derive(Default)]
pub(crate) struct TagPages {
    by_tag: RefCell<HashMap<String, Option<TagPage>>>,
}

/// Whether the Work panel lists an ask named `tag` (gx-chat-core work_panel.rs `ask_row`, whose
/// rows carry `"ask"` and `"askRow": true`): its Q-tag pill focuses that row, not a page.
fn names_work_ask(value: &Value, tag: &str) -> bool {
    match value {
        Value::Object(map) => {
            (map.get("askRow") == Some(&Value::Bool(true))
                && map
                    .get("ask")
                    .and_then(Value::as_str)
                    .is_some_and(|ask| ask.eq_ignore_ascii_case(tag)))
                || map.values().any(|child| names_work_ask(child, tag))
        }
        Value::Array(items) => items.iter().any(|child| names_work_ask(child, tag)),
        _ => false,
    }
}

impl NativeChatView {
    /// Remembers a page card's tags as it is drawn, for `open_tag_page`.
    pub(super) fn remember_tag_page(&self, page: &gxv::PageRef) {
        let (Some(page_id), Some(tags)) = (page.page_id(), page.questions.as_ref()) else {
            return;
        };
        let seen = TagPage {
            page_id: page_id.to_string(),
            url: page.url.clone(),
            title: page.title.clone(),
            file: page.file.clone(),
        };
        let mut by_tag = self.visual.tag_pages.by_tag.borrow_mut();
        for tag in tags {
            let entry = by_tag
                .entry(tag.to_ascii_uppercase())
                .or_insert_with(|| Some(seen.clone()));
            match entry {
                Some(known) if known.page_id != seen.page_id => *entry = None,
                Some(_) => *entry = Some(seen.clone()),
                None => {}
            }
        }
    }

    /// A Q-tag pill's click that names a page card this view drew: the page opens at once, as the
    /// card's Open does, instead of after the core's answer. False leaves the click to the core.
    ///
    /// CDXC:SessionChat 2026-11-18 WHY:
    /// User: "when i click this Q4170 it doesn't open right away, takes a while to open", with the page's card right above the pill. The card's Open floats the page from the view, but the pill sent `openMarkdownLink` to the chat core and waited for its Open effect; on a long chat the core's queue was seconds deep (`gxChat.host.slowTurn` waited 15 to 27 s while a reply streamed), so the page came up late, while the window itself opens in about 150 ms (`gpui.visualPage.openTiming`). The view now opens the page itself when it drew exactly one page card asking that tag and the Work panel has no ask by it; anything else (a form, a page not drawn yet, a tag two pages ask, a work ask) still goes to the core's `jump_to_tag`.
    pub(super) fn open_tag_page(&mut self, tag: &str, cx: &mut Context<Self>) -> bool {
        if !super::visual_popup::FLOATING_PAGES {
            return false;
        }
        let Some(page) = self
            .visual
            .tag_pages
            .by_tag
            .borrow()
            .get(&tag.to_ascii_uppercase())
            .cloned()
            .flatten()
        else {
            return false;
        };
        if self.snapshot["withdrawnPages"][&page.page_id]["replaced"] == true
            || names_work_ask(&self.snapshot["coordinatorBoard"], tag)
        {
            return false;
        }
        self.open_page_card(page.url, page.title, page.file, Some(page.page_id), cx);
        true
    }

    /// A page card's Open: where Settings > Chat > Open Ghostex pages in says on a host that floats
    /// pages, else where the chat opens every web link. Every card goes to the host's setting
    /// whatever the block's `"open"` says (CDXC:SessionChat DECISION in
    /// app/visual_page_modal_lifecycle.rs).
    pub(super) fn open_page_card(
        &mut self,
        url: String,
        title: String,
        file: Option<String>,
        page_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let floating = super::visual_popup::FLOATING_PAGES;
        // The floating window pictures a page that has no picture yet once it has loaded
        // (app/visual_page_thumbnails.rs); ask for it again a moment after opening.
        if let (true, Some(page_id)) = (floating, page_id.clone()) {
            let timer = cx.background_executor().timer(PICTURE_AFTER_OPEN);
            cx.spawn(async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |this, cx| {
                    let light = ChatAppearance::current(&this.snapshot).light;
                    let theme = if light { "light" } else { "dark" };
                    this.invoke(
                        json!({"type": "loadPageThumbnail", "pageId": page_id, "theme": theme, "fresh": true}),
                        cx,
                    )
                });
            })
            .detach();
        }
        if floating {
            // Floated from the view, so the core never sees the open: tell it, and a page that
            // waits only to be looked at counts as seen (gx-chat-core `questions/later.rs`
            // `seen_on_open`; the core leaves any other page alone).
            if let Some(page_id) = page_id.as_deref().filter(|id| !id.is_empty()) {
                self.invoke(json!({"type": "pageOpened", "pageId": page_id}), cx);
            }
            cx.emit(super::state::NativeChatEvent::Host(json!({
                "type": "open",
                "modal": "visualPage",
                "url": url,
                "title": title,
                "file": file,
            })));
        } else {
            self.invoke(
                json!({"type":"openMarkdownLink","href":url,"external":false}),
                cx,
            )
        }
    }

    /// The page's picture, asking the core for it the first time the card is drawn and again,
    /// while the card is still drawn, each time the core said to wait.
    fn page_thumbnail(
        &self,
        page_id: &str,
        url: &str,
        answered: bool,
        light: bool,
        cx: &Context<Self>,
    ) -> Option<(Arc<gpui::Image>, bool)> {
        let key = (page_id.to_string(), answered, light);
        let mut entries = self.visual.page_thumbnails.entries.borrow_mut();
        let entry = entries.entry(key).or_default();
        if entry.url.is_empty() {
            entry.url = url.to_string();
        }
        if !entry.settled && !entry.asking {
            entry.asking = true;
            let page_id = page_id.to_string();
            let theme = if light { "light" } else { "dark" };
            // Asked on the next turn of the loop, because this runs while the row is rendering.
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.invoke(
                        json!({"type": "loadPageThumbnail", "pageId": page_id, "theme": theme}),
                        cx,
                    )
                });
            })
            .detach();
        }
        entry
            .image
            .as_ref()
            .map(|image| (image.clone(), entry.transparent))
    }

    /// The core's `pageThumbnail` answer (`{pageId, png, settled, retryInMs, theme, transparent}`).
    pub(super) fn receive_page_thumbnail(&mut self, params: &Value, cx: &mut Context<Self>) {
        let page_id = params["pageId"].as_str().unwrap_or_default().to_string();
        if page_id.is_empty() {
            return;
        }
        let image = params["png"]
            .as_str()
            .and_then(|png| base64::engine::general_purpose::STANDARD.decode(png).ok())
            .map(|bytes| Arc::new(gpui::Image::from_bytes(ImageFormat::Png, bytes)));
        let settled = params["settled"].as_bool().unwrap_or(true);
        let retry = params["retryInMs"].as_u64();
        let response_light = match params["theme"].as_str() {
            Some("light") => Some(true),
            Some("dark") => Some(false),
            _ => None,
        };
        let transparent = params["transparent"].as_bool().unwrap_or(false);
        // The page has no picture and is new enough to be pictured unopened: the card that asked
        // is drawn, so the desktop pictures it in the background. Each answer offers it again (the
        // desktop takes each page once per run), so a page let go for now is offered again.
        let picture_unopened = super::visual_popup::FLOATING_PAGES
            && params["backgroundPicture"] == true
            && (image.is_none() || !transparent);
        let mut unopened_url = None;
        let mut replaced = Vec::new();
        {
            let mut entries = self.visual.page_thumbnails.entries.borrow_mut();
            for ((id, _, entry_light), entry) in entries.iter_mut() {
                if *id != page_id {
                    continue;
                }
                if let Some(target_light) = response_light
                    && *entry_light != target_light
                {
                    continue;
                }
                // A stale picture stays up until its replacement arrives.
                if let Some(image) = &image {
                    entry.transparent = transparent;
                    if let Some(old) = entry.image.replace(image.clone())
                        && old.id != image.id
                    {
                        replaced.push(old);
                    }
                }
                entry.settled = settled;
                if picture_unopened && !entry.url.is_empty() {
                    unopened_url.get_or_insert_with(|| entry.url.clone());
                }
            }
        }
        if let Some(url) = unopened_url {
            cx.emit(super::state::NativeChatEvent::Host(json!({
                "type": "sessionChatHostAction",
                "action": "pictureUnopenedPage",
                "pageId": page_id.as_str(),
                "url": url,
            })));
        }
        // GPUI keeps a decoded copy of each picture it painted until it is told to let it go;
        // the notify below draws the card with its new picture first.
        self.release_page_pictures(replaced, cx);
        if let (false, Some(retry)) = (settled, retry) {
            // Waits, then lets the next draw of the card ask again: a card scrolled away asks
            // nothing until it is drawn.
            let timer = cx.background_executor().timer(Duration::from_millis(retry));
            cx.spawn(async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |this, cx| {
                    let mut entries = this.visual.page_thumbnails.entries.borrow_mut();
                    for ((id, _, entry_light), entry) in entries.iter_mut() {
                        if *id == page_id
                            && response_light.is_none_or(|target| *entry_light == target)
                        {
                            entry.asking = false;
                        }
                    }
                    drop(entries);
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// A published HTML page: the window preview card, opened by the chat's own link handling (or
    /// in the floating page window when the block asks for it).
    ///
    /// CDXC:SessionChat 2026-10-06 WHY: The page's address is a random capability URL on gxserver that tells the reader nothing, so the card names the file the agent published instead (`file`, printed by `ghostex show`); a block without one just says it is an interactive page.
    pub(super) fn visual_page_card(
        &self,
        key: &str,
        page: gxv::PageRef,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let v = &*VISUAL;
        let chat = cx.weak_entity();
        let page_id = page.page_id().map(str::to_string);
        // The reader already sent this page's answers (`answeredPages`, page_answers.rs).
        let answered = page_id.as_deref().is_some_and(|page_id| {
            self.snapshot["answeredPages"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == page_id))
        });
        // Answered, with the reader's answers card right under it, which names the page and opens
        // it again (Q2995).
        if answered
            && page_id
                .as_deref()
                .is_some_and(|id| self.answers_right_below(id))
        {
            return div().into_any_element();
        }
        let open_questions = page_id
            .as_deref()
            .and_then(|id| self.snapshot["pageOpenQuestions"][id].as_u64())
            .unwrap_or(0) as usize;
        let collapsible = p.collapse_page_cards;
        let fold_key = format!("page-card:{key}");
        let expanded = !collapsible || self.is_expanded(&fold_key, false);
        if !expanded {
            return self.collapsed_visual_page_card(
                key,
                fold_key,
                page,
                page_id,
                answered,
                open_questions,
                p,
                cx,
            );
        }
        // Its questions a later block withdrew, some or all (`withdrawnPages`, the core's
        // open_questions.rs): the card says so where an answered page says Answered.
        let withdrawn = page_id
            .as_deref()
            .filter(|_| !answered)
            .map(|page_id| &self.snapshot["withdrawnPages"][page_id])
            .filter(|entry| entry.is_object())
            .map(|entry| {
                let tags: Vec<&str> = entry["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                let total = entry["total"].as_u64().unwrap_or(0) as usize;
                // A newer `ghostex show` of the same file replaced it (the core's
                // `Asker::replaced`); the newest card is the page.
                if entry["replaced"] == true {
                    (
                        "Replaced".to_string(),
                        "Replaced by a newer version of this page".to_string(),
                    )
                } else if tags.len() >= total {
                    (
                        "Withdrawn".to_string(),
                        "Withdrawn, no answer needed".to_string(),
                    )
                } else {
                    (
                        format!("{} of {total} withdrawn", tags.len()),
                        format!("{}: withdrawn, no answer needed", tags.join(", ")),
                    )
                }
            });
        // Where the page sits in the ? pill's list: a page still waiting offers For later, one put
        // off says so and offers Back to unanswered (gx-chat-core questions/later.rs).
        let waiting = page_id
            .as_deref()
            .filter(|_| !answered)
            .and_then(|page_id| self.question_section_of(page_id))
            .filter(|section| section == "unanswered" || section == "later");
        let later = waiting.as_deref() == Some("later");
        // Removed from Chat Resources without an answer (gx-chat-core questions/later.rs): the card
        // says Canceled where a withdrawn page says Withdrawn.
        let canceled = page_id
            .as_deref()
            .filter(|_| !answered)
            .and_then(|page_id| self.question_section_of(page_id))
            .is_some_and(|section| section == "canceled");
        // A page that waited only to be looked at and was cleared from the dock (Mark all read,
        // Dismiss) reads Seen, as one marked seen on the page does (gx-chat-core
        // questions/later.rs `mark_seen`).
        let seen_only = page.awaits_seen && page.questions.is_none();
        // A page its agent retired is on that removed list too, but reads Replaced
        // (`withdrawnPages`' `replaced`, gx-chat-core transcript/document.rs).
        let replaced = page_id
            .as_deref()
            .is_some_and(|id| self.snapshot["withdrawnPages"][id]["replaced"] == true);
        let withdrawn = if replaced {
            withdrawn
        } else if canceled && seen_only {
            Some(("Seen".to_string(), "Seen, no answer needed".to_string()))
        } else if canceled {
            Some((
                "Canceled".to_string(),
                "Canceled, no answer needed".to_string(),
            ))
        } else {
            withdrawn
        };
        let picture = page_id
            .as_deref()
            .and_then(|page_id| self.page_thumbnail(page_id, &page.url, answered, p.light, cx));
        let detail = match &page.file {
            Some(file) => format!("Live page · {file}"),
            None => "Live page".to_string(),
        };
        let file = page.file.clone();
        // A page that waited only to be looked at says Seen once marked so (CDXC:SessionChat
        // 2026-11-11 DECISION in server/src/visual_pages/publish.rs).
        let done_label = if page.awaits_seen && page.questions.is_none() {
            "Seen"
        } else {
            "Answered"
        };
        let gxv::PageRef { title, url, .. } = page;
        let export_url = page_id.as_ref().map(|_| url.clone());
        let share_target = page_id.as_ref().map(|id| (id.clone(), title.clone()));
        #[cfg(not(ghostex_web))]
        let is_shared = page_id
            .as_deref()
            .is_some_and(crate::app::window::share_link_popover::is_page_shared);
        #[cfg(ghostex_web)]
        let is_shared = false;
        let bar_label = file.clone().unwrap_or_else(|| title.clone());
        let page_title = title.clone();
        let aria = format!("Open page {title}");
        let placeholder_label = bar_label.clone();
        let opened_page = page_id.clone();
        let open = move |cx: &mut App| {
            let url = url.clone();
            let opened_page = opened_page.clone();
            let _ = chat.update(cx, |chat, cx| {
                chat.open_page_card(url, page_title.clone(), file.clone(), opened_page, cx)
            });
        };
        let open_card = open.clone();
        let window_bg = p.card_row;
        let bar_hover = super::cards::card_hover_fill(p);
        let collapse_key = fold_key.clone();
        let collapse_btn_key = fold_key;
        let bar = div()
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .h(px(v.bar_height * s))
            .px(px(12.0 * s))
            .bg(p.card_panel)
            .border_b(px(1.0))
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap(px(v.dot_gap * s))
                    .children((0..3).map(|_| {
                        div()
                            .size(px(v.dot_size * s))
                            .rounded_full()
                            .bg(p.muted.opacity(0.3))
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_center()
                    .text_size(px(11.5 * s))
                    .text_color(p.muted)
                    .child(bar_label),
            )
            // Balances the dots so the name sits in the middle of the bar, and holds the page's
            // star (gx-chat-core questions/later.rs `starred_page_ids`) and, in Summary view, the
            // collapse button at its right end.
            .child(
                div()
                    .flex_shrink_0()
                    .min_w(px((v.dot_size * 3.0 + v.dot_gap * 2.0) * s))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(2.0 * s))
                    .when_some(page_id.clone(), |this, page_id| {
                        let starred = self.snapshot["starredPages"]
                            .as_array()
                            .is_some_and(|ids| ids.iter().any(|id| id == page_id.as_str()));
                        this.child(star_button(key, &page_id, starred, p, cx))
                    })
                    .when(collapsible, |this| {
                        let key = key.to_string();
                        let fold_key = collapse_btn_key.clone();
                        this.child(collapse_button(&key, fold_key, p, cx))
                    }),
            );
        let bar: AnyElement = if collapsible {
            bar.id(SharedString::from(format!("visual-page-bar:{key}")))
                .role(gpui::Role::Button)
                .aria_label("Collapse page preview")
                .chat_cursor_pointer()
                .hover(move |style| style.bg(bar_hover))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_disclosure(&collapse_key, cx);
                }))
                .into_any_element()
        } else {
            bar.into_any_element()
        };
        // Under window glass, keep the preview area transparent so it blends directly with the
        // card's own translucent surface (`window_bg`) with no separate dark/light slab. When
        // glass is off (solid theme or gpui-web), match the card's own surface (`window_bg`).
        let preview_bg = if p.window_glass {
            gpui::transparent_black()
        } else {
            window_bg
        };
        let has_picture = picture.is_some();
        let preview = div()
            .relative()
            .w_full()
            .h(px(v.preview_height * s))
            .overflow_hidden()
            .bg(preview_bg)
            .when(p.window_glass, |this| {
                this.border_b(px(1.0)).border_color(p.border)
            })
            .map(|this| match picture {
                // As wide as the card, cut at the bottom: the top of the page is what it opens on.
                // Wrapped in a plain `div` with `aspect_ratio` so GPUI/Taffy computes height from
                // the card width (`width / 1.6`) rather than the 2x PNG's intrinsic 1000px height.
                Some((picture, _transparent)) => this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w_full()
                        .aspect_ratio(v.thumbnail_width / v.thumbnail_height)
                        .child(img(picture).size_full().object_fit(gpui::ObjectFit::Fill)),
                ),
                // No picture yet: a page icon and the file name.
                None => this
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0 * s))
                    .child(
                        svg()
                            .path("titlebar/file-type-html.svg")
                            .size(px(22.0 * s))
                            .text_color(p.muted),
                    )
                    .child(
                        div()
                            .max_w(gpui::relative(0.8))
                            .truncate()
                            .text_size(px(11.5 * s))
                            .text_color(p.muted)
                            .child(placeholder_label),
                    ),
            })
            // On opaque themes (glass off), fade the preview cleanly into the card's solid footer;
            // on window glass, avoid painting a dark/opaque fade band over the translucent surface.
            .when(has_picture && !p.window_glass, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(v.fade_height * s))
                        .bg(gpui::linear_gradient(
                            180.0,
                            gpui::linear_color_stop(window_bg.opacity(0.0), 0.0),
                            gpui::linear_color_stop(window_bg, 1.0),
                        )),
                )
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(p.background.opacity(0.45))
                    .opacity(0.0)
                    .group_hover(CARD_GROUP, |style| style.opacity(1.0))
                    .child(primary_button("Open page", p)),
            );
        let footer = div()
            .flex()
            .items_center()
            .gap(px(12.0 * s))
            .px(px(v.footer_padding_x * s))
            .py(px(v.footer_padding_y * s))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(v.title_size * s))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(v.detail_size * s))
                            .text_color(p.muted)
                            .child(detail),
                    ),
            )
            .when_some(withdrawn, |this, (label, hint)| {
                let hint = SharedString::from(hint);
                this.child(
                    div()
                        .id(SharedString::from(format!("page-withdrawn:{key}")))
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .px(px(8.0 * s))
                        .py(px(2.0 * s))
                        .rounded_full()
                        .bg(p.muted.opacity(0.14))
                        .text_size(px(11.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .role(gpui::Role::Status)
                        .aria_label(hint.clone())
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
                        })
                        .child(label),
                )
            })
            .when(later, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0 * s))
                        .flex_shrink_0()
                        .px(px(8.0 * s))
                        .py(px(2.0 * s))
                        .rounded_full()
                        .bg(p.muted.opacity(0.14))
                        .text_size(px(11.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child(
                            svg()
                                .path("titlebar/clock.svg")
                                .size(px(11.0 * s))
                                .text_color(p.muted),
                        )
                        .child("For later"),
                )
            })
            .when_some(waiting.zip(page_id.clone()), |this, (_, page_id)| {
                this.child(later_button(key, &page_id, later, p, cx))
            })
            .when(answered, |this| {
                let good = super::page_answers::answered_color(p);
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0 * s))
                        .flex_shrink_0()
                        .px(px(8.0 * s))
                        .py(px(2.0 * s))
                        .rounded_full()
                        .bg(good.opacity(0.14))
                        .text_size(px(11.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(good)
                        .child(
                            svg()
                                .path("titlebar/check.svg")
                                .size(px(11.0 * s))
                                .text_color(good),
                        )
                        .child(done_label),
                )
            })
            .when(is_shared, |this| {
                let good = super::page_answers::answered_color(p);
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0 * s))
                        .flex_shrink_0()
                        .px(px(8.0 * s))
                        .py(px(2.0 * s))
                        .rounded_full()
                        .bg(good.opacity(0.14))
                        .text_size(px(11.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(good)
                        .child(
                            svg()
                                .path("titlebar/world.svg")
                                .size(px(11.0 * s))
                                .text_color(good),
                        )
                        .child("Shared"),
                )
            })
            .when_some(share_target, |this, (share_page_id, share_title)| {
                #[cfg(not(ghostex_web))]
                {
                    this.child(share_link_button(
                        key,
                        share_page_id,
                        share_title,
                        is_shared,
                        p,
                        cx,
                    ))
                }
                #[cfg(ghostex_web)]
                {
                    let _ = (share_page_id, share_title);
                    this
                }
            })
            .when_some(export_url, |this, export_url| {
                this.child(export_button(key, export_url, p, cx))
            })
            .child(
                div()
                    .id(SharedString::from(format!("visual-page-open:{key}")))
                    .flex_shrink_0()
                    .role(gpui::Role::Button)
                    .aria_label("Open")
                    .chat_cursor_pointer()
                    // The card behind it opens the page too; one press opens it once.
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        open(cx)
                    })
                    .child(primary_button("Open", p)),
            );
        div()
            .id(SharedString::from(format!("visual-page:{key}")))
            .group(CARD_GROUP)
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .bg(window_bg)
            .border_1()
            .border_color(p.border)
            .rounded(px(v.radius * s))
            .role(gpui::Role::Button)
            .aria_label(aria)
            .chat_cursor_pointer()
            .hover(|style| style.border_color(p.muted.opacity(0.6)))
            .on_click(move |_, _, cx| open_card(cx))
            .child(bar)
            .child(preview)
            .child(footer)
            .into_any_element()
    }

    /// A Live page collapsed to one compact row in Summary view: page icon tile, title,
    /// open-questions pill (if any), and Open button. Clicking the row expands the full preview
    /// card; clicking Open opens the page directly.
    #[allow(clippy::too_many_arguments)]
    fn collapsed_visual_page_card(
        &self,
        key: &str,
        fold_key: String,
        page: gxv::PageRef,
        page_id: Option<String>,
        answered: bool,
        open_questions: usize,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let shape = &*super::cards::SINGLE_LINE_CARD;
        let chat = cx.weak_entity();
        let gxv::PageRef {
            title, url, file, ..
        } = page;
        let page_title = title.clone();
        let expand_aria = format!("Expand page {title}");
        let opened_page = page_id.clone();
        let open = move |cx: &mut App| {
            let url = url.clone();
            let opened_page = opened_page.clone();
            let _ = chat.update(cx, |chat, cx| {
                chat.open_page_card(url, page_title.clone(), file.clone(), opened_page, cx)
            });
        };
        let mut tile = shape.tile("titlebar/file-type-html.svg", p);
        if open_questions > 0 {
            tile = tile.child(shape.ask_badge(p));
        } else if answered {
            tile = tile.child(shape.check_badge(p));
        }
        let mut head = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(super::cards::TITLE_ROW_GAP * s))
            .w_full()
            .min_w_0()
            .h(px(shape.header_height(s)))
            .px(px(shape.padding_x * s))
            .child(tile)
            .child(
                shape
                    .title(div().flex_1().min_w_0().truncate(), p)
                    .text_color(p.foreground)
                    .child(title),
            );
        if open_questions > 0 {
            let label = if open_questions == 1 {
                "1 open question".to_string()
            } else {
                format!("{open_questions} open questions")
            };
            head = head.child(shape.pill(None, Some(label), super::ask_card::ask_color(p), p));
        }
        head = head.child(
            div()
                .id(SharedString::from(format!("visual-page-open:{key}")))
                .flex_shrink_0()
                .role(gpui::Role::Button)
                .aria_label("Open")
                .chat_cursor_pointer()
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    open(cx)
                })
                .child(primary_button("Open", p)),
        );
        let head = super::ask_card::single_line_press_head(
            head,
            SharedString::from(format!("visual-page:{key}")),
            "Expand page",
            false,
            p,
        )
        .aria_label(expand_aria)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_disclosure(&fold_key, cx);
        }));
        shape
            .shell(div().w_full().min_w_0().overflow_hidden(), p)
            .child(head)
            .into_any_element()
    }
}

/// The collapse chevron at the right end of an expanded page card's window bar in Summary view.
fn collapse_button(
    key: &str,
    fold_key: String,
    p: &ChatAppearance,
    cx: &Context<NativeChatView>,
) -> AnyElement {
    let s = p.scale;
    let hover = p.foreground;
    div()
        .id(SharedString::from(format!("visual-page-collapse:{key}")))
        .role(gpui::Role::Button)
        .aria_label("Collapse page preview")
        .flex_shrink_0()
        .size(px(22.0 * s))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0 * s))
        .chat_cursor_pointer()
        .hover(move |style| style.bg(hover.opacity(0.08)))
        .tooltip(|window, cx| {
            gpui_component::tooltip::Tooltip::new("Collapse preview").build(window, cx)
        })
        .child(
            svg()
                .path("titlebar/chevron-up.svg")
                .size(px(14.0 * s))
                .text_color(p.muted),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.toggle_disclosure(&fold_key, cx);
        }))
        .into_any_element()
}

/// The star at the right end of the card's window bar: yellow and filled once the page is starred,
/// a quiet outline otherwise. Its press never opens the page behind it.
///
/// CDXC:SessionChat 2026-11-16 DECISION:
/// The user picked the window bar for the card's star (Q3544 on the star-resources page); the rest
/// of the decision is on gx-chat-core questions/later.rs `starred_page_ids`.
fn star_button(
    key: &str,
    page_id: &str,
    starred: bool,
    p: &ChatAppearance,
    cx: &Context<NativeChatView>,
) -> AnyElement {
    let s = p.scale;
    let label = if starred { "Unstar page" } else { "Star page" };
    let action = json!({"type": "pageStar", "id": page_id, "starred": !starred});
    let (icon, color) = if starred {
        (
            "titlebar/star-filled.svg",
            gpui::Hsla::from(gpui::rgb(STAR_YELLOW)),
        )
    } else {
        ("titlebar/star.svg", p.muted)
    };
    let hover = p.foreground;
    div()
        .id(SharedString::from(format!("visual-page-star:{key}")))
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_shrink_0()
        .size(px(22.0 * s))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0 * s))
        .chat_cursor_pointer()
        .hover(move |style| style.bg(hover.opacity(0.08)))
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
        .child(svg().path(icon).size(px(14.0 * s)).text_color(color))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.invoke(action.clone(), cx)
        }))
        .into_any_element()
}

/// The star yellow of a starred page (the settings' favourite star,
/// window/settings_modal/fields/tag_list.rs).
pub(super) const STAR_YELLOW: u32 = 0xf3cd5f;

/// The filled button the card opens its page with (also the hover label over the picture).
/// "For later" on a page still waiting, "Back to unanswered" on one put off: the question form's
/// quiet button (`ask_soft_button`), which keeps its press from opening the page behind it.
fn later_button(
    key: &str,
    page_id: &str,
    later: bool,
    p: &ChatAppearance,
    cx: &Context<NativeChatView>,
) -> AnyElement {
    let s = p.scale;
    let (rest, hover) = if p.light { (0.07, 0.11) } else { (0.10, 0.15) };
    let hover_fill = p.foreground.opacity(hover);
    let label = if later {
        "Back to unanswered"
    } else {
        "For later"
    };
    let action = json!({"type": "questionForLater", "id": page_id, "later": !later});
    div()
        .id(SharedString::from(format!("visual-page-later:{key}")))
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_shrink_0()
        .h(px(28.0 * s))
        .px(px(11.0 * s))
        .flex()
        .items_center()
        .rounded(px(8.0 * s))
        .bg(p.foreground.opacity(rest))
        .text_color(p.foreground)
        .text_size(px(12.5 * s))
        .font_weight(FontWeight::MEDIUM)
        .chat_cursor_pointer()
        .hover(move |style| style.bg(hover_fill))
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.invoke(action.clone(), cx)
        }))
        .into_any_element()
}

/// The address of the page's exported file (server/src/visual_pages/exported.rs): the page's own
/// address with `/export`, which the reader's browser downloads.
#[cfg(ghostex_web)]
fn exported_page_url(url: &str) -> String {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    format!("{}/export", url[..end].trim_end_matches('/'))
}

/// Export: downloads the page as an exported page, one HTML file anyone can open and answer in a
/// browser; its Send gives the answers as JSON, which the chat box imports
/// (CDXC:SessionChat 2026-11-13 DECISION in server/src/visual_pages/exported.rs). The browser
/// downloads it, here or through a remote computer's forward, as Open in browser opens the page.
fn export_button(
    key: &str,
    export_url: String,
    p: &ChatAppearance,
    cx: &Context<NativeChatView>,
) -> AnyElement {
    let s = p.scale;
    let (rest, hover) = if p.light { (0.07, 0.11) } else { (0.10, 0.15) };
    let hover_fill = p.foreground.opacity(hover);
    // The desktop has its own gxserver build the file and saves it (app/visual_page_modal_lifecycle.rs
    // `export_visual_page_to_file`), which works for a page on a computer with an older Ghostex; the
    // web build hands the page's export address to the browser.
    #[cfg(not(ghostex_web))]
    let action = json!({"type": "exportVisualPage", "url": export_url});
    #[cfg(ghostex_web)]
    let action = json!({"type": "openMarkdownLink", "href": exported_page_url(&export_url), "external": true});
    div()
        .id(SharedString::from(format!("visual-page-export:{key}")))
        .role(gpui::Role::Button)
        .aria_label("Export page")
        .flex_shrink_0()
        .h(px(28.0 * s))
        .px(px(11.0 * s))
        .flex()
        .items_center()
        .gap(px(5.0 * s))
        .rounded(px(8.0 * s))
        .bg(p.foreground.opacity(rest))
        .text_color(p.foreground)
        .text_size(px(12.5 * s))
        .font_weight(FontWeight::MEDIUM)
        .chat_cursor_pointer()
        .hover(move |style| style.bg(hover_fill))
        .tooltip(|window, cx| {
            gpui_component::tooltip::Tooltip::new(
                "Export page: one file anyone can open and answer in a browser",
            )
            .build(window, cx)
        })
        .child(
            svg()
                .path("titlebar/file-down.svg")
                .size(px(13.0 * s))
                .text_color(p.foreground),
        )
        .child("Export")
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            #[cfg(not(ghostex_web))]
            cx.emit(super::state::NativeChatEvent::Host(action.clone()));
            #[cfg(ghostex_web)]
            this.invoke(action.clone(), cx);
            let _ = &this;
        }))
        .into_any_element()
}

#[cfg(not(ghostex_web))]
fn share_link_button(
    key: &str,
    page_id: String,
    title: String,
    is_shared: bool,
    p: &ChatAppearance,
    cx: &Context<NativeChatView>,
) -> AnyElement {
    let s = p.scale;
    let (rest, hover) = if p.light { (0.07, 0.11) } else { (0.10, 0.15) };
    let hover_fill = p.foreground.opacity(hover);
    let action = json!({
        "type": "shareVisualPage",
        "pageId": page_id,
        "title": title,
    });
    let label = if is_shared { "Shared" } else { "Share" };
    let tip = if is_shared {
        "Manage shared link for this Live page"
    } else {
        "Share link: serve this Live page over a public tunnel"
    };
    div()
        .id(SharedString::from(format!("visual-page-share:{key}")))
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_shrink_0()
        .h(px(28.0 * s))
        .px(px(11.0 * s))
        .flex()
        .items_center()
        .gap(px(5.0 * s))
        .rounded(px(8.0 * s))
        .bg(p.foreground.opacity(rest))
        .text_color(p.foreground)
        .text_size(px(12.5 * s))
        .font_weight(FontWeight::MEDIUM)
        .chat_cursor_pointer()
        .hover(move |style| style.bg(hover_fill))
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip).build(window, cx))
        .child(
            svg()
                .path("titlebar/link.svg")
                .size(px(13.0 * s))
                .text_color(p.foreground),
        )
        .child(label)
        .on_click(cx.listener(move |_, _, _, cx| {
            cx.stop_propagation();
            cx.emit(super::state::NativeChatEvent::Host(action.clone()));
        }))
        .into_any_element()
}

fn primary_button(label: &'static str, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex()
        .items_center()
        .gap(px(5.0 * s))
        .h(px(28.0 * s))
        .px(px(11.0 * s))
        .rounded(px(8.0 * s))
        .bg(p.control_primary)
        .text_size(px(12.5 * s))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.background)
        .child(label)
        .child(
            svg()
                .path("titlebar/arrow-up-right.svg")
                .size(px(13.0 * s))
                .text_color(p.background),
        )
        .into_any_element()
}

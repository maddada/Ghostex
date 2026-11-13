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
    /// The picture is the page's current one (or there will be none); nothing more to ask.
    settled: bool,
    /// A read is out, or the wait before the next one is running.
    asking: bool,
    /// The page's address, as the card that asked shows it.
    url: String,
}

/// The pictures of the transcript's page cards, by page id and whether the page was answered when
/// the card asked (an answered page is pictured again).
#[derive(Default)]
pub(crate) struct PageThumbnails {
    entries: RefCell<HashMap<(String, bool), Thumbnail>>,
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

impl NativeChatView {
    /// The page's picture, asking the core for it the first time the card is drawn and again,
    /// while the card is still drawn, each time the core said to wait.
    fn page_thumbnail(
        &self,
        page_id: &str,
        url: &str,
        answered: bool,
        cx: &Context<Self>,
    ) -> Option<Arc<gpui::Image>> {
        let key = (page_id.to_string(), answered);
        let mut entries = self.visual.page_thumbnails.entries.borrow_mut();
        let entry = entries.entry(key).or_default();
        if entry.url.is_empty() {
            entry.url = url.to_string();
        }
        if !entry.settled && !entry.asking {
            entry.asking = true;
            let page_id = page_id.to_string();
            // Asked on the next turn of the loop, because this runs while the row is rendering.
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.invoke(json!({"type": "loadPageThumbnail", "pageId": page_id}), cx)
                });
            })
            .detach();
        }
        entry.image.clone()
    }

    /// The core's `pageThumbnail` answer (`{pageId, png, settled, retryInMs}`).
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
        // The page has no picture and is new enough to be pictured unopened: the card that asked
        // is drawn, so the desktop pictures it in the background. Each answer offers it again (the
        // desktop takes each page once per run), so a page let go for now is offered again.
        let picture_unopened = super::visual_popup::FLOATING_PAGES
            && image.is_none()
            && params["backgroundPicture"] == true;
        let mut unopened_url = None;
        let mut replaced = Vec::new();
        {
            let mut entries = self.visual.page_thumbnails.entries.borrow_mut();
            for ((id, _), entry) in entries.iter_mut() {
                if *id != page_id {
                    continue;
                }
                // A stale picture stays up until its replacement arrives.
                if let Some(image) = &image
                    && let Some(old) = entry.image.replace(image.clone())
                    && old.id != image.id
                {
                    replaced.push(old);
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
                    for ((id, _), entry) in entries.iter_mut() {
                        if *id == page_id {
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
        // Every card goes to the host's Open Ghostex pages in setting where the host can float a page,
        // whatever the block's `"open"` says (CDXC:SessionChat DECISION in
        // app/visual_page_modal_lifecycle.rs).
        let floating = super::visual_popup::FLOATING_PAGES;
        let page_id = page.page_id().map(str::to_string);
        // The reader already sent this page's answers (`answeredPages`, page_answers.rs).
        let answered = page_id.as_deref().is_some_and(|page_id| {
            self.snapshot["answeredPages"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == page_id))
        });
        // Answered, with the reader's answers card right under it, which names the page and opens
        // it again (Q2995).
        if answered && page_id.as_deref().is_some_and(|id| self.answers_right_below(id)) {
            return div().into_any_element();
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
                if tags.len() >= total {
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
        let withdrawn = if canceled {
            Some((
                "Canceled".to_string(),
                "Canceled, no answer needed".to_string(),
            ))
        } else {
            withdrawn
        };
        let picture = page_id
            .as_deref()
            .and_then(|page_id| self.page_thumbnail(page_id, &page.url, answered, cx));
        let detail = match &page.file {
            Some(file) => format!("Ghostex page · {file}"),
            None => "Ghostex page".to_string(),
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
        let bar_label = file.clone().unwrap_or_else(|| title.clone());
        let page_title = title.clone();
        let aria = format!("Open page {title}");
        let placeholder_label = bar_label.clone();
        let opened_page = page_id.clone();
        let open = move |cx: &mut App| {
            let url = url.clone();
            let opened_page = opened_page.clone();
            let _ = chat.update(cx, |chat, cx| {
                // The floating window pictures a page that has no picture yet once it has loaded
                // (app/visual_page_thumbnails.rs); ask for it again a moment after opening.
                if let (true, Some(page_id)) = (floating, opened_page) {
                    let timer = cx.background_executor().timer(PICTURE_AFTER_OPEN);
                    cx.spawn(async move |this, cx| {
                        timer.await;
                        let _ = this.update(cx, |this, cx| {
                            this.invoke(
                                json!({"type": "loadPageThumbnail", "pageId": page_id, "fresh": true}),
                                cx,
                            )
                        });
                    })
                    .detach();
                }
                // The desktop opens it where Settings > Chat > Open Ghostex pages in says; a build that
                // cannot float a page opens it where the chat opens every web link.
                if floating {
                    cx.emit(super::state::NativeChatEvent::Host(json!({
                        "type": "open",
                        "modal": "visualPage",
                        "url": url,
                        "title": page_title,
                        "file": file,
                    })));
                } else {
                    chat.invoke(
                        json!({"type":"openMarkdownLink","href":url,"external":false}),
                        cx,
                    )
                }
            });
        };
        let open_card = open.clone();
        let window_bg = p.card_row;
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
            // Balances the dots so the name sits in the middle of the bar.
            .child(
                div()
                    .flex_shrink_0()
                    .w(px((v.dot_size * 3.0 + v.dot_gap * 2.0) * s)),
            );
        let preview = div()
            .relative()
            .w_full()
            .h(px(v.preview_height * s))
            .overflow_hidden()
            .bg(p.background)
            .map(|this| match picture {
                // As wide as the card, cut at the bottom: the top of the page is what it opens on.
                Some(picture) => this.child(
                    img(picture)
                        .absolute()
                        .top_0()
                        .left_0()
                        .w_full()
                        .aspect_ratio(v.thumbnail_width / v.thumbnail_height)
                        .object_fit(gpui::ObjectFit::Fill),
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
            // Painted, not masked: a plain div with no id takes no input.
            .child(
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
            .when_some(export_url, |this, export_url| {
                this.child(export_button(key, export_url, p, cx))
            })
            .child(
                div()
                    .id("visual-page-open")
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
}

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
#[cfg(target_arch = "wasm32")]
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
    #[cfg(not(target_arch = "wasm32"))]
    let action = json!({"type": "exportVisualPage", "url": export_url});
    #[cfg(target_arch = "wasm32")]
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
                .path("titlebar/download.svg")
                .size(px(13.0 * s))
                .text_color(p.foreground),
        )
        .child("Export")
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            #[cfg(not(target_arch = "wasm32"))]
            cx.emit(super::state::NativeChatEvent::Host(action.clone()));
            #[cfg(target_arch = "wasm32")]
            this.invoke(action.clone(), cx);
            let _ = &this;
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

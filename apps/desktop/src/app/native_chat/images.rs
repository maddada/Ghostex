//! Pictures shared in the conversation.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! The core classifies each block with `image_source` in
//! packages/gx-chat-core/src/transcript/images.rs. A machine path is bytes only
//! `readSessionChatImage` can serve, because the picture lives on the session's machine, so it is
//! read once here and shared by the thumbnail and the full-size viewer. A picture that cannot be
//! read renders the named chip it would otherwise have been, never a broken image well.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use base64::Engine as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ImageFormat, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderImage, StatefulInteractiveElement as _, Styled as _, StyledImage as _, div, img, px, svg,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, LazyLock},
};

/// The measurements React drew a transcript picture with, read from `image-visual.json`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChatImageVisual {
    /// React's `.ghostex-chat-inline-image`: a 3rem square.
    pub(super) thumbnail_size: f32,
    pub(super) thumbnail_radius: f32,
    pub(super) border_width: f32,
    /// `gap-1.5` between the user's own pictures, `gap-2` between an agent's.
    pub(super) user_row_gap: f32,
    pub(super) assistant_row_gap: f32,
    pub(super) row_padding_y: f32,
    /// The air React leaves around a picture written into prose.
    pub(super) inline_margin_x: f32,
    pub(super) inline_margin_y: f32,
    pub(super) loading_icon_size: f32,
}

/// CDXC:SessionChat 2026-09-19 SEE-ALSO: The thumbnail's size, radius, hairline and row gaps come
/// from packages/gx-chat-core/visual/image-visual.json, which mirrors the CSS in
/// packages/core-ui/styles/chat.css.
pub(super) static VISUAL: LazyLock<ChatImageVisual> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/image-visual.json"
    ))
    .expect("shared transcript image appearance")
});

/// Device pixels per point a thumbnail is rasterized at.
///
/// A screenshot painted straight into a 48pt tile leaves the GPU to shrink it by twenty times or
/// more with a single sample per pixel, which is the mush a full-size picture used to show here.
/// Two per point is 1:1 on every Retina display and a clean halving on a 1x one, and the browser
/// does the same thing for React's `<img>`.
const THUMBNAIL_PIXEL_RATIO: f32 = 2.0;

/// A picture this big is left to GPUI rather than decoded twice: the resize would cost more than
/// the sampling it saves.
const THUMBNAIL_MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;

enum ChatImageEntry {
    Loading,
    Ready(Arc<gpui::Image>),
    Failed,
}

/// One picture shrunk to the tile it is painted in.
enum ChatImageThumbnail {
    Loading,
    Ready(Arc<RenderImage>),
    /// Bytes no decoder here reads (an SVG, which GPUI rasterizes at the tile's own size anyway):
    /// the picture itself is painted instead.
    Whole,
}

/// Bytes for the transcript's pictures, read once per location and shared by every thumbnail and the viewer.
///
/// A picture written into prose is asked for from the Markdown renderer, which runs with the view
/// borrowed shared, so the map is behind a cell rather than reachable only from `&mut self`.
#[derive(Default)]
pub(crate) struct ChatImageCache {
    entries: RefCell<HashMap<String, ChatImageEntry>>,
    /// Cover-cropped copies keyed by the picture and the device pixels it is painted at, so a zoom
    /// change rasterizes once more and scrolling never resizes anything.
    thumbnails: RefCell<HashMap<(u64, u32), ChatImageThumbnail>>,
}

/// What a renderer can do with one picture right now.
pub(super) enum ChatImageSource {
    /// An http(s) address GPUI fetches itself.
    Uri(String),
    Bytes(Arc<gpui::Image>),
    Loading,
    /// No transport, unreadable bytes, or a format GPUI cannot decode: the named chip stands in.
    Unavailable,
}

/// What one transcript tile paints: the same picture, already shrunk to the size it is drawn at.
enum ChatImageTile {
    Uri(String),
    Thumbnail(Arc<RenderImage>),
    Whole(Arc<gpui::Image>),
    Loading,
    Unavailable,
}

fn image_format(media_type: &str) -> Option<ImageFormat> {
    match media_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/webp" => Some(ImageFormat::Webp),
        "image/gif" => Some(ImageFormat::Gif),
        "image/svg+xml" => Some(ImageFormat::Svg),
        "image/bmp" => Some(ImageFormat::Bmp),
        "image/tiff" => Some(ImageFormat::Tiff),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some(ImageFormat::Ico),
        _ => None,
    }
}

fn decode_data_url(url: &str) -> Option<gpui::Image> {
    let (meta, payload) = url.strip_prefix("data:")?.split_once(',')?;
    if !meta.contains(";base64") {
        return None;
    }
    let format = image_format(meta.split(';').next().unwrap_or_default())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .ok()?;
    Some(gpui::Image::from_bytes(format, bytes))
}

/// The 3rem square React draws for a picture (`.ghostex-chat-inline-image`), or nothing when its
/// bytes are not there and the caller has to fall back to the picture's name.
fn thumbnail(source: &ChatImageTile, p: &ChatAppearance) -> Option<AnyElement> {
    let s = p.scale;
    let size = px(VISUAL.thumbnail_size * s);
    let radius = px(VISUAL.thumbnail_radius * s);
    let hairline = px(VISUAL.border_width * s);
    // `ObjectFit::Cover` paints the scaled picture through the bounds it was given instead of
    // cropping to them, so the tile itself has to be the clipping well React's `overflow-hidden`
    // rounded box is; without it a wide picture bleeds over its neighbours and the pane's edge.
    let framed = |picture: gpui::Img| {
        div()
            .size(size)
            .flex_shrink_0()
            .overflow_hidden()
            .rounded(radius)
            .border(hairline)
            .border_color(p.border)
            .child(
                picture
                    .size_full()
                    .rounded(radius)
                    .object_fit(gpui::ObjectFit::Cover),
            )
            .into_any_element()
    };
    match source {
        ChatImageTile::Uri(url) => Some(framed(img(url.clone()))),
        ChatImageTile::Thumbnail(picture) => Some(framed(img(picture.clone()))),
        ChatImageTile::Whole(bytes) => Some(framed(img(bytes.clone()))),
        // React's `.ghostex-chat-inline-image-pending`: the same square with a dashed hairline and
        // a turning loader, so a picture being read looks like one arriving rather than one lost.
        ChatImageTile::Loading => Some(
            div()
                .size(size)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius)
                .border(hairline)
                .border_dashed()
                .border_color(p.border)
                .child(
                    svg()
                        .path("titlebar/loader2.svg")
                        .size(px(VISUAL.loading_icon_size * s))
                        .text_color(p.muted)
                        .with_throttled_animation(
                            "chat-image-spinner",
                            std::time::Duration::from_millis(900),
                            |svg, delta| {
                                svg.with_transformation(gpui::Transformation::rotate(
                                    gpui::radians(delta * std::f32::consts::TAU),
                                ))
                            },
                        ),
                )
                .into_any_element(),
        ),
        ChatImageTile::Unavailable => None,
    }
}

/// The device pixels one tile is painted with at the transcript's current zoom.
fn thumbnail_pixels(p: &ChatAppearance) -> u32 {
    (VISUAL.thumbnail_size * p.scale * THUMBNAIL_PIXEL_RATIO)
        .ceil()
        .max(1.0) as u32
}

/// Shrinks one picture to the square it is painted in, off the UI thread.
///
/// The crop is centred, which is what React's `object-fit: cover` does, and the sampling averages
/// every source pixel that lands in a tile pixel instead of picking one of them, which is the
/// difference between a readable screenshot and noise.
fn build_thumbnail(picture: &gpui::Image, pixels: u32) -> Option<RenderImage> {
    if picture.format == ImageFormat::Svg {
        return None;
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(picture.bytes.as_slice()))
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0
        || height == 0
        || u64::from(width) * u64::from(height) > THUMBNAIL_MAX_SOURCE_PIXELS
    {
        return None;
    }
    let decoded = image::load_from_memory(&picture.bytes).ok()?;
    let side = width.min(height);
    let square = decoded
        .crop_imm((width - side) / 2, (height - side) / 2, side, side)
        .into_rgba8();
    let mut data = if side >= pixels {
        image::imageops::thumbnail(&square, pixels, pixels)
    } else {
        image::imageops::resize(
            &square,
            pixels,
            pixels,
            image::imageops::FilterType::Triangle,
        )
    };
    // gpui uploads sprite atlas tiles as BGRA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(RenderImage::new(vec![image::Frame::new(data)]))
}

fn decode_read_result(result: &Value) -> Option<gpui::Image> {
    let format = image_format(result["mediaType"].as_str()?)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(result["base64Data"].as_str()?)
        .ok()?;
    Some(gpui::Image::from_bytes(format, bytes))
}

impl NativeChatView {
    /// Resolves one projected image source, starting the read the first time it is asked for.
    pub(super) fn chat_image(&self, image: &Value, cx: &Context<Self>) -> ChatImageSource {
        let transport = image["transport"].as_str().unwrap_or("none");
        if transport == "url" {
            let url = text(image, "url");
            return if url.is_empty() {
                ChatImageSource::Unavailable
            } else {
                ChatImageSource::Uri(url)
            };
        }
        let key = match transport {
            "data" => text(image, "url"),
            "read" => text(image, "path"),
            _ => String::new(),
        };
        if key.is_empty() {
            return ChatImageSource::Unavailable;
        }
        match self.images.entries.borrow().get(&key) {
            Some(ChatImageEntry::Ready(image)) => return ChatImageSource::Bytes(image.clone()),
            Some(ChatImageEntry::Failed) => return ChatImageSource::Unavailable,
            Some(ChatImageEntry::Loading) => return ChatImageSource::Loading,
            None => {}
        }
        self.images
            .entries
            .borrow_mut()
            .insert(key.clone(), ChatImageEntry::Loading);
        if transport == "data" {
            let decode_key = key.clone();
            let task = cx
                .background_executor()
                .spawn(async move { decode_data_url(&decode_key) });
            cx.spawn(async move |this, cx| {
                let loaded = task.await;
                let _ = this.update(cx, |this, cx| this.store_chat_image(key, loaded, cx));
            })
            .detach();
        } else {
            // The path is on the session's machine, so the read goes through the shared transport.
            // Asked for on the next turn of the loop, because this runs while the row is rendering.
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.invoke(json!({"type": "loadImage", "path": key}), cx)
                });
            })
            .detach();
        }
        ChatImageSource::Loading
    }

    /// The same picture as `chat_image`, shrunk once to the square the transcript paints it in.
    ///
    /// The viewer still opens the original; only the tile trades it for a copy the size of the
    /// tile, which is what keeps a transcript of screenshots readable and cheap to scroll.
    fn chat_image_tile(
        &self,
        image: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> ChatImageTile {
        let picture = match self.chat_image(image, cx) {
            // An address GPUI fetches and scales itself, as the browser does for React.
            ChatImageSource::Uri(url) => return ChatImageTile::Uri(url),
            ChatImageSource::Loading => return ChatImageTile::Loading,
            ChatImageSource::Unavailable => return ChatImageTile::Unavailable,
            ChatImageSource::Bytes(picture) => picture,
        };
        if picture.format == ImageFormat::Svg {
            // Drawn from its own shapes at whatever size it is given, so there is nothing to shrink
            // and nothing to wait for.
            return ChatImageTile::Whole(picture);
        }
        let key = (picture.id, thumbnail_pixels(p));
        match self.images.thumbnails.borrow().get(&key) {
            Some(ChatImageThumbnail::Ready(thumbnail)) => {
                return ChatImageTile::Thumbnail(thumbnail.clone());
            }
            Some(ChatImageThumbnail::Whole) => return ChatImageTile::Whole(picture),
            Some(ChatImageThumbnail::Loading) => return ChatImageTile::Loading,
            None => {}
        }
        self.images
            .thumbnails
            .borrow_mut()
            .insert(key, ChatImageThumbnail::Loading);
        let source = picture.clone();
        let task = cx
            .background_executor()
            .spawn(async move { build_thumbnail(&source, key.1).map(Arc::new) });
        cx.spawn(async move |this, cx| {
            let thumbnail = task.await;
            let _ = this.update(cx, |this, cx| {
                this.images.thumbnails.borrow_mut().insert(
                    key,
                    match thumbnail {
                        Some(thumbnail) => ChatImageThumbnail::Ready(thumbnail),
                        None => ChatImageThumbnail::Whole,
                    },
                );
                cx.notify();
            });
        })
        .detach();
        ChatImageTile::Loading
    }

    /// The transport answered a `loadImage` ask (the core's `chatImage` request).
    pub(super) fn receive_chat_image(&mut self, request: &Value, cx: &mut Context<Self>) {
        let path = text(&request["params"], "path");
        if path.is_empty() {
            return;
        }
        if request["method"] != "loaded" {
            self.store_chat_image(path, None, cx);
            return;
        }
        let params = request["params"].clone();
        let task = cx
            .background_executor()
            .spawn(async move { decode_read_result(&params) });
        cx.spawn(async move |this, cx| {
            let loaded = task.await;
            let _ = this.update(cx, |this, cx| this.store_chat_image(path, loaded, cx));
        })
        .detach();
    }

    fn store_chat_image(
        &mut self,
        key: String,
        loaded: Option<gpui::Image>,
        cx: &mut Context<Self>,
    ) {
        self.images.entries.borrow_mut().insert(
            key,
            match loaded {
                Some(image) => ChatImageEntry::Ready(Arc::new(image)),
                None => ChatImageEntry::Failed,
            },
        );
        self.list.remeasure();
        cx.notify();
    }

    /// The user turn's own pictures, right-aligned above the bubble (React: `UserImageThumbnails`).
    pub(super) fn user_image_thumbnails(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.image_row(message, true, p, cx)
    }

    /// Pictures an agent shared, left-aligned above its prose (React: `ImageAttachments`).
    pub(super) fn assistant_image_attachments(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.image_row(message, false, p, cx)
    }

    fn image_row(
        &mut self,
        message: &Value,
        user: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let images = message["images"].as_array()?.clone();
        if images.is_empty() {
            return None;
        }
        let id = text(message, "id");
        // React: `gap-2` between an agent's pictures (`ImageAttachments`), `gap-1.5` between the
        // user's own (`UserImageThumbnails`), both with `py-1`.
        let mut row = div()
            .flex()
            .flex_wrap()
            .min_w_0()
            .gap(px(if user {
                VISUAL.user_row_gap
            } else {
                VISUAL.assistant_row_gap
            } * p.scale))
            .py(px(VISUAL.row_padding_y * p.scale))
            .when(user, |row| row.justify_end());
        for index in 0..images.len() {
            row = row.child(self.image_tile(&id, &images, index, user, p, cx));
        }
        Some(row.into_any_element())
    }

    fn image_tile(
        &mut self,
        message_id: &str,
        images: &[Value],
        index: usize,
        user: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let image = &images[index];
        let label = text(image, "label");
        let alt = text(image, "alt");
        let source = self.chat_image_tile(image, p, cx);
        let open = images.to_vec();
        let tile = div()
            .id(gpui::SharedString::from(format!(
                "chat-image:{message_id}:{index}"
            )))
            .role(gpui::Role::Button)
            .aria_label(if alt.is_empty() {
                format!("View {label}")
            } else {
                format!("View {alt}")
            })
            .flex_shrink_0()
            .chat_cursor_pointer()
            .on_click(
                cx.listener(move |chat, _, _, cx| chat.open_image_viewer(open.clone(), index, cx)),
            );
        match thumbnail(&source, p) {
            Some(picture) => tile.child(picture).into_any_element(),
            // A host with no image transport, or a file that has since gone: the honest stand-in.
            None if user => div()
                .flex_shrink_0()
                .text_size(px(12.0 * s))
                .text_color(p.muted)
                .child(if label.is_empty() {
                    format!("Image #{}", index + 1)
                } else {
                    label
                })
                .into_any_element(),
            // React's `Attachment size='xs'`: a 12px-radius card with a 4px inset, a 28px rounded
            // media well for the icon, and a medium-weight title in the card's own colour. Its
            // `min-w-40` is what keeps a short file name from shrinking the card to a pill.
            None => div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.0 * s))
                .min_w(px(160.0 * s))
                .max_w(px(220.0 * s))
                .p(px(4.0 * s))
                .rounded(px(12.0 * s))
                .border(px(s))
                .border_color(p.border)
                .bg(p.card_background)
                .text_size(px(12.0 * s))
                .text_color(p.foreground)
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(28.0 * s))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.0 * s))
                        .bg(p.input)
                        .child(
                            svg()
                                .path("chat-actions/photo")
                                .size(px(14.0 * s))
                                .flex_shrink_0()
                                .text_color(p.foreground),
                        ),
                )
                .child(
                    div()
                        .min_w_0()
                        .px(px(6.0 * s))
                        .py(px(4.0 * s))
                        .truncate()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(label),
                )
                .into_any_element(),
        }
    }

    /// A picture written into prose, at the position its author wrote it.
    ///
    /// React drew the same 3rem thumbnail for a Markdown image and for a link that names a picture
    /// (`SessionChatInlineImage`); the shared projection marks both, so a `data:` URL and a machine
    /// path load through this transport instead of the blank band a `TextView` leaves behind. When
    /// the bytes cannot be read the picture's own words stand in, exactly as React's fallback did.
    pub(super) fn inline_image(
        &self,
        id: &str,
        image: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let label = text(image, "label");
        let alt = text(image, "alt");
        let source = self.chat_image_tile(image, p, cx);
        let open = vec![image.clone()];
        match thumbnail(&source, p) {
            // React leaves 4px either side and 2px above and below a picture written into prose
            // (`.ghostex-chat-markdown .ghostex-chat-inline-image-frame`), so the words beside it
            // keep their air whether or not the line wraps.
            Some(picture) => div()
                .id(gpui::SharedString::from(format!("chat-inline-image:{id}")))
                .role(gpui::Role::Button)
                .aria_label(if alt.is_empty() {
                    format!("View {label}")
                } else {
                    format!("View {alt}")
                })
                .flex_shrink_0()
                .mx(px(VISUAL.inline_margin_x * p.scale))
                .my(px(VISUAL.inline_margin_y * p.scale))
                .chat_cursor_pointer()
                // Opening the viewer consumes the press: React's picture is a
                // button, which its row's click handler skips over.
                .on_click(cx.listener(move |chat, _, _, cx| {
                    cx.stop_propagation();
                    chat.open_image_viewer(open.clone(), 0, cx);
                }))
                .child(picture)
                .into_any_element(),
            None => div()
                .flex_shrink_0()
                .child(if label.is_empty() {
                    "Image".to_owned()
                } else {
                    label
                })
                .into_any_element(),
        }
    }
}

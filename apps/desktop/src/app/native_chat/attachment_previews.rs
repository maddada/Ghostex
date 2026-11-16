use super::file_actions::FileMenuExt as _;
use super::{appearance::ChatAppearance, images::ChatImageTile, state::NativeChatView};
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, div, img, px,
};
use serde_json::{Value, json};

/// Side of a thumbnail tile, matching React's `h-12 w-12` previews.
const TILE_PX: f32 = 48.0;
/// Hover group of one tile, which is what uncovers its remove button.
const TILE_GROUP: &str = "chat-attachment";

impl NativeChatView {
    /// Drops the markdown reference this tile stands for, through the shared removal rule.
    fn remove_composer_attachment(
        &mut self,
        range: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let start = self.draft[..range.start].encode_utf16().count();
        let end = self.draft[..range.end].encode_utf16().count();
        self.invoke(
            json!({"type":"removeAttachment","text":self.draft,"start":start,"end":end}),
            cx,
        );
    }

    /// The draft's image references in the shape the viewer reads, and where `path` sits in them.
    ///
    /// The viewer reads the same projected shape a transcript picture uses (`images.rs`), so a
    /// pasted image reaches it through the session's transport instead of a local file read.
    pub(super) fn composer_image_gallery(&self, path: &str) -> (Vec<Value>, usize) {
        let paths: Vec<&str> = self
            .composer_references
            .iter()
            .filter(|reference| reference.kind == "image")
            .map(|reference| reference.path.as_str())
            .collect();
        let index = paths.iter().position(|other| *other == path).unwrap_or(0);
        let images = paths
            .into_iter()
            .map(|path| json!({"transport":"read","path":path,"label":path,"alt":"Pasted image"}))
            .collect();
        (images, index)
    }

    /// The composer's pasted and dropped image thumbnails, with the uploading tile React showed.
    ///
    /// CDXC:SessionChat 2026-09-18 SEE-ALSO:
    /// The tiles come from the same `[Image #N](path)` references the composer already paints as
    /// pills (`composer_references.rs`), so a reference the user deletes by hand drops its tile too.
    ///
    /// CDXC:SessionChat 2026-09-20 DECISION:
    /// User: a tile's remove button shows only while that tile is hovered, and the tile's tooltip
    /// names the reference (`Image #1`) instead of repeating the whole file path, which the pill's
    /// own tooltip already shows.
    pub(super) fn render_attachment_previews(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pending = self
            .paste_tiles
            .shown(self.snapshot["pendingAttachments"].as_u64().unwrap_or(0));
        let active = self.composer_active_image(cx);
        let failed = self.failed_upload_tiles(p, cx);
        self.render_reference_previews(
            self.composer_references.clone(),
            pending,
            active,
            None,
            false,
            failed,
            p,
            cx,
        )
    }

    /// The pictures and files whose upload failed twice (`failed_uploads`, composer_attach.rs):
    /// each a tile with a red ring and mark, the picture dimmed, its remove button, and after them
    /// the words saying Send uploads them again.
    ///
    /// CDXC:Clipboard 2026-11-15 DECISION (issue #618, Q3274 A): never "tap the picture to retry"; Send retries.
    /// SEE-ALSO: apps/mobile/app/src/chat/native/composer/AttachmentPreviews.tsx (`PICTURE_FAILED_WORDS`).
    fn failed_upload_tiles(&self, p: &ChatAppearance, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.failed_uploads.is_empty() {
            return Vec::new();
        }
        let s = p.scale;
        let error = p.error();
        let words = super::composer_attach::UPLOAD_FAILED_WORDS;
        let mut tiles: Vec<AnyElement> = self
            .failed_uploads
            .iter()
            .map(|failed| {
                let key = failed.key;
                let (name, picture) = match &failed.upload {
                    super::composer_attach::Upload::Picture(image) => (
                        "Pasted image".to_owned(),
                        Some(std::sync::Arc::new(image.clone())),
                    ),
                    super::composer_attach::Upload::File(path) => (
                        std::path::Path::new(path)
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.clone()),
                        None,
                    ),
                };
                let tooltip = if failed.error.trim().is_empty() {
                    format!("{name}: {words}")
                } else {
                    format!("{name}: {words} ({})", failed.error.trim())
                };
                div()
                    .relative()
                    .flex_shrink_0()
                    .group(TILE_GROUP)
                    .size(px(TILE_PX * s))
                    .child(
                        div()
                            .id(("chat-attachment-failed", key as usize))
                            .role(gpui::Role::Image)
                            .aria_label(format!("{name}: {words}"))
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0 * s))
                            .overflow_hidden()
                            .border(px(1.5 * s))
                            .border_color(error)
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(tooltip.clone())
                                    .build(window, cx)
                            })
                            .when_some(picture, |tile, image| {
                                tile.child(
                                    img(image)
                                        .absolute()
                                        .size_full()
                                        .opacity(0.55)
                                        .object_fit(gpui::ObjectFit::Cover),
                                )
                            })
                            .child(
                                div()
                                    .size(px(18.0 * s))
                                    .rounded_full()
                                    .bg(error)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.0 * s))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(gpui::rgb(if p.light {
                                        0xffffff
                                    } else {
                                        0x1a0b0b
                                    }))
                                    .child("!"),
                            ),
                    )
                    .child(
                        div()
                            .id(("chat-attachment-failed-remove", key as usize))
                            .role(gpui::Role::Button)
                            .aria_label("Remove image")
                            .invisible()
                            .group_hover(TILE_GROUP, |style| style.visible())
                            .block_mouse_except_scroll()
                            .absolute()
                            .top(px(-5.0 * s))
                            .right(px(-5.0 * s))
                            .chat_cursor_pointer()
                            .size(px(16.0 * s))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(p.input_border)
                            .bg(p.background)
                            .hover(|style| {
                                style
                                    .visible()
                                    .bg(p.background.blend(p.muted.opacity(0.35)))
                            })
                            .child(
                                gpui::svg()
                                    .path("titlebar/x.svg")
                                    .size(px(9.0 * s))
                                    .text_color(p.muted),
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.drop_failed_upload(key, cx)),
                            ),
                    )
                    .into_any_element()
            })
            .collect();
        tiles.push(
            div()
                .min_w_0()
                .text_size(px(12.0 * s))
                .text_color(error)
                .child(words)
                .into_any_element(),
        );
        tiles
    }

    pub(super) fn render_reference_previews(
        &mut self,
        references: Vec<super::composer_references::ComposerReference>,
        pending: u64,
        active: Option<String>,
        answer_key: Option<String>,
        disabled: bool,
        failed: Vec<AnyElement>,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let attachments: Vec<_> = references
            .iter()
            .filter(|reference| reference.kind == "image")
            .map(|reference| {
                (
                    reference.range.clone(),
                    reference.path.clone(),
                    reference.label.clone(),
                )
            })
            .collect();
        if attachments.is_empty() && pending == 0 && failed.is_empty() {
            return None;
        }
        // The viewer reads the same projected shape a transcript picture uses (`images.rs`), so a
        // pasted image reaches it through the session's transport instead of a local file read.
        let images: Vec<Value> = attachments
            .iter()
            .map(|(_, path, _)| json!({"transport":"read","path":path,"label":path,"alt":"Pasted image"}))
            .collect();
        // React's `flex flex-wrap items-center gap-2 pb-2`: separate rounded chips with a real gap
        // between them, never one fused strip.
        let mut row = div()
            .id(answer_key
                .as_ref()
                .map(|key| format!("answer-attachments:{key}"))
                .unwrap_or_else(|| "composer-attachments".into()))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0 * s))
            .pb(px(8.0 * s));
        for (index, (range, path, label)) in attachments.into_iter().enumerate() {
            let removed = range.clone();
            let answer_key = answer_key.clone();
            let outlined = active.as_deref() == Some(path.as_str());
            // The transcript's own cover-cropped copy (the tile is the transcript thumbnail's
            // size): painting the picture itself here made GPUI decode it at full size and keep
            // that copy for as long as the app ran.
            let source = self.chat_image_tile(&images[index], p, cx);
            let open = images.clone();
            row = row.child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .group(TILE_GROUP)
                    .size(px(TILE_PX * s))
                    .child(
                        div()
                            .id(("chat-attachment", index))
                            .role(gpui::Role::Button)
                            .aria_label("View pasted image")
                            .chat_cursor_pointer()
                            .size_full()
                            .rounded(px(8.0 * s))
                            .overflow_hidden()
                            .border_1()
                            .border_color(p.input_border)
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(label.clone())
                                    .build(window, cx)
                            })
                            .when_some(
                                match source {
                                    ChatImageTile::Thumbnail(shrunk) => Some(img(shrunk)),
                                    ChatImageTile::Whole(bytes) => Some(img(bytes)),
                                    ChatImageTile::Uri(url) => Some(img(url)),
                                    ChatImageTile::Loading | ChatImageTile::Unavailable(_) => None,
                                },
                                |tile, image| {
                                    tile.child(image.size_full().object_fit(gpui::ObjectFit::Cover))
                                },
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_image_viewer(open.clone(), index, cx)
                            }))
                            .file_menu(super::file_actions::image_file_path(&images[index]), cx),
                    )
                    // React's `outline: 2px solid white; outline-offset: 1px` on the active
                    // thumbnail. GPUI has no outline, so a ring is painted around the tile; it has
                    // no id or listener, so it takes no input.
                    .when(outlined, |tile| {
                        tile.child(
                            div()
                                .absolute()
                                .top(px(-3.0 * s))
                                .left(px(-3.0 * s))
                                .right(px(-3.0 * s))
                                .bottom(px(-3.0 * s))
                                .rounded(px(11.0 * s))
                                .border_2()
                                .border_color(gpui::white()),
                        )
                    })
                    .child(
                        div()
                            .id(("chat-attachment-remove", index))
                            .role(gpui::Role::Button)
                            .aria_label("Remove image")
                            // The button hangs over the tile's corner, so its own hover keeps it
                            // up once the pointer has left the tile's bounds for it.
                            .invisible()
                            .group_hover(TILE_GROUP, |style| style.visible())
                            // GPUI hovers every hitbox under the pointer, so over the tile's
                            // corner a click also reached the tile and opened the viewer.
                            .block_mouse_except_scroll()
                            .absolute()
                            .top(px(-5.0 * s))
                            .right(px(-5.0 * s))
                            .chat_cursor_pointer()
                            .size(px(16.0 * s))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(p.input_border)
                            // It hangs over the picture, so it is opaque at rest too: under window
                            // glass `card_background` is clear and the picture read through it.
                            .bg(p.background)
                            /*
                            CDXC:SessionChat 2026-09-27 DECISION:
                            User: a hovered remove button has a grayish background, never a transparent one. The palette's `border` and `card_background` are see-through washes under window glass, so the gray is the muted tone mixed into the opaque chat background.
                            */
                            .hover(|style| {
                                style
                                    .visible()
                                    .bg(p.background.blend(p.muted.opacity(0.35)))
                            })
                            .child(
                                gpui::svg()
                                    .path("titlebar/x.svg")
                                    .size(px(9.0 * s))
                                    .text_color(p.muted),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if disabled {
                                    return;
                                }
                                if let Some(key) = &answer_key {
                                    this.remove_answer_attachment(key, removed.clone(), window, cx);
                                } else {
                                    this.remove_composer_attachment(removed.clone(), cx)
                                }
                            })),
                    ),
            );
        }
        if pending > 0 {
            row = row.child(
                div()
                    .flex_shrink_0()
                    .size(px(TILE_PX * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0 * s))
                    .border_dashed()
                    .border_1()
                    .border_color(p.input_border)
                    .child(
                        gpui::svg()
                            .path("titlebar/loader2.svg")
                            .size(px(16.0 * s))
                            .text_color(p.muted)
                            .with_throttled_animation(
                                "chat-attachment-spinner",
                                std::time::Duration::from_millis(900),
                                |svg, delta| {
                                    svg.with_transformation(gpui::Transformation::rotate(
                                        gpui::radians(delta * std::f32::consts::TAU),
                                    ))
                                },
                            ),
                    ),
            );
        }
        Some(row.children(failed).into_any_element())
    }
}

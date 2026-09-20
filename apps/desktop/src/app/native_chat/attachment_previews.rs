use super::{appearance::ChatAppearance, images::ChatImageSource, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnimationExt as _, AnyElement, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _, div, img,
    px,
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

    /// The composer's pasted and dropped image thumbnails, with the uploading tile React shows.
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
        let s = p.scale;
        let pending = self.snapshot["pendingAttachments"].as_u64().unwrap_or(0);
        let attachments: Vec<_> = self
            .composer_references
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
        if attachments.is_empty() && pending == 0 {
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
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0 * s))
            .pb(px(8.0 * s));
        let active = self.composer_active_image(cx);
        for (index, (range, path, label)) in attachments.into_iter().enumerate() {
            let removed = range.clone();
            let outlined = active.as_deref() == Some(path.as_str());
            let source = self.chat_image(&images[index], cx);
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
                                    ChatImageSource::Bytes(bytes) => Some(img(bytes)),
                                    ChatImageSource::Uri(url) => Some(img(url)),
                                    _ => None,
                                },
                                |tile, image| {
                                    tile.child(image.size_full().object_fit(gpui::ObjectFit::Cover))
                                },
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_image_viewer(open.clone(), index, cx)
                            })),
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
                            .bg(p.card_background)
                            .hover(|style| style.visible().bg(p.border))
                            .child(
                                gpui::svg()
                                    .path("titlebar/x.svg")
                                    .size(px(9.0 * s))
                                    .text_color(p.muted),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_composer_attachment(removed.clone(), cx)
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
                            .with_animation(
                                "chat-attachment-spinner",
                                gpui::Animation::new(std::time::Duration::from_millis(900))
                                    .repeat(),
                                |svg, delta| {
                                    svg.with_transformation(gpui::Transformation::rotate(
                                        gpui::radians(delta * std::f32::consts::TAU),
                                    ))
                                },
                            ),
                    ),
            );
        }
        Some(row.into_any_element())
    }
}

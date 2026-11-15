//! The `text` block: gx-core's small Markdown subset (`parse_text`) drawn as the sidebar's text,
//! with links that open in the browser and "Show more" past the preview's lines. An editable one
//! (`editable: true`) opens in the sidebar's inline field on a click, and gxserver applies what the
//! user writes and tells the extension (actions.rs).

use ghostex_gx_core::extension_panel::{ExtensionBlockView, TextLineKind, TextSpan};
use ghostex_gx_protocol::extension_panel::PanelBlockBody;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::{h_flex, v_flex};

use super::PanelPlace;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;

impl GhostexGpuiApp {
    pub(super) fn render_native_panel_text(
        &self,
        view: &ExtensionBlockView,
        place: &PanelPlace,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let block_id = view.block.id.clone();
        let editable_text = match &view.block.body {
            PanelBlockBody::Text {
                text,
                editable: Some(true),
            } => Some(text.clone()),
            _ => None,
        };
        if editable_text.is_some() {
            if let Some(editor) = self.native_panel_editor("extensionText", place, &block_id, None)
            {
                return div()
                    .w_full()
                    .px(px(6.0 * scale))
                    .on_action(
                        cx.listener(|app, _: &gpui_component::input::Escape, _, cx| {
                            cx.stop_propagation();
                            app.finish_native_sidebar_rename(false, cx);
                        }),
                    )
                    .child(gpui_component::input::Input::new(&editor.input).h(px(24.0 * scale)))
                    .into_any_element();
            }
        }
        let lines: Vec<AnyElement> = view
            .text_lines
            .iter()
            .enumerate()
            .map(|(line_index, line)| {
                let spans = line.spans.iter().enumerate().map(|(span_index, span)| {
                    render_span(
                        span,
                        format!(
                            "ext-text-{}-{block_id}-{line_index}-{span_index}",
                            place.ui_key()
                        ),
                        appearance,
                        cx,
                    )
                });
                let body = h_flex().flex_1().min_w_0().flex_wrap().children(spans);
                match &line.kind {
                    TextLineKind::Gap => div().h(px(6.0 * scale)).into_any_element(),
                    TextLineKind::Paragraph => body.into_any_element(),
                    TextLineKind::Bullet | TextLineKind::Numbered(_) => h_flex()
                        .w_full()
                        .min_w_0()
                        .items_start()
                        .gap(px(6.0 * scale))
                        .child(
                            div()
                                .flex_shrink_0()
                                .min_w(px(12.0 * scale))
                                .text_color(appearance.muted)
                                .child(match &line.kind {
                                    TextLineKind::Numbered(number) => format!("{number}."),
                                    _ => "•".to_string(),
                                }),
                        )
                        .child(body)
                        .into_any_element(),
                }
            })
            .collect();
        let expand_key = Self::native_panel_text_key(place, &view.block.id);
        let expanded = self
            .native_sidebar
            .extension_panel_ui
            .expanded_texts
            .contains(&expand_key);
        let more = (view.text_truncated || expanded).then(|| {
            div()
                .id(SharedString::from(format!("ext-text-more-{expand_key}")))
                .role(gpui::Role::Button)
                .text_color(appearance.muted)
                .cursor_pointer()
                .hover(|more| more.text_color(appearance.foreground))
                .child(if expanded { "Show less" } else { "Show more" })
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    let texts = &mut app.native_sidebar.extension_panel_ui.expanded_texts;
                    if !texts.remove(&expand_key) {
                        texts.insert(expand_key.clone());
                    }
                    cx.notify();
                }))
        });
        let body = v_flex()
            .w_full()
            .min_w_0()
            .px(px(6.0 * scale))
            .gap(px(2.0 * scale))
            .children(lines)
            .children(more);
        match editable_text {
            Some(text) => {
                let edit_place = place.clone();
                let edit_block = block_id.clone();
                body.id(SharedString::from(format!(
                    "ext-text-edit-{}-{block_id}",
                    place.ui_key()
                )))
                .role(gpui::Role::Button)
                .aria_label("Edit")
                .rounded(px(5.0 * scale))
                .py(px(2.0 * scale))
                .cursor_pointer()
                .hover(|body| body.bg(appearance.hover))
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.begin_native_panel_text_editor(&edit_place, &edit_block, &text, window, cx);
                }))
                .into_any_element()
            }
            None => body.into_any_element(),
        }
    }
}

fn render_span(
    span: &TextSpan,
    id: String,
    appearance: &SidebarAppearance,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) -> AnyElement {
    let scale = appearance.scale;
    let text = div()
        .min_w_0()
        .when(span.bold, |text| text.font_weight(FontWeight::SEMIBOLD))
        .when(span.italic, |text| text.italic())
        .when(span.code, |text| {
            text.px(px(3.0 * scale))
                .rounded(px(3.0 * scale))
                .bg(appearance.hover)
                .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                .text_size(px(11.5 * scale))
        })
        .child(span.text.clone());
    match &span.link {
        Some(url) => {
            let url = url.clone();
            text.id(SharedString::from(id))
                .underline()
                .cursor_pointer()
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.open_url(&url);
                }))
                .into_any_element()
        }
        None => text.into_any_element(),
    }
}

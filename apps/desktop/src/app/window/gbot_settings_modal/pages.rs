//! The Instructions tab: the GBot's standing instructions, its SOUL.md. The Memory tab is
//! memory.rs.
use super::super::native_modal_kit::*;
use super::parts::{notice, page_header};
use super::window::GpuiGbotSettingsModalWindow;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, px};
use gpui_component::v_flex;

impl GpuiGbotSettingsModalWindow {
    pub(super) fn render_instructions(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let mut column = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .pr(px(16.0))
            .gap(px(16.0))
            .child(page_header(
                &p,
                "Instructions",
                "What this GBot always follows, in every chat. Ghostex keeps them in its SOUL.md.",
            ));
        if let Some(message) = &self.original.soul_drift_message {
            column = column.child(notice(&p, "SOUL.md changed outside Ghostex", message));
        }
        column
            .child(
                v_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .line_height(px(20.3))
                    .child(modal_text_area(
                        &self.kit,
                        &self.soul_input,
                        None,
                        self.saving,
                        window,
                        cx,
                    )),
            )
            .into_any_element()
    }
}

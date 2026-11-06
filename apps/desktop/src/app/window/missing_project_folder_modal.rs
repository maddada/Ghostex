//! Native GPUI Missing Project Folder dialog, the desktop twin of the React
//! `MissingProjectFolderModal` in packages/core-ui/missing-project-folder-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:Projects 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in native GPUI one at a time, and each native modal must be EXACTLY 1 to 1 with its React twin: the same layout, copy, colors, radii, spacing, states, keyboard behaviour and bridge messages in both appearances. Locate Folder keeps the dialog open until the sidebar runtime confirms the relocation; only Remove Project and Escape close it.
//! SEE-ALSO: packages/core-ui/missing-project-folder-modal.tsx (deleted 2026-10-01) (the React twin) and the `.missing-project-folder-*` rules in packages/core-ui/styles/modals.css (deleted 2026-11-06), apps/desktop/src/app/missing_project_folder_modal_lifecycle.rs (open, close, sidebar bridge).
use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, Context, FocusHandle, IntoElement, KeyDownEvent, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use gpui_component::v_flex;
use std::rc::Rc;

/// `APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_WIDTH`.
pub(crate) const MISSING_PROJECT_FOLDER_MODAL_WIDTH: f32 = 560.0;
/// First-frame height only (`APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_HEIGHT`); the window is resized to the measured layout on the first prepaint.
pub(crate) const MISSING_PROJECT_FOLDER_MODAL_INITIAL_HEIGHT: f32 = 360.0;

const ICON_TRASH: &str = "modals/missing-project-folder/trash.svg";
const ICON_FOLDER_SEARCH: &str = "modals/missing-project-folder/folder-search.svg";

const TITLE: &str = "Project folder can’t be found";
const NOTE: &str =
    "Locating the moved folder keeps this project’s sessions, groups, actions, and settings.";
const REMOVE_PROJECT: &str = "Remove Project";
const LOCATE_FOLDER: &str = "Locate Folder…";

/// What the dialog asks its host to do. `Locate` leaves the window open; the
/// other two remove it first.
pub(crate) enum MissingProjectFolderModalCommand {
    /// "Locate Folder…": `pickReplacementProjectFolder`; the dialog stays open until the host closes it.
    Locate,
    /// "Remove Project": `removeProject`.
    Remove,
    /// Escape: the React `onCancel`, which only closes the dialog.
    Cancel,
}

pub(crate) type MissingProjectFolderModalHost =
    Rc<dyn Fn(MissingProjectFolderModalCommand, &mut App)>;

pub(crate) struct MissingProjectFolderModalConfig {
    pub(crate) project_name: String,
    pub(crate) project_path: String,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiMissingProjectFolderModalWindow {
    host: MissingProjectFolderModalHost,
    palette: ModalPalette,
    project_name: String,
    project_path: String,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiMissingProjectFolderModalWindow {
    pub(crate) fn new(
        config: MissingProjectFolderModalConfig,
        host: MissingProjectFolderModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: config.palette,
            project_name: config.project_name,
            project_path: config.project_path,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: MissingProjectFolderModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn locate(&mut self, cx: &mut Context<Self>) {
        (self.host)(MissingProjectFolderModalCommand::Locate, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() != "escape" {
            return;
        }
        self.close_window_and_send(MissingProjectFolderModalCommand::Cancel, window, cx);
        cx.stop_propagation();
    }

    /// The one `Card size='sm'` (16px padding): the mono path box and the muted note, 10px apart.
    fn render_body(&self) -> AnyElement {
        let p = self.palette;
        modal_panel(&p)
            .p(px(16.0))
            .gap(px(10.0))
            .child(
                modal_raised_box(&p)
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .px(px(10.0))
                    .py(px(8.0))
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .line_height(px(17.4))
                    .text_color(hsla(p.foreground))
                    .child(self.project_path.clone()),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(19.5))
                    .text_color(hsla(p.muted))
                    .child(NOTE),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        modal_footer(vec![
            modal_action_button(
                &p,
                "missing-project-folder-remove",
                REMOVE_PROJECT,
                Some(modal_icon(ICON_TRASH, 15.0, p.destructive).into_any_element()),
                ModalButtonTone::Danger,
                false,
                |this, window, cx| {
                    this.close_window_and_send(MissingProjectFolderModalCommand::Remove, window, cx)
                },
                cx,
            ),
            modal_action_button(
                &p,
                "missing-project-folder-locate",
                LOCATE_FOLDER,
                Some(modal_icon(ICON_FOLDER_SEARCH, 15.0, p.foreground).into_any_element()),
                ModalButtonTone::Neutral,
                false,
                |this, _window, cx| this.locate(cx),
                cx,
            ),
        ])
    }
}

impl Render for GpuiMissingProjectFolderModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let description = format!(
            "Ghostex can’t start terminals or agents because {}’s folder is missing.",
            self.project_name
        );
        let content = vec![
            modal_header(&p, TITLE, Some(description)),
            v_flex()
                .w_full()
                .gap(px(12.0))
                .child(self.render_body())
                .into_any_element(),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-missing-project-folder-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiMissingProjectFolderModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(MissingProjectFolderModalCommand::Cancel, window, cx);
    }
}

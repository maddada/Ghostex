//! The screenshot editor: opening it on a fresh capture and what happens to the picture after.

mod export;
mod frame;
pub(crate) mod model;
mod view;

use std::path::PathBuf;

use gpui::{
    App, AppContext as _, Context, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, px, size,
};
use image::RgbaImage;

use super::placement::{self, Screen};
use super::platform;
use super::save;
use crate::GhostexGpuiApp;
pub(crate) use model::EditSource;
pub(crate) use view::EditorView;

/// The smallest the editor window can be made.
const MIN_WIDTH: f32 = 520.0;
const MIN_HEIGHT: f32 = 320.0;

pub(crate) struct EditorWindow {
    pub(crate) handle: WindowHandle<EditorView>,
    pub(crate) native: Option<platform::NativeWindow>,
    /// The picture as it was when the editor opened, already saved; sent as is when nothing was
    /// changed.
    pub(crate) original: PathBuf,
    /// The prompt picture being edited again: Enter replaces it instead of adding a new one.
    pub(crate) replacing: Option<u32>,
}

impl GhostexGpuiApp {
    /// Opens the editor on a capture taken from `screen`. `px_per_pt` is the picture's pixels per
    /// point of that screen.
    pub(super) fn open_ghostex_capture_editor(
        &mut self,
        image: RgbaImage,
        original: PathBuf,
        px_per_pt: f32,
        screen: Screen,
        cx: &mut Context<Self>,
    ) {
        let source = EditSource {
            image: std::sync::Arc::new(image),
            marks: Vec::new(),
            px_per_pt,
        };
        self.open_ghostex_capture_editor_on(source, original, None, Some(screen), cx);
    }

    /// Opens the editor where it was last left, at the size it was left at; the first time,
    /// sized to the picture in the middle of `screen` (else the screen under the pointer).
    pub(super) fn open_ghostex_capture_editor_on(
        &mut self,
        source: EditSource,
        original: PathBuf,
        replacing: Option<u32>,
        screen: Option<Screen>,
        cx: &mut Context<Self>,
    ) {
        self.close_ghostex_capture_editor(cx);
        let Some(screen) =
            screen.or_else(|| placement::pointer_screen(self.ghostex_capture_scale(), cx))
        else {
            return;
        };
        let visible = screen.visible;
        let shown_width = source.image.width() as f32 / source.px_per_pt;
        let shown_height = source.image.height() as f32 / source.px_per_pt;
        let width = (shown_width + 36.0)
            .max(720.0)
            .min(f32::from(visible.size.width) * 0.86);
        let height = (shown_height + view::TOOLBAR_HEIGHT + 36.0)
            .max(420.0)
            .min(f32::from(visible.size.height) * 0.86);
        let extent = size(px(width), px(height));
        let frame = self
            .ghostex_capture
            .saved
            .editor_frame
            .as_ref()
            .and_then(|saved| placement::restore_frame(saved, extent, cx))
            .unwrap_or_else(|| placement::centered(&screen, extent));
        let reopened = replacing.is_some();
        let app = cx.weak_entity();
        App::defer(cx, move |cx| {
            let owner = app.clone();
            let (bounds, display_id) = crate::app::window::popup_frame::place_global(frame, cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    titlebar: None,
                    focus: true,
                    show: true,
                    kind: if cfg!(target_os = "linux") {
                        crate::app::window::popup_frame::child_window_kind()
                    } else {
                        WindowKind::PopUp
                    },
                    window_decorations: crate::app::window::popup_frame::child_window_decorations(),
                    is_movable: true,
                    is_resizable: true,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                move |window, cx| {
                    crate::app::window::popup_frame::round_popup_window(window, px(12.0), false);
                    let view = cx.new(|cx| EditorView::new(owner, source, reopened, cx));
                    let focus = view.read(cx).focus.clone();
                    focus.focus(window, cx);
                    view
                },
            );
            let Some(app) = app.upgrade() else {
                return;
            };
            let Ok(handle) = result else {
                return;
            };
            let native = handle
                .update(cx, |_, window, _| {
                    crate::app::helpers::cef_parent_native_view(window)
                        .ok()
                        .map(|view| view as usize)
                })
                .ok()
                .flatten();
            if let Some(native) = native {
                platform::prepare_floating_window(native, true);
                platform::make_resizable(native, size(px(MIN_WIDTH), px(MIN_HEIGHT)));
            }
            app.update(cx, |app, cx| {
                app.ghostex_capture.editor = Some(EditorWindow {
                    handle,
                    native,
                    original,
                    replacing,
                });
                cx.notify();
            });
        });
    }

    /// Sends the picture in an open editor to the prompt box, as Enter would.
    pub(super) fn commit_open_ghostex_capture_editor(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self
            .ghostex_capture
            .editor
            .as_ref()
            .map(|editor| editor.handle)
        else {
            return;
        };
        let app = cx.weak_entity();
        App::defer(cx, move |cx| {
            let Ok((image, edited, source)) = handle.update(cx, |view, _, _| view.take_result())
            else {
                return;
            };
            let _ = app.update(cx, |app, cx| {
                app.ghostex_capture_editor_done(image, edited, source, cx)
            });
        });
    }

    pub(crate) fn close_ghostex_capture_editor(&mut self, cx: &mut Context<Self>) {
        self.remember_ghostex_capture_editor_frame(cx);
        if let Some(editor) = self.ghostex_capture.editor.take() {
            let handle = editor.handle;
            App::defer(cx, move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    /// The picture for the prompt: the original file when nothing changed, else an edited copy.
    fn ghostex_capture_editor_file(&mut self, image: &RgbaImage, edited: bool) -> Option<PathBuf> {
        let original = self.ghostex_capture.editor.as_ref()?.original.clone();
        if !edited {
            return Some(original);
        }
        match save::save_png(image, true) {
            Ok(path) => Some(path),
            Err(_) => Some(original),
        }
    }

    pub(super) fn ghostex_capture_editor_done(
        &mut self,
        image: RgbaImage,
        edited: bool,
        source: EditSource,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.ghostex_capture_editor_file(&image, edited) else {
            return;
        };
        let replacing = self
            .ghostex_capture
            .editor
            .as_ref()
            .and_then(|editor| editor.replacing);
        self.close_ghostex_capture_editor(cx);
        match replacing {
            Some(number) => {
                self.replace_ghostex_capture_attachment(number, path, image, source, cx)
            }
            None => self.add_ghostex_capture_attachment(path, image, source, cx),
        }
    }

    pub(super) fn ghostex_capture_editor_copy(
        &mut self,
        image: RgbaImage,
        edited: bool,
        cx: &mut Context<Self>,
    ) {
        let _ = self.ghostex_capture_editor_file(&image, edited);
        let mut png = Vec::new();
        if image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .is_err()
        {
            crate::app::helpers::gpui_copy_failed_feedback("Couldn't copy the image", cx);
            return;
        }
        crate::app::helpers::gpui_copy_picture_to_clipboard(
            &gpui::Image::from_bytes(gpui::ImageFormat::Png, png),
            cx,
        );
    }
}

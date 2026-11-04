//! The floating prompt box: text, the screenshots taken for it, and where it goes.
//!
//! CDXC:GhostexCapture 2026-09-30 DECISION:
//! User: "I want to be able to prompt quickly into the app without having to switch back to
//! ghostex": a floating prompt box where screenshots land as thumbnails, more text and more
//! screenshots can be added (the capture buttons or the hotkeys), and Enter sends it to the project
//! or session picked. The editor is a separate window that hands its picture here (layout B).
//! Closing the box keeps the draft for next time.
//!
//! CDXC:GhostexCapture 2026-10-22 DECISION:
//! User: "make it show the selection of where to send the prompt as a dropdown and make it default to the currently active session in the actual window that i last interacted with (last session that became active in the app)" (Q380 "Last active session", Q381 "Yes, same list"). Send to is a dropdown holding the same collapsed, filterable project list; a session picked in it stays the target for this box. Supersedes the 2026-09-30 "a new session by default" (the default is in `targets.rs`).

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, ImageSource, KeyDownEvent, MouseDownEvent,
    RenderImage, SharedString, Subscription, WeakEntity, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, anchored, canvas, deferred, div, img,
    point, px, rgb, rgba, size,
};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use image::RgbaImage;

use super::model::CaptureAction;
use super::persistence;
use super::placement;
use super::platform;
use super::project_list::{ListState, Shown};
use super::targets::Target;
use crate::GhostexGpuiApp;

const PROMPT_WIDTH: f32 = 520.0;
const PROMPT_HEIGHT: f32 = 236.0;
const PROMPT_THUMBS_HEIGHT: f32 = 66.0;
const PICKER_HEIGHT: f32 = 300.0;
/// Where the Send to dropdown hangs from when its trigger has not been laid out yet: the bottom of
/// the box's header (10px padding, 26px header, 4px gap).
const PICKER_TOP: f32 = 40.0;

pub(crate) struct Attachment {
    pub(crate) number: u32,
    pub(crate) path: PathBuf,
    pub(crate) thumb: Arc<RenderImage>,
    /// What the editor needs to open the picture again.
    pub(crate) source: super::editor::EditSource,
}

pub(crate) struct PromptWindow {
    pub(crate) handle: WindowHandle<gpui_component::Root>,
    pub(crate) view: Entity<PromptView>,
    pub(crate) native: Option<platform::NativeWindow>,
    pub(crate) scale: f32,
    pub(crate) frame: Bounds<gpui::Pixels>,
    /// Whether the box had the keyboard when a capture hid it, so it gets it back.
    pub(crate) had_keyboard: bool,
}

#[derive(Default)]
pub(crate) struct PromptState {
    pub(crate) window: Option<PromptWindow>,
    pub(crate) opening: bool,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) next_number: u32,
    pub(crate) target: Option<Target>,
    pub(crate) picker_open: bool,
    /// The text as typed, kept in step with the field so it survives closing the box.
    pub(crate) draft: String,
    pub(crate) sending: bool,
    /// The Ghostex draft session the unsent text is kept in, once there is one.
    pub(crate) draft_session: Option<super::drafts::DraftSession>,
    /// Bumped by every fresh prompt, so a draft session that finishes being made afterwards does
    /// not attach itself to the new prompt.
    pub(crate) generation: u64,
}

fn thumbnail(image: &RgbaImage) -> Arc<RenderImage> {
    let mut small = image::imageops::thumbnail(image, 168, 108);
    for pixel in small.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(small)]))
}

impl GhostexGpuiApp {
    /// The box itself, without the Send to dropdown.
    fn ghostex_capture_prompt_box_height(&self) -> f32 {
        PROMPT_HEIGHT
            + if self.ghostex_capture.prompt.attachments.is_empty() {
                0.0
            } else {
                PROMPT_THUMBS_HEIGHT
            }
    }

    /// The window: the box, and room below it for the open Send to dropdown to hang into.
    fn ghostex_capture_prompt_height(&self) -> f32 {
        let height = self.ghostex_capture_prompt_box_height();
        if self.ghostex_capture.prompt.picker_open {
            height.max(PICKER_TOP + PICKER_HEIGHT + 8.0)
        } else {
            height
        }
    }

    /// Where the box opens: where it was last left, else the middle of the screen the pointer is
    /// on.
    fn ghostex_capture_prompt_frame(&self, cx: &App) -> Option<Bounds<gpui::Pixels>> {
        let extent = size(px(PROMPT_WIDTH), px(self.ghostex_capture_prompt_height()));
        if let Some(frame) = self
            .ghostex_capture
            .saved
            .prompt_position
            .as_ref()
            .and_then(|saved| placement::restore_frame(saved, extent, cx))
        {
            return Some(frame);
        }
        let screen = placement::pointer_screen(self.ghostex_capture_scale(), cx)?;
        Some(placement::centered(&screen, extent))
    }

    /// Saves where the box is, so the next one opens there.
    fn remember_ghostex_capture_prompt_position(&mut self, cx: &App) {
        let Some(frame) = self
            .ghostex_capture
            .prompt
            .window
            .as_ref()
            .map(|window| window.frame)
        else {
            return;
        };
        let Some(position) = placement::save_frame(frame, false, cx) else {
            return;
        };
        let saved = &mut self.ghostex_capture.saved;
        if saved.prompt_position.as_ref() == Some(&position) {
            return;
        }
        saved.prompt_position = Some(position);
        persistence::save(saved);
    }

    pub(super) fn open_ghostex_capture_prompt(&mut self, cx: &mut Context<Self>) {
        if self.ghostex_capture.prompt.target.is_none() {
            self.ghostex_capture.prompt.target = self.ghostex_capture_default_target();
        }
        if let Some(window) = self.ghostex_capture.prompt.window.as_ref() {
            // Already open: Cmd/Alt+Ctrl+Shift+T only gives it the keyboard.
            let (handle, view) = (window.handle, window.view.clone());
            if let Some(native) = window.native {
                platform::focus_window(native);
            }
            App::defer(cx, move |cx| {
                let _ = handle.update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| view.focus_input(window, cx))
                });
            });
            return;
        }
        if self.ghostex_capture.prompt.opening {
            return;
        }
        let Some(frame) = self.ghostex_capture_prompt_frame(cx) else {
            return;
        };
        self.ghostex_capture.prompt.opening = true;
        let draft = self.ghostex_capture.prompt.draft.clone();
        let app = cx.weak_entity();
        App::defer(cx, move |cx| {
            let owner = app.clone();
            let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<PromptView>>>> =
                Default::default();
            let slot_in = slot.clone();
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
                    // CDXC:GhostexCapture 2026-09-30 DECISION: User: "please allow me to move the prompt and screenshot floating windows" (dragged by the empty space in their top bar).
                    is_movable: true,
                    is_resizable: false,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                move |window, cx| {
                    crate::app::window::popup_frame::round_popup_window(window, px(14.0), false);
                    let view = cx.new(|cx| PromptView::new(owner, draft, window, cx));
                    view.update(cx, |view, cx| view.focus_input(window, cx));
                    *slot_in.borrow_mut() = Some(view.clone());
                    // Text inputs need gpui-component's Root in their window.
                    cx.new(|cx| {
                        gpui_component::Root::new(view, window, cx).bg(gpui::transparent_black())
                    })
                },
            );
            let Some(app) = app.upgrade() else {
                return;
            };
            let view = slot.borrow_mut().take();
            let (Ok(handle), Some(view)) = (result, view) else {
                app.update(cx, |app, _| app.ghostex_capture.prompt.opening = false);
                return;
            };
            let found = handle
                .update(cx, |_, window, _| {
                    let native = crate::app::helpers::cef_parent_native_view(window)
                        .ok()
                        .map(|view| view as usize);
                    (native, window.scale_factor())
                })
                .ok();
            let Some((native, scale)) = found else {
                app.update(cx, |app, _| app.ghostex_capture.prompt.opening = false);
                return;
            };
            if let Some(native) = native {
                platform::prepare_floating_window(native, true);
            }
            app.update(cx, |app, cx| {
                app.ghostex_capture.prompt.opening = false;
                app.ghostex_capture.prompt.window = Some(PromptWindow {
                    handle,
                    view,
                    native,
                    scale,
                    frame,
                    had_keyboard: false,
                });
                cx.notify();
            });
        });
    }

    pub(crate) fn close_ghostex_capture_prompt(&mut self, cx: &mut Context<Self>) {
        self.ghostex_capture.prompt.picker_open = false;
        if self.ghostex_capture.prompt.window.is_some() {
            self.save_ghostex_capture_draft(false, cx);
            self.sync_ghostex_capture_prompt_origin(cx);
            self.remember_ghostex_capture_prompt_position(cx);
        }
        if let Some(window) = self.ghostex_capture.prompt.window.take() {
            let handle = window.handle;
            App::defer(cx, move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    /// Takes in where the box is now: the user may have dragged it away from where it opened.
    /// While a capture has it hidden, the spot it comes back to is already kept.
    fn sync_ghostex_capture_prompt_origin(&mut self, cx: &mut App) {
        if self.ghostex_capture.capturing {
            return;
        }
        if let Some(origin) = self.ghostex_capture_prompt_live_origin(cx)
            && let Some(window) = self.ghostex_capture.prompt.window.as_mut()
        {
            window.frame.origin = origin;
        }
    }

    /// Where the box is on screen right now, in the space its frame is set in.
    fn ghostex_capture_prompt_live_origin(
        &self,
        cx: &mut App,
    ) -> Option<gpui::Point<gpui::Pixels>> {
        let window = self.ghostex_capture.prompt.window.as_ref()?;
        placement::live_frame(window.handle, window.native, cx).map(|frame| frame.origin)
    }

    /// Gives the box the height its content needs now, where it is: it moves only when the new
    /// height would run off its screen, and only as far as it must.
    fn resize_ghostex_capture_prompt(&mut self, cx: &mut Context<Self>) {
        self.sync_ghostex_capture_prompt_origin(cx);
        let height = px(self.ghostex_capture_prompt_height());
        let Some(current) = self
            .ghostex_capture
            .prompt
            .window
            .as_ref()
            .map(|window| window.frame)
        else {
            return;
        };
        let mut frame = Bounds::new(current.origin, size(px(PROMPT_WIDTH), height));
        if let Some(screen) = placement::screen_at(current.center(), cx) {
            frame = placement::fit_on_screen(&screen, frame);
        }
        let hidden = self.ghostex_capture.capturing;
        let Some(window) = self.ghostex_capture.prompt.window.as_mut() else {
            return;
        };
        if window.frame == frame {
            return;
        }
        window.frame = frame;
        if hidden {
            // `show_ghostex_capture_prompt_window` puts it there when the capture ends.
            return;
        }
        if let Some(native) = window.native {
            platform::set_window_frame(native, frame, window.scale);
        }
        let handle = window.handle;
        App::defer(cx, move |cx| {
            let _ = handle.update(cx, |_, window, cx| window.bounds_changed(cx));
        });
    }

    /// Hides the box for a capture. Called with `capturing` already set, so it reads where the
    /// box is itself: that is where it comes back to.
    ///
    /// CDXC:GhostexCapture 2026-10-22 DECISION:
    /// User: "when the send to prompt box is shown then i try to capture we move it off screen (top left) but it's not all the way off screen, would be better to just hide it then bring it back". macOS keeps part of a titled window on screen whatever frame it is given, so a strip with the box's buttons showed at the screen's left edge and in the picture. The box is hidden instead and shown again where it was, with the keyboard if it had it.
    pub(super) fn hide_ghostex_capture_prompt_window(&mut self, cx: &mut App) {
        if let Some(origin) = self.ghostex_capture_prompt_live_origin(cx)
            && let Some(window) = self.ghostex_capture.prompt.window.as_mut()
        {
            window.frame.origin = origin;
        }
        self.remember_ghostex_capture_prompt_position(cx);
        if let Some(window) = self.ghostex_capture.prompt.window.as_mut()
            && let Some(native) = window.native
        {
            // A second capture started while hidden (Space in the area picker) keeps the first
            // answer: the box is not key while hidden.
            let had_keyboard = platform::hide_window(native);
            window.had_keyboard |= had_keyboard;
        }
    }

    pub(super) fn show_ghostex_capture_prompt_window(&mut self) {
        if let Some(window) = self.ghostex_capture.prompt.window.as_mut()
            && let Some(native) = window.native
        {
            platform::set_window_frame(native, window.frame, window.scale);
            platform::show_window(native, std::mem::take(&mut window.had_keyboard));
        }
    }

    /// A finished picture from the editor: a thumbnail in the box and `[Image #N]` in the text.
    pub(super) fn add_ghostex_capture_attachment(
        &mut self,
        path: PathBuf,
        image: RgbaImage,
        source: super::editor::EditSource,
        cx: &mut Context<Self>,
    ) {
        let prompt = &mut self.ghostex_capture.prompt;
        prompt.next_number += 1;
        let number = prompt.next_number;
        prompt.attachments.push(Attachment {
            number,
            path,
            thumb: thumbnail(&image),
            source,
        });
        let token = format!("[Image #{number}] ");
        match prompt.window.as_ref() {
            Some(window) => {
                let (handle, view) = (window.handle, window.view.clone());
                App::defer(cx, move |cx| {
                    let _ = handle.update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| view.insert(&token, window, cx))
                    });
                });
                self.resize_ghostex_capture_prompt(cx);
            }
            None => {
                if !prompt.draft.is_empty() && !prompt.draft.ends_with(char::is_whitespace) {
                    prompt.draft.push(' ');
                }
                prompt.draft.push_str(&token);
                self.open_ghostex_capture_prompt(cx);
            }
        }
        cx.notify();
    }

    /// A picture edited again: it keeps its number and its place in the text.
    pub(super) fn replace_ghostex_capture_attachment(
        &mut self,
        number: u32,
        path: PathBuf,
        image: RgbaImage,
        source: super::editor::EditSource,
        cx: &mut Context<Self>,
    ) {
        let Some(attachment) = self
            .ghostex_capture
            .prompt
            .attachments
            .iter_mut()
            .find(|attachment| attachment.number == number)
        else {
            // Removed from the prompt while it was being edited: it comes back as a new picture.
            self.add_ghostex_capture_attachment(path, image, source, cx);
            return;
        };
        attachment.path = path;
        attachment.thumb = thumbnail(&image);
        attachment.source = source;
        if self.ghostex_capture.prompt.window.is_none() {
            self.open_ghostex_capture_prompt(cx);
        }
        cx.notify();
    }

    fn remove_ghostex_capture_attachment(&mut self, number: u32, cx: &mut Context<Self>) {
        let prompt = &mut self.ghostex_capture.prompt;
        prompt
            .attachments
            .retain(|attachment| attachment.number != number);
        let token = format!("[Image #{number}]");
        let cleaned = prompt
            .draft
            .replace(&format!("{token} "), "")
            .replace(&token, "");
        if cleaned != prompt.draft {
            prompt.draft = cleaned.clone();
            if let Some(window) = prompt.window.as_ref() {
                let (handle, view) = (window.handle, window.view.clone());
                App::defer(cx, move |cx| {
                    let _ = handle.update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| view.set_text(&cleaned, window, cx))
                    });
                });
            }
        }
        self.resize_ghostex_capture_prompt(cx);
        cx.notify();
    }

    fn toggle_ghostex_capture_picker(&mut self, cx: &mut Context<Self>) {
        self.ghostex_capture.prompt.picker_open = !self.ghostex_capture.prompt.picker_open;
        self.resize_ghostex_capture_prompt(cx);
        cx.notify();
    }

    fn pick_ghostex_capture_target(&mut self, target: Target, cx: &mut Context<Self>) {
        self.ghostex_capture.prompt.target = Some(target);
        self.ghostex_capture.prompt.picker_open = false;
        self.resize_ghostex_capture_prompt(cx);
        cx.notify();
    }
}

pub(crate) struct PromptView {
    app: WeakEntity<GhostexGpuiApp>,
    input: Entity<TextareaState>,
    /// The Send to list's filter field, and which project it has open.
    picker_filter: Entity<InputState>,
    picker_list: ListState,
    /// What Enter in the filter picks: the first match in the list as last drawn.
    picker_first: Option<Target>,
    /// The Send to trigger as last laid out, in window coordinates: the dropdown hangs below it.
    trigger_bounds: Rc<Cell<Option<Bounds<gpui::Pixels>>>>,
    _subscriptions: Vec<Subscription>,
}

impl PromptView {
    fn new(
        app: WeakEntity<GhostexGpuiApp>,
        draft: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .submit_on_enter(true)
                .placeholder("Say what you want. Screenshots are added as [Image #N].")
                .default_value(draft)
        });
        let mut subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    let text = input.read(cx).value().to_string();
                    this.with_app(cx, move |app, _| app.ghostex_capture.prompt.draft = text);
                }
                InputEvent::PressEnter { shift: false, .. } => {
                    this.with_app(cx, |app, cx| app.send_ghostex_capture_prompt(cx));
                }
                _ => {}
            },
        )];
        let picker_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter projects and sessions"));
        subscriptions.push(cx.subscribe_in(
            &picker_filter,
            window,
            |this: &mut Self, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.picker_list.filter = input.read(cx).value().to_string();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    if let Some(target) = this.picker_first.clone() {
                        this.pick(target, window, cx);
                    }
                }
                _ => {}
            },
        ));
        if let Some(owner) = app.upgrade() {
            subscriptions.push(cx.observe(&owner, |_, _, cx| cx.notify()));
        }
        Self {
            app,
            input,
            picker_filter,
            picker_list: ListState::default(),
            picker_first: None,
            trigger_bounds: Rc::new(Cell::new(None)),
            _subscriptions: subscriptions,
        }
    }

    fn pick(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        self.with_app(cx, move |app, cx| {
            app.pick_ghostex_capture_target(target, cx)
        });
        self.focus_input(window, cx);
    }

    /// Opens or closes the Send to list; it opens with every project collapsed and the keyboard in
    /// its filter.
    fn toggle_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_app(cx, |app, cx| app.toggle_ghostex_capture_picker(cx));
        let open = self
            .app
            .upgrade()
            .is_some_and(|app| app.read(cx).ghostex_capture.prompt.picker_open);
        if open {
            self.picker_list = ListState::default();
            self.picker_filter.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
        } else {
            self.focus_input(window, cx);
        }
    }

    fn with_app(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut GhostexGpuiApp, &mut Context<GhostexGpuiApp>),
    ) {
        if let Some(app) = self.app.upgrade() {
            app.update(cx, f);
        }
    }

    pub(crate) fn focus_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn insert(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.to_string();
        self.input
            .update(cx, |input, cx| input.insert(text, window, cx));
        let value = self.input.read(cx).value().to_string();
        self.with_app(cx, move |app, _| app.ghostex_capture.prompt.draft = value);
        self.focus_input(window, cx);
    }

    fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.to_string();
        self.input
            .update(cx, |input, cx| input.set_value(text, window, cx));
    }

    fn render_picker(
        &mut self,
        projects: Vec<crate::app::gx_store::CaptureTargetProject>,
        current: Option<Target>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filter = self.picker_list.filter.trim().to_lowercase();
        let mut first: Option<Target> = None;
        let mut list = div()
            .id("ghostex-capture-targets")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(6.0))
            .pb(px(6.0));
        let mut any = false;
        for (rank, project) in projects.into_iter().enumerate() {
            let heading = match &project.machine {
                Some(machine) => format!("{} · {machine}", project.title),
                None => project.title.clone(),
            };
            let titles: Vec<&str> = project
                .sessions
                .iter()
                .map(|session| session.title.as_str())
                .collect();
            let open =
                match self
                    .picker_list
                    .shown(rank, &project.workspace_project_id, &heading, &titles)
                {
                    Shown::Hidden => continue,
                    Shown::Collapsed => None,
                    Shown::Open(indices) => Some(indices),
                };
            any = true;
            let project_id = project.workspace_project_id.clone();
            list = list.child(picker_project_row(
                SharedString::from(format!("project-{}", project.workspace_project_id)),
                heading.clone(),
                project.sessions.len(),
                open.is_some(),
                cx.listener(move |this, _, window, cx| {
                    if this.picker_list.toggle(rank, &project_id) {
                        this.picker_filter
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    cx.notify();
                }),
            ));
            let Some(indices) = open else {
                continue;
            };
            let new_session = Target::NewSession {
                project: project.workspace_project_id.clone(),
                title: project.title.clone(),
            };
            let title_matched = filter.is_empty() || heading.to_lowercase().contains(&filter);
            if first.is_none() && title_matched {
                first = Some(new_session.clone());
            }
            list = list.child(target_row(
                SharedString::from(format!("new-{}", project.workspace_project_id)),
                div()
                    .w(px(9.0))
                    .text_color(rgb(0x9a9a9a))
                    .child("+")
                    .into_any_element(),
                "New session".to_string(),
                String::new(),
                current.as_ref() == Some(&new_session),
                cx.listener(move |this, _, window, cx| this.pick(new_session.clone(), window, cx)),
            ));
            for index in indices {
                let Some(session) = project.sessions.get(index) else {
                    continue;
                };
                let target = Target::Session {
                    session: session.sidebar_session_id.clone(),
                    title: session.title.clone(),
                    project_title: project.title.clone(),
                };
                if first.is_none() {
                    first = Some(target.clone());
                }
                let (color, square) = if session.question {
                    (0xf472b6, false)
                } else {
                    match session.activity.as_str() {
                        "working" => (0xc68a06, true),
                        "attention" => (0x0093fe, false),
                        _ => (if session.sleeping { 0x5f5f5f } else { 0xd9d9d9 }, false),
                    }
                };
                let dot = div()
                    .flex_none()
                    .size(px(if square { 8.0 } else { 9.0 }))
                    .bg(rgb(color));
                let dot = if square {
                    dot.rounded(px(1.0))
                } else {
                    dot.rounded_full()
                };
                let selected = current.as_ref() == Some(&target);
                list = list.child(target_row(
                    SharedString::from(format!("session-{}", session.sidebar_session_id)),
                    dot.into_any_element(),
                    session.title.clone(),
                    if session.question {
                        "asking".into()
                    } else {
                        String::new()
                    },
                    selected,
                    cx.listener(move |this, _, window, cx| this.pick(target.clone(), window, cx)),
                ));
            }
        }
        if !any {
            list = list.child(
                div()
                    .py(px(14.0))
                    .flex()
                    .justify_center()
                    .text_size(px(12.5))
                    .text_color(rgb(0x6b6b6b))
                    .child("No matching projects or sessions"),
            );
        }
        self.picker_first = first;
        let trigger = self.trigger_bounds.get();
        let position = trigger
            .map(|bounds| point(px(10.0), bounds.bottom() + px(4.0)))
            .unwrap_or_else(|| point(px(10.0), px(PICKER_TOP)));
        let panel = div()
            .id("ghostex-capture-target-menu")
            .occlude()
            .w(px(PROMPT_WIDTH - 20.0))
            .h(px(PICKER_HEIGHT))
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .bg(rgb(0x1b1b1b))
            .border_1()
            .border_color(rgb(0x3a3a3a))
            .shadow_lg()
            .on_mouse_down_out(
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    // A press on the trigger is the trigger's own toggle.
                    if trigger.is_some_and(|bounds| bounds.contains(&event.position)) {
                        return;
                    }
                    this.toggle_picker(window, cx);
                }),
            )
            .child(
                div().flex_none().p(px(6.0)).child(
                    div()
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(9.0))
                        .rounded(px(7.0))
                        .border_1()
                        .border_color(rgba(0xffffff14))
                        .bg(rgb(0x141414))
                        .child(
                            gpui::svg()
                                .path("titlebar/search.svg")
                                .size(px(13.0))
                                .text_color(rgb(0x6b6b6b)),
                        )
                        .child(
                            Input::new(&self.picker_filter)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .flex_1()
                                .px(px(0.0))
                                .py(px(0.0))
                                .text_size(px(12.5))
                                .text_color(rgb(0xe8e8e8)),
                        ),
                ),
            )
            .child(list);
        deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(4.0))
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }
}

/// A project in the Send to list: a chevron, its name and its session count. A click opens it.
fn picker_project_row(
    id: SharedString,
    title: String,
    sessions: usize,
    open: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(8.0))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .hover(|row| row.bg(rgb(0x242424)))
        .cursor_pointer()
        .child(
            gpui::svg()
                .path(if open {
                    "titlebar/chevron-down.svg"
                } else {
                    "titlebar/chevron-right.svg"
                })
                .size(px(12.0))
                .text_color(rgb(0x9a9a9a)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(0xcfd2d8))
                .child(SharedString::from(title)),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(rgb(0x6b6b6b))
                .child(sessions.to_string()),
        )
        .on_click(on_click)
        .into_any_element()
}

fn target_row(
    id: SharedString,
    marker: AnyElement,
    title: String,
    trailing: String,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .h(px(28.0))
        .pl(px(28.0))
        .pr(px(8.0))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .when(selected, |row| row.bg(rgb(0x2a2a2a)))
        .hover(|row| row.bg(rgb(0x242424)))
        .cursor_pointer()
        .child(marker)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.5))
                .text_color(rgb(0xc9ccd2))
                .child(SharedString::from(title)),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(rgb(0x5f5f5f))
                .child(SharedString::from(trailing)),
        )
        .on_click(on_click)
        .into_any_element()
}

/// The Send to dropdown's trigger: the chosen target and a chevron, outlined while open.
fn dropdown_trigger(
    label: String,
    open: bool,
    bounds: Rc<Cell<Option<Bounds<gpui::Pixels>>>>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id("ghostex-capture-target")
        .relative()
        .flex_shrink(1.0)
        .min_w_0()
        .max_w(px(340.0))
        .h(px(26.0))
        .pl(px(9.0))
        .pr(px(7.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .rounded(px(7.0))
        .bg(rgb(0x1d1d1d))
        .border_1()
        .border_color(if open {
            rgba(0xffffff40)
        } else {
            rgba(0xffffff14)
        })
        .hover(|style| style.bg(rgb(0x232323)))
        .cursor_pointer()
        .child(
            canvas(move |frame, _, _| bounds.set(Some(frame)), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.0))
                .text_color(rgb(0xe8e8e8))
                .child(SharedString::from(label)),
        )
        .child(
            gpui::svg()
                .flex_none()
                .path("titlebar/chevron-down.svg")
                .size(px(12.0))
                .text_color(rgb(0x9a9a9a)),
        )
}

impl Render for PromptView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        let app = app.read(cx);
        let prompt = &app.ghostex_capture.prompt;
        let target = prompt
            .target
            .as_ref()
            .map(Target::label)
            .unwrap_or_else(|| "Pick a project".into());
        let picker_open = prompt.picker_open;
        let box_height = app.ghostex_capture_prompt_box_height();
        let picker_projects = picker_open.then(|| app.ghostex_capture_targets());
        let current = prompt.target.clone();
        let sending = prompt.sending;
        let thumbs: Vec<(u32, Arc<RenderImage>)> = prompt
            .attachments
            .iter()
            .map(|attachment| (attachment.number, attachment.thumb.clone()))
            .collect();
        let picker = picker_projects.map(|projects| self.render_picker(projects, current, cx));
        let header = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0x9a9a9a))
                    .child("Send to"),
            )
            .child(
                dropdown_trigger(target, picker_open, self.trigger_bounds.clone())
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_picker(window, cx))),
            )
            .child(crate::app::render::window_drag_region::window_drag_region(
                div().id("ghostex-capture-prompt-drag").flex_1().h(px(26.0)),
            ))
            .child(
                div()
                    .id("ghostex-capture-prompt-close")
                    .px(px(6.0))
                    .text_size(px(14.0))
                    .text_color(rgb(0x9a9a9a))
                    .cursor_pointer()
                    .child("✕")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.with_app(cx, |app, cx| app.close_ghostex_capture_prompt(cx))
                    })),
            );
        // The text field takes all the height the header, thumbnails and buttons leave.
        let mut field = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(12.0))
            .bg(rgb(0x121212))
            .border_1()
            .border_color(rgba(0xffffff29));
        if !thumbs.is_empty() {
            let mut row = div().flex_none().flex().gap(px(8.0));
            for (number, thumb) in thumbs {
                row = row.child(
                    div()
                        .id(("ghostex-capture-thumb", number as usize))
                        .relative()
                        .cursor_pointer()
                        .tooltip(|window, cx| {
                            super::tooltip::solid_tooltip("Click to edit", window, cx)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.with_app(cx, move |app, cx| {
                                app.reopen_ghostex_capture_attachment(number, cx)
                            })
                        }))
                        .w(px(84.0))
                        .h(px(54.0))
                        .rounded(px(7.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(rgba(0xffffff29))
                        .child(img(ImageSource::Render(thumb)).size_full())
                        .child(
                            div()
                                .absolute()
                                .left(px(4.0))
                                .bottom(px(3.0))
                                .px(px(4.0))
                                .rounded(px(4.0))
                                .bg(rgba(0x000000b3))
                                .text_size(px(10.0))
                                .text_color(rgb(0xe8e8e8))
                                .child(format!("#{number}")),
                        )
                        .child(
                            div()
                                .id(("ghostex-capture-remove", number as usize))
                                .absolute()
                                .right(px(3.0))
                                .top(px(3.0))
                                .size(px(16.0))
                                .rounded_full()
                                .bg(rgba(0x000000b3))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(9.0))
                                .text_color(rgb(0xe8e8e8))
                                .cursor_pointer()
                                .child("✕")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.with_app(cx, move |app, cx| {
                                        app.remove_ghostex_capture_attachment(number, cx)
                                    })
                                })),
                        ),
                );
            }
            field = field.child(row);
        }
        field = field.child(
            div().flex_1().min_h_0().w_full().child(
                Textarea::new(&self.input)
                    .appearance(false)
                    .w_full()
                    .h_full()
                    .text_size(px(13.5))
                    .text_color(rgb(0xe8e8e8)),
            ),
        );
        // CDXC:GhostexCapture 2026-09-30 DECISION: User: "remove hotkeys from area app screen and instead of 3 buttons just have 3 icons instead"; each names itself in a tooltip.
        let capture_button = |id: &'static str, label: &'static str, icon: &'static str, action| {
            div()
                .id(id)
                .size(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .hover(|style| style.bg(rgb(0x262626)))
                .cursor_pointer()
                .tooltip(move |window, cx| super::tooltip::solid_tooltip(label, window, cx))
                .child(
                    gpui::svg()
                        .path(icon)
                        .size(px(17.0))
                        .text_color(rgb(0x9a9a9a)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.with_app(cx, move |app, cx| {
                        app.run_ghostex_capture_action(action, cx)
                    })
                }))
        };
        let footer = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(capture_button(
                "ghostex-capture-more-area",
                "Capture Area",
                "capture/area.svg",
                CaptureAction::Area,
            ))
            .child(capture_button(
                "ghostex-capture-more-app",
                "Capture App",
                "capture/app-window.svg",
                CaptureAction::CurrentApp,
            ))
            .child(capture_button(
                "ghostex-capture-more-screen",
                "Capture Screen",
                "titlebar/device-desktop.svg",
                CaptureAction::FullScreen,
            ))
            .child(div().flex_1())
            .child(
                div()
                    .id("ghostex-capture-send")
                    .h(px(28.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(if sending {
                        rgb(0x3a4a52)
                    } else {
                        rgb(0x86d3f8)
                    })
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(0x0b1a24))
                    .cursor_pointer()
                    .child(if sending { "Sending…" } else { "Send ↵" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.with_app(cx, |app, cx| app.send_ghostex_capture_prompt(cx))
                    })),
            );
        let mut root = div()
            .id("ghostex-capture-prompt")
            .w_full()
            .h(px(box_height))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(14.0))
            .bg(rgb(0x161616))
            .border_1()
            .border_color(rgb(0x3a3a3a))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    let picker_open = this
                        .app
                        .upgrade()
                        .is_some_and(|app| app.read(cx).ghostex_capture.prompt.picker_open);
                    if picker_open {
                        this.toggle_picker(window, cx);
                    } else {
                        this.with_app(cx, |app, cx| app.close_ghostex_capture_prompt(cx));
                    }
                }
            }))
            .child(header);
        if let Some(picker) = picker {
            root = root.child(picker);
        }
        // The window is taller than the box while the dropdown is open; the rest stays clear.
        div()
            .size_full()
            .child(root.child(field).child(footer))
            .into_any_element()
    }
}

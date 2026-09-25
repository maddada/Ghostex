use super::runtime_worker::{ChatRuntimeOutput, ChatRuntimeWorker};
use crate::app::{helpers::*, model::*};
use futures::StreamExt as _;
use gpui::{AppContext as _, Context, Entity, EventEmitter, Focusable as _, Subscription, Window};
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

#[derive(Clone)]
pub(crate) struct NativeChatConfig {
    /// `"local"` for a chat on this computer, the saved machine's settings id otherwise. The Rust
    /// chat host keys the machine's chat socket by it and builds its `remote-<machineId>:` storage
    /// prefix from the same test, so a remote chat reads back the drafts, notices and option pills
    /// the TypeScript brain wrote rather than a local session's.
    pub(crate) machine_id: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) sidebar_session_id: String,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) client_id: String,
    pub(crate) remote: Option<GpuiRemoteGxserverRequestTarget>,
    pub(crate) app: Option<gpui::WeakEntity<crate::GhostexGpuiApp>>,
    pub(crate) parent_native_view: *mut std::ffi::c_void,
    pub(crate) initial_snapshot: Option<Value>,
    pub(crate) initial_presentation: Option<Value>,
}

#[derive(Clone)]
pub(crate) enum NativeChatEvent {
    Broker(Value),
    Host(Value),
    DraftState(bool),
    /// The composer input took GPUI focus by any route (click, handoff, draft insert).
    ComposerFocused,
}

pub(crate) struct NativeChatView {
    pub(crate) config: NativeChatConfig,
    pub(crate) runtime: Option<ChatRuntimeWorker>,
    pub(crate) snapshot: Arc<Value>,
    pub(crate) items: Arc<Vec<Value>>,
    /// The open subagent transcript's items, projected and spliced on their own channel.
    pub(crate) subagent_items: Arc<Vec<Value>>,
    pub(crate) error: Option<String>,
    pub(crate) input: Option<Entity<InputState>>,
    input_subscription: Option<Subscription>,
    input_observer: Option<Subscription>,
    pub(super) suggestions: super::suggestions::SuggestionWindowState,
    pub(super) frosted_overlays: super::frosted_overlay_window::FrostedOverlayWindows,
    pub(super) composer_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    pub(super) suggestion_selection: Option<(String, usize)>,
    input_window: Option<gpui::WindowId>,
    pub(super) option_menu: Option<Entity<super::option_menu::ChatOptionMenu>>,
    /// Which trigger owns the open menu, so pressing it again shuts it (`menu_toggle.rs`).
    pub(super) menu_toggle: super::menu_toggle::ChatMenuToggle,
    pub(super) save_markdown_window: super::save_markdown::SaveMarkdownWindowState,
    pub(super) rewind_window: super::rewind::RewindWindowState,
    pub(super) image_viewer: super::image_viewer::ImageViewerState,
    /// The larger preview of one transcript table (table_preview/).
    pub(super) table_preview: super::table_preview::TablePreviewState,
    /// Bytes for the transcript's pictures, read once and shared by the thumbnails and the viewer.
    pub(super) images: super::images::ChatImageCache,
    /// True while a completed turn's work rows render, which is where answered question cards are suppressed.
    pub(super) in_work_fold: bool,
    /// True while any row of a completed turn renders: its writes belong to that turn's "N files changed" fold.
    pub(super) hide_file_changes: bool,
    /// Every disclosure that is opening, closing or folding, and the state each was last drawn in.
    /// A cell because rows drawn through `&self` report their state too.
    pub(super) disclosure_motion: std::cell::RefCell<super::disclosure_motion::DisclosureMotions>,
    /// True while a row of the subagent viewer's transcript renders, where a rewind would act on the wrong conversation.
    pub(super) in_subagent: bool,
    pub(super) context_editor_window: super::context_editor::ContextEditorWindowState,
    pub(crate) maximized_window: Option<gpui::WindowHandle<gpui_component::Root>>,
    /// Whether this session's composer is maximized, which outlives the window while the pane is off screen.
    pub(super) maximized_wanted: bool,
    /// True between asking for the maximized window and GPUI handing back its handle.
    pub(super) maximized_opening: bool,
    /// True while this session's pane is off screen: its modal windows stay closed, and the state behind them waits for the pane to come back.
    pub(super) pane_hidden: bool,
    pub(crate) main_window: Option<gpui::AnyWindowHandle>,
    /// Where the composer's model pill was last painted, which Option+P opens the model pop-up against.
    pub(super) model_pill_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Set while the next menu this view opens belongs to another surface (the terminal's model pill), not to its own pane.
    pub(super) menu_outside_pane: bool,
    /// A model pop-up asked for before this view had its first snapshot: the trigger and its window.
    pub(super) pending_model_menu: Option<(gpui::Bounds<gpui::Pixels>, gpui::AnyWindowHandle)>,
    pub(crate) window_subscription: Option<Subscription>,
    pub(crate) send_hold_task: Option<gpui::Task<()>>,
    pub(crate) send_hold_fired: bool,
    pub(crate) stop_cooldown_task: Option<gpui::Task<()>>,
    pub(super) context_status_measurements: Option<Value>,
    pub(crate) composer_measurements: Option<Value>,
    pub(super) composer_held_key: Option<String>,
    /// The chat box's height tween and the arrival fade of the controls an expansion brings back.
    pub(super) composer_animation: super::composer_animation::ComposerAnimation,
    /// Whether this session's status line holds its row of space, remembered across view
    /// re-creations so a chat that is going to show one never paints a frame without it.
    pub(super) status_line_reserved: bool,
    pub(crate) bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// The transcript scrollbar's measured track: from the top of the transcript region to the
    /// bottom of the pane, so the composer's height never shortens it (scrollbar.rs).
    pub(super) scrollbar_track: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
    /// When the reader last scrolled the transcript, the only thing that shows its scrollbar.
    /// `instant::Instant` because gpui-component's scrollbar takes that type; it is std's on native and a browser clock on wasm32.
    pub(super) transcript_scrolled_at: Option<instant::Instant>,
    input_needs_sync: bool,
    input_placeholder: String,
    input_undoable: bool,
    pub(super) input_caret: Option<usize>,
    /// The draft's markdown references, and the draft they were parsed from.
    pub(super) composer_references: Vec<super::composer_references::ComposerReference>,
    pub(super) composer_reference_draft: Option<String>,
    pub(super) composer_reference_click: u64,
    pub(super) composer_reference_task: Option<gpui::Task<()>>,
    /// Re-asks the shared parser after a paint found the runtime thread busy.
    pub(super) composer_reference_retry: Option<gpui::Task<()>>,
    /// Path of the image pill under the pointer.
    pub(super) composer_image_hover: Option<String>,
    /// Image the caret last sat against, so a caret move repaints the thumbnails only when it matters.
    pub(super) composer_caret_image: Option<String>,
    pub(super) async_answer_input: Option<(String, Entity<InputState>)>,
    pub(super) async_answer_subscription: Option<Subscription>,
    pub(super) async_answer_echo: super::async_questions::AsyncAnswerEcho,
    pub(crate) answer_input: Option<(String, Entity<InputState>)>,
    pub(crate) answer_subscription: Option<Subscription>,
    pub(super) terminal_dialog_input: Option<super::terminal_dialog::TerminalDialogInput>,
    pub(super) terminal_dialog_key_focus: gpui::FocusHandle,
    pub(crate) note_input: Option<Entity<InputState>>,
    pub(crate) note_subscription: Option<Subscription>,
    /// Transcript search (Cmd+F): the field, and the navigation it last scrolled to.
    pub(super) search_input: Option<Entity<InputState>>,
    pub(super) search_subscription: Option<Subscription>,
    pub(super) search_scrolled_revision: i64,
    /// Open or closed as this pane last asked the shared runtime, held until its snapshot agrees.
    pub(super) search_pending_open: Option<bool>,
    /// Keyboard zoom (Cmd+= / Cmd+- / Cmd+0): this pane's temporary size (zoom.rs).
    pub(super) zoom: super::zoom::ChatZoomState,
    pub(crate) draft: String,
    pub(crate) draft_revision: u64,
    pub(crate) draft_id: String,
    pub(crate) pending_send: bool,
    pub(crate) composer_ready: bool,
    pub(crate) expanded: HashSet<String>,
    /// Scroll offsets of the capped boxes inside transcript rows (nested_scroll.rs).
    pub(super) nested_scrolls: super::nested_scroll::NestedScrolls,
    /// The detail of each row drawn open, keyed like the row (row_details.rs).
    pub(super) row_details: Value,
    /// The details this frame's rows asked for, and the set last sent to the host.
    pub(super) detail_demand: std::collections::BTreeMap<String, Value>,
    pub(super) detail_sent: std::collections::BTreeMap<String, Value>,
    pub(super) detail_sync_scheduled: bool,
    /// Armed Delayed Send / Close After Done labels drawn on the working row, set by the app (session_chat_armed_actions.rs).
    pub(crate) armed_actions: Value,
    pub(crate) collapsed: HashSet<String>,
    /// How each fenced block the reader has touched wraps; the rest follow `code_wrap_default`.
    pub(super) code_wrap: HashMap<String, bool>,
    /// React's remembered last choice (session-chat-code-wrap.ts): the blocks that
    /// scroll into view after a toggle start the way the reader last asked for.
    pub(super) code_wrap_default: bool,
    pub(crate) list: gpui::ListState,
    /// The transcript's own cached view, created on the first draw (transcript_host.rs).
    pub(super) transcript_host: Option<Entity<super::transcript_host::TranscriptHost>>,
    /// The loading hold and the fade that ends it (transcript_reveal.rs).
    pub(super) transcript_reveal: super::transcript_reveal::TranscriptReveal,
    /// The composer tween's bottom inset for the row list, computed once per chat render.
    pub(super) transcript_inset: f32,
    /// The transcript minimap's dashes, hover and measured column (minimap.rs).
    pub(super) minimap: super::minimap::MinimapState,
    /// The subagent viewer's list, kept apart so opening it never disturbs the main transcript's scroll.
    pub(crate) subagent_list: gpui::ListState,
    /// The subagent viewer's own focus, so its Escape works without the composer having been focused first.
    pub(super) subagent_focus: gpui::FocusHandle,
    /// Whether the open viewer has already taken focus, so a redraw does not steal it back every frame.
    pub(super) subagent_focused: bool,
    pub(crate) pane_focused: bool,
    /// Whether this chat's composer field itself holds the keyboard, which is what the `@`, `$` and
    /// `/` picker window keys off (`suggestions/window.rs`).
    pub(super) composer_focused: bool,
    /// Whether the user opened the chat box of a short pane (composer_scroll.rs).
    pub(crate) short_pane_composer_open: bool,
    pub(crate) focus_requested: bool,
    pub(crate) subscriptions: Vec<Subscription>,
    /// When this view last rendered; a parked chat keeps applying frames without redrawing the window.
    pub(crate) last_render: Option<web_time::Instant>,
    last_notified: Option<web_time::Instant>,
    notify_scheduled: bool,
    /// The transcript starts at the window's top edge under the floating work area header, so its
    /// first row reserves the header's height (`set_under_workarea_header`).
    pub(crate) under_workarea_header: bool,
}

impl EventEmitter<NativeChatEvent> for NativeChatView {}

impl NativeChatView {
    pub(crate) fn new(config: NativeChatConfig, cx: &mut Context<Self>) -> Self {
        super::fonts::register(cx);
        super::keyboard::register(cx);
        let runtime = (!config.session_id.is_empty()).then(|| Self::start_runtime(&config, cx));
        let error = None;
        Self::with_runtime(config, runtime, error, cx)
    }

    pub(super) fn start_runtime(
        config: &NativeChatConfig,
        cx: &mut Context<Self>,
    ) -> ChatRuntimeWorker {
        let (wake, mut wakes) = futures::channel::mpsc::unbounded::<()>();
        // A remote chat names its machine's gxserver, which its chat socket connects to; this
        // computer's daemon is the app's to give the chat host (`gx_chat::set_endpoint`).
        let endpoint = config.remote.as_ref().map(|target| {
            json!({"baseUrl": format!("http://127.0.0.1:{}", target.local_port), "authToken": target.token})
        });
        let runtime = ChatRuntimeWorker::start(
            json!({"clientId":config.client_id,"machineId":config.machine_id,"projectId":config.project_id,"sessionId":config.session_id,"initialSnapshot":config.initial_snapshot,"initialPresentation":config.initial_presentation,"endpoint":endpoint}),
            move || {
                let _ = wake.unbounded_send(());
            },
        );
        cx.spawn(async move |this, cx| {
            while wakes.next().await.is_some() {
                if this.update(cx, |this, cx| this.pump(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        runtime
    }

    fn with_runtime(
        config: NativeChatConfig,
        runtime: Option<ChatRuntimeWorker>,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = gpui::ListState::new(0, gpui::ListAlignment::Top, gpui::px(400.0));
        list.set_follow_mode(gpui::FollowMode::Tail);
        let subagent_list = gpui::ListState::new(0, gpui::ListAlignment::Top, gpui::px(400.0));
        subagent_list.set_follow_mode(gpui::FollowMode::Tail);
        let chat = cx.weak_entity();
        // The header's fade is painted by the app, so the app repaints when the list reaches or
        // leaves its top (`transcript_scrolled_to_top`).
        let at_top = std::rc::Rc::new(std::cell::Cell::new(true));
        list.set_scroll_handler(move |_, _, cx| {
            let chat = chat.clone();
            let at_top = at_top.clone();
            cx.defer(move |cx| {
                let _ = chat.update(cx, |chat, cx| {
                    chat.transcript_scrolled_at = Some(instant::Instant::now());
                    if chat.list.is_following_tail() && chat.snapshot["composerCollapsed"] == true {
                        chat.invoke(json!({"type":"composerExpand"}), cx);
                    }
                    chat.load_earlier_if_near_top(cx);
                    let now = chat.transcript_scrolled_to_top();
                    if at_top.replace(now) != now
                        && let Some(app) = chat.config.app.as_ref().and_then(|app| app.upgrade())
                    {
                        app.update(cx, |_, cx| cx.notify());
                    }
                    cx.notify();
                });
            });
        });
        Self {
            draft_id: format!(
                "{}-{}",
                config.client_id,
                SessionChatPageState::next_identity()
            ),
            status_line_reserved: super::context_meter::remembered_status_line_reservation(
                &config.session_id,
            ),
            config,
            runtime,
            error,
            snapshot: Arc::new(json!({
                "composerPlaceholder": ghostex_gx_chat_core::composer::policy::DESKTOP_COMPOSER_PLACEHOLDER,
                "composerActions": {"summary": true, "note": false, "stash": true, "attach": true, "terminal": true},
                "optionLabels": {"showModel": true}
            })),
            items: Arc::default(),
            subagent_items: Arc::default(),
            input: None,
            input_subscription: None,
            input_observer: None,
            suggestions: Default::default(),
            frosted_overlays: Default::default(),
            composer_bounds: Default::default(),
            suggestion_selection: None,
            input_window: None,
            option_menu: None,
            menu_toggle: Default::default(),
            context_editor_window: Default::default(),
            save_markdown_window: Default::default(),
            rewind_window: Default::default(),
            image_viewer: Default::default(),
            table_preview: Default::default(),
            images: Default::default(),
            in_work_fold: false,
            hide_file_changes: false,
            disclosure_motion: Default::default(),
            in_subagent: false,
            maximized_window: None,
            maximized_wanted: false,
            maximized_opening: false,
            pane_hidden: false,
            main_window: None,
            model_pill_bounds: Default::default(),
            menu_outside_pane: false,
            pending_model_menu: None,
            window_subscription: None,
            send_hold_task: None,
            send_hold_fired: false,
            stop_cooldown_task: None,
            composer_measurements: None,
            composer_held_key: None,
            composer_animation: Default::default(),
            context_status_measurements: None,
            bounds: Default::default(),
            scrollbar_track: Default::default(),
            transcript_scrolled_at: None,
            input_needs_sync: false,
            input_placeholder: String::new(),
            input_undoable: false,
            input_caret: None,
            composer_references: Vec::new(),
            composer_reference_draft: None,
            composer_reference_click: 0,
            composer_reference_task: None,
            composer_reference_retry: None,
            composer_image_hover: None,
            composer_caret_image: None,
            async_answer_input: None,
            async_answer_subscription: None,
            async_answer_echo: Default::default(),
            answer_input: None,
            answer_subscription: None,
            terminal_dialog_input: None,
            terminal_dialog_key_focus: cx.focus_handle(),
            note_input: None,
            note_subscription: None,
            search_input: None,
            search_subscription: None,
            search_scrolled_revision: -1,
            search_pending_open: None,
            zoom: Default::default(),
            draft: String::new(),
            draft_revision: 0,
            pending_send: false,
            composer_ready: false,
            expanded: HashSet::new(),
            nested_scrolls: Default::default(),
            row_details: Value::Null,
            detail_demand: Default::default(),
            detail_sent: Default::default(),
            detail_sync_scheduled: false,
            armed_actions: Value::Array(Vec::new()),
            collapsed: HashSet::new(),
            code_wrap: HashMap::new(),
            code_wrap_default: false,
            list,
            transcript_host: None,
            transcript_reveal: Default::default(),
            transcript_inset: 0.0,
            minimap: Default::default(),
            subagent_list,
            subagent_focus: cx.focus_handle(),
            subagent_focused: false,
            pane_focused: false,
            composer_focused: false,
            short_pane_composer_open: false,
            focus_requested: false,
            subscriptions: Vec::new(),
            last_render: None,
            last_notified: None,
            notify_scheduled: false,
            under_workarea_header: false,
        }
    }

    pub(crate) fn ensure_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_none() {
            let draft = self.draft.clone();
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .submit_on_enter(true)
                    .auto_grow(3, 7)
                    .default_value(draft)
            });
            self.input = Some(input);
        }
        let placeholder = self.snapshot["composerPlaceholder"]
            .as_str()
            .unwrap_or_default();
        if self.input_placeholder != placeholder {
            self.input_placeholder = placeholder.to_owned();
            self.input.as_ref().unwrap().update(cx, |input, cx| {
                input.set_placeholder(self.input_placeholder.clone(), window, cx)
            });
        }
        if self.input_window != Some(window.window_handle().window_id()) {
            self.input_window = Some(window.window_handle().window_id());
            let input = self.input.as_ref().unwrap().clone();
            self.input_observer = Some(cx.observe_in(&input, window, |this, input, window, cx| {
                if input.read(cx).focus_handle(cx).is_focused(window) {
                    this.update_suggestion_selection(cx);
                }
                let caret_image = this.composer_caret_image(cx);
                if this.composer_caret_image != caret_image {
                    this.composer_caret_image = caret_image;
                    cx.notify();
                }
            }));
            self.input_subscription = Some(cx.subscribe_in(
                &input,
                window,
                |this, input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        let draft = input.read(cx).value().to_string();
                        if draft == this.draft {
                            return;
                        }
                        this.invoke(json!({"type":"composerExpand","editor":true}), cx);
                        if this.composer_focused {
                            this.short_pane_composer_open = true;
                        }
                        this.draft = draft;
                        this.draft_revision += 1;
                        this.persist_draft(cx);
                        cx.emit(NativeChatEvent::DraftState(this.draft.is_empty()));
                        cx.notify();
                    }
                    InputEvent::Focus => {
                        super::focus::reclaim_keyboard_focus(window);
                        this.composer_focused = true;
                        this.sync_suggestion_window(cx);
                        this.invoke(json!({"type":"composerExpand","editor":true}), cx);
                        cx.emit(NativeChatEvent::ComposerFocused);
                        cx.notify();
                    }
                    InputEvent::Blur => {
                        this.composer_focused = false;
                        this.short_pane_composer_open = false;
                        this.sync_suggestion_window(cx);
                        this.save_draft(cx);
                        // A short pane's box collapses again once it loses focus (composer_scroll.rs).
                        cx.notify();
                    }
                    _ => {}
                },
            ));
        }
        if self.input_needs_sync {
            self.input_needs_sync = false;
            let undoable = std::mem::take(&mut self.input_undoable);
            let caret = self
                .input_caret
                .take()
                .map(|utf16| {
                    let mut offset = 0;
                    self.draft
                        .char_indices()
                        .find_map(|(index, ch)| {
                            if offset >= utf16 {
                                return Some(index);
                            }
                            offset += ch.len_utf16();
                            None
                        })
                        .unwrap_or(self.draft.len())
                })
                .unwrap_or(self.draft.len());
            if let Some(input) = &self.input {
                input.update(cx, |input, cx| {
                    if undoable {
                        input.replace_all(self.draft.clone(), window, cx);
                    } else {
                        input.set_value(self.draft.clone(), window, cx);
                    }
                    input.set_selected_range(caret..caret, cx);
                });
            }
        }
        if self.focus_requested {
            self.focus_requested = false;
            if let Some(input) = &self.input {
                input.read(cx).focus_handle(cx).focus(window, cx);
            }
        }
    }

    pub(crate) fn invoke(&mut self, action: Value, cx: &mut Context<Self>) {
        if let Some(runtime) = &self.runtime {
            runtime.call("action", vec![action]);
        }
        let _ = cx;
    }

    pub(crate) fn receive_callback(
        &mut self,
        callback: &str,
        payload: &Value,
        cx: &mut Context<Self>,
    ) {
        if callback == "onSessionChatAttachmentsPicked" {
            self.invoke(json!({"type":"attachPaths","paths":payload["paths"]}), cx);
        }
    }

    pub(crate) fn insert_prompt(&mut self, content: &str, cx: &mut Context<Self>) {
        self.replace_draft(content, false, false, cx);
    }

    fn replace_draft(
        &mut self,
        content: &str,
        history: bool,
        preserve_error: bool,
        cx: &mut Context<Self>,
    ) {
        self.draft = content.to_string();
        self.draft_revision += 1;
        self.input_needs_sync = true;
        self.input_undoable = true;
        self.focus_requested = true;
        if self.composer_ready {
            self.invoke(json!({"type":"editDraft", "text":self.draft, "draftVersion":{"draftId":self.draft_id,"revision":self.draft_revision.max(1)},"history":history,"preserveError":preserve_error}),cx);
        }
        if preserve_error && self.composer_ready {
            self.invoke(json!({"type":"saveDraft","content":self.draft,"draftVersion":{"draftId":self.draft_id,"revision":self.draft_revision},"preserveError":true}),cx);
        } else {
            self.save_draft(cx);
        }
        cx.notify();
    }

    pub(crate) fn persist_draft(&mut self, cx: &mut Context<Self>) {
        if self.composer_ready {
            self.invoke(json!({"type":"editDraft", "text":self.draft, "draftVersion":{"draftId":self.draft_id,"revision":self.draft_revision.max(1)}}), cx);
        }
    }

    pub(crate) fn save_draft(&mut self, cx: &mut Context<Self>) {
        if !self.composer_ready {
            return;
        }
        self.invoke(json!({"type":"saveDraft","content":self.draft,"draftVersion":{"draftId":self.draft_id,"revision":self.draft_revision}}), cx);
    }

    pub(crate) fn host(&self, action: &str, fields: Value, cx: &mut Context<Self>) {
        let mut message = fields.as_object().cloned().unwrap_or_default();
        message.insert("type".into(), "sessionChatHostAction".into());
        message.insert("action".into(), action.into());
        cx.emit(NativeChatEvent::Host(Value::Object(message)));
    }

    /// Apply everything the runtime thread has drained since the last wake.
    pub(crate) fn pump(&mut self, cx: &mut Context<Self>) {
        let Some(runtime) = &self.runtime else {
            return;
        };
        for output in runtime.take_outputs() {
            match output {
                ChatRuntimeOutput::Drained(output) => self.apply_output(output, cx),
                ChatRuntimeOutput::Error(error) => {
                    self.error = Some(error);
                    cx.notify();
                }
            }
        }
    }

    /// CDXC:SessionChat 2026-09-18 WHY:
    /// Retained chat views stay subscribed while parked, and every state frame (several a second across working sessions) notified, which redraws the whole window for a view nobody sees.
    /// The state is applied either way; only a view that rendered recently asks for a redraw, and a parked one paints the latest state when it comes back.
    /// CDXC:SessionChat 2026-09-19 WHY:
    /// A streaming agent produced a runtime output several times a frame, and each one redrew the whole window, which re-lays out every visible transcript row; with a few agents streaming that was most of the UI thread.
    /// Redraws from runtime output are coalesced to one per 50ms; the last output in a burst still paints, only never sooner than that.
    /// CDXC:SessionChat 2026-09-24 WHY:
    /// The model pop-up opened from terminal view belongs to a hidden chat view and repaints only by observing it, so an open pop-up counts as shown; otherwise its tab and search changes reached the runtime while the pop-up kept painting the state it opened with. Supersedes the same rule for the retired full-screen picker.
    fn notify_if_shown(&mut self, cx: &mut Context<Self>) {
        const NOTIFY_MIN_INTERVAL: Duration = Duration::from_millis(50);
        if self.option_menu.is_none()
            && !self
                .last_render
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        let now = web_time::Instant::now();
        if let Some(at) = self.last_notified
            && now.duration_since(at) < NOTIFY_MIN_INTERVAL
        {
            if !self.notify_scheduled {
                self.notify_scheduled = true;
                let wait = NOTIFY_MIN_INTERVAL - now.duration_since(at);
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    let _ = this.update(cx, |this, cx| {
                        this.notify_scheduled = false;
                        this.last_notified = Some(web_time::Instant::now());
                        cx.notify();
                    });
                })
                .detach();
            }
            return;
        }
        self.last_notified = Some(now);
        cx.notify();
    }

    fn apply_output(&mut self, mut output: Value, cx: &mut Context<Self>) {
        if let Some(mut splice) = output
            .as_object_mut()
            .and_then(|output| output.remove("itemsSplice"))
            .filter(Value::is_object)
        {
            // The host ships only the changed window of transcript items (see transcriptItemsSplice in native-host.ts).
            let inserted = splice
                .get_mut("items")
                .and_then(Value::as_array_mut)
                .map(std::mem::take)
                .unwrap_or_default();
            let items = Arc::make_mut(&mut self.items);
            let start = (splice["start"].as_u64().unwrap_or(0) as usize).min(items.len());
            let end = start
                .saturating_add(splice["deleteCount"].as_u64().unwrap_or(0) as usize)
                .min(items.len());
            let inserted_len = inserted.len();
            items.splice(start..end, inserted);
            self.splice_transcript(start, end, inserted_len);
            self.notify_if_shown(cx);
        }
        if let Some(markers) = output
            .as_object_mut()
            .and_then(|output| output.remove("minimap"))
        {
            self.minimap.adopt(&markers);
            self.notify_if_shown(cx);
        }
        if let Some(mut splice) = output
            .as_object_mut()
            .and_then(|output| output.remove("subagentSplice"))
            .filter(Value::is_object)
        {
            self.apply_subagent_splice(&mut splice);
            self.notify_if_shown(cx);
        }
        if let Some(details) = output
            .as_object_mut()
            .and_then(|output| output.remove("rowDetails"))
        {
            self.row_details = details;
            self.list.remeasure();
            self.subagent_list.remeasure();
            self.notify_if_shown(cx);
        }
        if let Some(mut snapshot) = output
            .as_object_mut()
            .and_then(|output| output.remove("snapshot"))
        {
            // The pane's keyboard zoom is its own, and outlives the snapshots the host publishes.
            self.apply_chat_zoom(&mut snapshot);
            self.retain_launch_welcome(&mut snapshot);
            if let Some(measured) = self
                .composer_measurements
                .as_ref()
                .and_then(|value| serde_json::from_value(value.clone()).ok())
            {
                snapshot["composerOverflow"] =
                    json!(ghostex_gx_chat_core::composer::layout::fit_composer_controls(&measured));
            }
            self.snapshot = Arc::new(snapshot);
            self.adopt_status_line_reservation();
            self.open_pending_model_menu(cx);
            self.sync_context_editor_window(cx);
            self.sync_save_markdown_window(cx);
            self.sync_rewind_window(cx);
            self.sync_suggestion_window(cx);
            self.load_earlier_if_near_top(cx);
            self.notify_if_shown(cx);
        }
        for request in output["requests"].as_array().into_iter().flatten() {
            match request["kind"].as_str() {
                Some("rpc") => self.rpc(request.clone(), cx),
                Some("broker") => cx.emit(NativeChatEvent::Broker(request.clone())),
                Some("composerClearExpected") => {
                    if request["params"]["text"].as_str() == Some(self.draft.as_str()) { self.replace_draft("",false,false,cx); }
                }
                Some("returnedPrompt") => self.invoke(json!({"type":"applyReturned","text":request["params"]["text"],"current":self.draft}),cx),
                Some("composerInit") => {
                    let local_draft = (self.draft_revision > 0).then(|| self.draft.clone());
                    let entry = &request["params"]["entry"];
                    self.config.client_id = request["params"]["clientId"].as_str().unwrap_or_default().to_string();
                    self.draft_id = entry["version"]["draftId"].as_str().unwrap_or_default().to_string();
                    self.draft_revision = entry["version"]["revision"].as_u64().unwrap_or(1);
                    let restored = if entry["parked"] == true || entry["submitted"] == true { String::new() } else { entry["text"].as_str().unwrap_or_default().to_string() };
                    let draft = match &local_draft {
                        Some(local) => format!("{restored}{local}"),
                        None => restored,
                    };
                    self.input_needs_sync = self.draft != draft;
                    self.draft = draft;
                    self.composer_ready = true;
                    self.composer_measurements = None;
                    if local_draft.is_some() {
                        self.draft_revision += 1;
                        self.persist_draft(cx);
                        self.save_draft(cx);
                    }
                    self.host("composerReady", json!({}), cx);
                    // The stash badge and the session-note dot need their first read (native-composer-chrome.ts).
                    self.invoke(json!({"type":"refreshComposerChrome","sessionId":self.config.session_id}), cx);
                    cx.emit(NativeChatEvent::DraftState(self.draft.is_empty()));
                    cx.notify();
                }
                Some("draftSubmitted") => {
                    self.pending_send = false;
                    if request["method"] == "handoff" && request["params"]["version"]["draftId"].as_str() == Some(&self.draft_id) && request["params"]["version"]["revision"].as_u64() == Some(self.draft_revision) {
                        self.draft.clear();
                        self.draft_id = request["params"]["nextVersion"]["draftId"].as_str().unwrap_or_default().to_string();
                        self.draft_revision = 1;
                        self.input_needs_sync = true;
                        self.focus_requested = true;
                        cx.emit(NativeChatEvent::DraftState(true));
                    }
                    cx.notify();
                }
                Some("submissionFailed") => {
                    self.pending_send = false;
                    self.invoke(json!({"type":"restoreSubmission","text":request["params"]["text"],"current":self.draft}),cx);
                }
                Some("draftReceived") => {
                    if request["params"]["previous"].as_str() == Some(self.draft.as_str()) {
                        self.draft = request["params"]["content"].as_str().unwrap_or_default().to_owned();
                        self.draft_id = request["params"]["version"]["draftId"].as_str().unwrap_or_default().to_owned();
                        self.draft_revision = request["params"]["version"]["revision"].as_u64().unwrap_or(1);
                        self.input_needs_sync = true;
                        self.focus_requested = true;
                        cx.emit(NativeChatEvent::DraftState(self.draft.is_empty()));
                    }
                    cx.notify();
                }
                Some("attachmentReferences") => {
                    let selection = self.input.as_ref().map(|input|input.read(cx).selected_range()).unwrap_or(self.draft.len()..self.draft.len());
                    let start = self.draft[..selection.start].encode_utf16().count();
                    let end = self.draft[..selection.end].encode_utf16().count();
                    self.invoke(json!({"type":"insertAttachments","paths":request["params"]["paths"],"text":self.draft,"start":start,"end":end}), cx);
                }
                Some("markdownSaved") => {
                    let path = request["params"]["path"].as_str().unwrap_or_default().to_string();
                    crate::app::helpers::gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(path.clone()), cx);
                    if let Some(main) = self.main_window {
                        cx.defer(move |cx| {
                            let _ = main.update(cx, |_, window, cx| {
                                use gpui_component::WindowExt as _;
                                window.push_notification(gpui_component::notification::Notification::success(format!("Saved to Markdown\n{path} was copied to the clipboard.")), cx);
                            });
                        });
                    }
                }
                Some("host") => self.host(request["method"].as_str().unwrap_or_default(), request["params"].clone(), cx),
                Some("chatImage") => self.receive_chat_image(request, cx),
                // The two arms the Rust brain's `Effect::Copy` and `Effect::Toast` ride in. The
                // TypeScript brain the web build still runs pushes neither (a clipboard write only
                // ever reaches the view inside `markdownSaved`).
                Some("copy") => {
                    if let Some(text) = request["params"]["text"].as_str() {
                        crate::app::helpers::gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(text.to_string()), cx);
                    }
                }
                Some("toast") => {
                    let message = request["params"]["message"].as_str().unwrap_or_default().to_string();
                    let error = request["params"]["level"] == "error";
                    if let Some(main) = self.main_window && !message.is_empty() {
                        cx.defer(move |cx| {
                            let _ = main.update(cx, |_, window, cx| {
                                use gpui_component::WindowExt as _;
                                use gpui_component::notification::Notification;
                                window.push_notification(if error { Notification::error(message) } else { Notification::success(message) }, cx);
                            });
                        });
                    }
                }
                Some("actionError") => { cx.notify(); }
                Some("composer") => {
                    self.replace_draft(request["params"]["content"].as_str().unwrap_or_default(), request["method"] == "history", request["params"]["preserveError"] == true, cx);
                    self.input_caret = request["params"]["caret"].as_u64().map(|caret|caret as usize);
                }
                _ => {}
            }
        }
    }

    fn rpc(&mut self, request: Value, cx: &mut Context<Self>) {
        let config = self.config.clone();
        let Some(method) = request["method"].as_str() else {
            return;
        };
        if method == "readNativeComposer" {
            if let Some(runtime) = &self.runtime {
                runtime.call(
                    "resolve",
                    vec![
                        request["id"].clone(),
                        self.draft.clone().into(),
                        Value::Null,
                    ],
                );
            }
            return;
        }
        let endpoint = format!("/api/{method}");
        let diagnostic_method = method.to_owned();
        let import_attachments = method == "importNativeAttachments";
        let mut params = request["params"].as_object().cloned().unwrap_or_default();
        let diagnostic_session = format!("{}:{}", config.project_id, config.session_id);
        params.insert("projectId".into(), config.project_id.into());
        params.insert("sessionId".into(), config.session_id.into());
        let params = Value::Object(params);
        let id = request["id"].clone();
        let task = cx.background_executor().spawn(async move {
            if import_attachments {
                return super::attachments::import_paths(&config.remote, &params)
                    .map_err(|message| json!({"message":message,"endpoint":endpoint}));
            }
            super::rpc::request(config.remote, &endpoint, &params).await
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let (value, error) = match result {
                    Ok(value) => (value, Value::Null),
                    Err(error) => (Value::Null, error),
                };
                crate::support_logs::append_for_scenario(
                    crate::support_logs::GpuiSupportLog::SessionChat,
                    "gpui.sessionChat.viewState",
                    "sessionChat.nativeRpcResult",
                    // The session and the refusal's own sentence let a failed send be matched
                    // to its terminal capture in gxserver's session-chat-send-failures.jsonl.
                    json!({
                        "method": diagnostic_method,
                        "requestId": id,
                        "sessionKey": diagnostic_session,
                        "succeeded": error.is_null(),
                        "errorCode": error["code"],
                        "errorMessage": error["message"],
                    }),
                );
                if let Some(runtime) = &this.runtime {
                    runtime.call("resolve", vec![id, value, error]);
                }
                let _ = cx;
            });
        })
        .detach();
    }
}

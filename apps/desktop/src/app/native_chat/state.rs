use super::runtime_worker::{ChatRuntimeOutput, ChatRuntimeWorker};
use crate::app::gx_chat::DrainedFrame;
use crate::app::model::*;
use futures::StreamExt as _;
use ghostex_gx_chat_core::TranscriptItem;
use gpui::{AppContext as _, Context, Entity, EventEmitter, Focusable as _, Subscription, Window};
use gpui_component::input::{InputEvent, InputState, TextareaState};
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
    /// The header's Session details ring changed: the panel came or went for this chat, or its
    /// context percentage moved. The app draws the header and does not redraw for the chat's own
    /// changes, so without this the ring showed whatever the last unrelated redraw caught.
    SessionDetailsRing,
}

pub(crate) struct NativeChatView {
    pub(crate) config: NativeChatConfig,
    pub(crate) runtime: Option<ChatRuntimeWorker>,
    pub(crate) snapshot: Arc<Value>,
    /// The transcript's rows, whose models the chat host's core holds too.
    pub(crate) items: Arc<Vec<TranscriptItem>>,
    /// The open subagent transcript's items, projected and spliced on their own channel.
    pub(crate) subagent_items: Arc<Vec<TranscriptItem>>,
    pub(crate) error: Option<String>,
    pub(crate) input: Option<Entity<TextareaState>>,
    input_subscription: Option<Subscription>,
    input_observer: Option<Subscription>,
    pub(super) suggestions: super::suggestions::SuggestionWindowState,
    pub(super) frosted_overlays: super::frosted_overlay_window::FrostedOverlayWindows,
    pub(super) composer_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    pub(super) suggestion_selection: Option<(String, usize)>,
    input_window: Option<gpui::WindowId>,
    /// The composer's focus and blur listeners, bound to `input_window`.
    input_focus_listeners: Vec<Subscription>,
    pub(super) option_menu: Option<Entity<super::option_menu::ChatOptionMenu>>,
    /// Which trigger owns the open menu, so pressing it again shuts it (`menu_toggle.rs`).
    pub(super) menu_toggle: super::menu_toggle::ChatMenuToggle,
    /// The working card's pills, measured where their menus open (`chat_pills.rs`).
    pub(super) pill_bounds: super::chat_pills::ChatPillBounds,
    /// What the working card measured at its last paint (`working_card.rs`).
    pub(super) working_card: super::working_card::WorkingCardCells,
    /// The chat box's pill strip (pills_strip.rs).
    pub(super) pills_strip: super::pills_strip::PillsStripCells,
    pub(super) save_markdown_window: super::save_markdown::SaveMarkdownWindowState,
    /// The transcript menu's Reply popover (reply_popover.rs).
    pub(super) reply_popover: super::reply_popover::ReplyPopoverState,
    pub(super) rewind_window: super::rewind::RewindWindowState,
    pub(super) image_viewer: super::image_viewer::ImageViewerState,
    /// The larger preview of one transcript table (table_preview/).
    pub(super) table_preview: super::table_preview::TablePreviewState,
    /// Bytes for the transcript's pictures, read once and shared by the thumbnails and the viewer.
    pub(super) images: super::images::ChatImageCache,
    /// The transcript's Mermaid diagrams, drawn once per source and theme.
    pub(super) mermaid: super::mermaid::ChatMermaidCache,
    pub(super) visual: super::visual::ChatVisualCache,
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
    /// The native view of `main_window`, which the chat's own child windows attach to and are placed in.
    pub(super) drawn_native_view: Option<*mut std::ffi::c_void>,
    /// The content size of the window that drew the chat last, which an expanded table or picture opens over.
    pub(in crate::app::native_chat) drawn_window_size: gpui::Size<gpui::Pixels>,
    /// Where the composer's model pill was last painted, which Option+P opens the model pop-up against.
    pub(super) model_pill_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Set while the next menu this view opens belongs to another surface (the terminal's model pill), not to its own pane.
    pub(super) menu_outside_pane: bool,
    /// The next menu opens above its trigger, on this side of it (`MenuSide`, option_menu/window.rs).
    pub(super) menu_side: Option<super::option_menu::MenuSide>,
    /// Where the composer's More actions, Attach and Send buttons were last painted, which their
    /// menus line up with.
    pub(super) composer_button_bounds: super::toolbar::ComposerButtonBounds,
    /// A model pop-up asked for before this view had its first snapshot: the trigger and its window.
    pub(super) pending_model_menu: Option<(gpui::Bounds<gpui::Pixels>, gpui::AnyWindowHandle)>,
    pub(crate) window_subscription: Option<Subscription>,
    /// Redraws the view when an app modal opens or closes over the window (`AppModalOverChat`).
    _app_modal_subscription: Subscription,
    pub(crate) send_hold_task: Option<gpui::Task<()>>,
    pub(crate) send_hold_fired: bool,
    pub(crate) stop_cooldown_task: Option<gpui::Task<()>>,
    /// How far the status line under the composer is scrolled sideways (context_meter.rs).
    pub(super) context_status_scroll: gpui::ScrollHandle,
    pub(crate) composer_measurements: Option<Value>,
    pub(super) composer_held_key: Option<String>,
    /// When the last `Paste` reached a chat field, for the slow clipboard read log (clipboard.rs).
    pub(super) paste_started: Option<web_time::Instant>,
    /// The chat box's height tween and the arrival fade of the controls an expansion brings back.
    pub(super) composer_animation: super::composer_animation::ComposerAnimation,
    /// Whether this session's status line holds its row of space, remembered across view
    /// re-creations so a chat that is going to show one never paints a frame without it.
    pub(super) status_line_reserved: bool,
    pub(crate) bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// The transcript rows' text width on the last frame no panel was sliding, and the width they
    /// keep this frame while one slides (`render.rs` `hold_text_width_while_sliding`).
    pub(super) settled_text_width: Option<f32>,
    pub(super) held_text_width: Option<f32>,
    /// The transcript scrollbar's measured track: from the top of the transcript region to the
    /// bottom of the pane, so the composer's height never shortens it (scrollbar.rs).
    pub(super) scrollbar_track: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
    /// When the reader last scrolled the transcript, the only thing that shows its scrollbar.
    /// `instant::Instant` because gpui-component's scrollbar takes that type; it is std's on native and a browser clock on wasm32.
    pub(super) transcript_scrolled_at: Option<instant::Instant>,
    /// Whether the last render drew the "Scroll to bottom" pill (scroll_bottom.rs).
    pub(super) scroll_bottom_drawn: std::rc::Rc<std::cell::Cell<bool>>,
    input_needs_sync: bool,
    input_placeholder: String,
    input_undoable: bool,
    pub(super) input_caret: Option<usize>,
    /// The draft's markdown references, and the draft they were parsed from.
    pub(super) composer_references: Vec<super::composer_references::ComposerReference>,
    pub(super) composer_reference_draft: Option<String>,
    pub(super) composer_reference_click: u64,
    pub(super) composer_reference_task: Option<gpui::Task<()>>,
    /// Path of the image pill under the pointer.
    pub(super) composer_image_hover: Option<String>,
    /// Image the caret last sat against, so a caret move repaints the thumbnails only when it matters.
    pub(super) composer_caret_image: Option<String>,
    pub(super) async_answer_input: Option<(String, Entity<TextareaState>)>,
    pub(super) async_answer_subscription: Option<Subscription>,
    pub(super) async_answer_echo: super::async_questions::AsyncAnswerEcho,
    pub(crate) answer_input: Option<(String, Entity<TextareaState>)>,
    pub(crate) answer_subscription: Option<Subscription>,
    pub(super) terminal_dialog_input: Option<super::terminal_dialog::TerminalDialogInput>,
    pub(super) terminal_dialog_key_focus: gpui::FocusHandle,
    pub(super) terminal_picker_input: Option<super::terminal_picker::TerminalPickerInput>,
    pub(crate) note_input: Option<Entity<TextareaState>>,
    pub(crate) note_subscription: Option<Subscription>,
    /// The note and text boxes of the open question forms, by `<form id>:<question>`.
    pub(super) form_inputs: std::collections::HashMap<String, super::question_form::FormInput>,
    /// Transcript search (Cmd+F): the field, and the navigation it last scrolled to.
    pub(super) search_input: Option<Entity<InputState>>,
    pub(super) search_subscription: Option<Subscription>,
    pub(super) search_scrolled_revision: i64,
    /// Files and links: the card's filter field, and the jump it last scrolled to (files_links.rs).
    pub(super) files_links_input: Option<Entity<InputState>>,
    pub(super) files_links_subscription: Option<Subscription>,
    /// The window the filter field was made in: the Files and links menu's own window, which a
    /// reopened menu replaces.
    pub(super) files_links_input_window: Option<gpui::WindowId>,
    /// The Files and links menu is about to open, or its close has been sent to the core and the
    /// document still says open: either way the next render must not open it again.
    pub(super) files_links_menu_pending: bool,
    /// The Resources modal's text boxes, made in the modal's own window (`resource_modal.rs`).
    pub(super) modal_form_inputs:
        std::collections::HashMap<String, super::question_form::FormInput>,
    /// The modal is being drawn, so a form's boxes are looked up in `modal_form_inputs`.
    pub(super) form_in_modal: std::cell::Cell<bool>,
    /// The modal's window was asked for and not yet open, or the reader closed it and the core has
    /// not yet been told.
    pub(super) resource_modal_pending: bool,
    pub(super) resource_modal_closing: bool,
    /// The Session details panel: wanted, docked or floating (session_details/).
    pub(crate) session_details: super::session_details::SessionDetailsView,
    pub(super) files_links_closing: bool,
    pub(super) files_links_jump_revision: u64,
    /// The question jump this pane last scrolled to (`None` before its first document), and the
    /// highlight on the question it landed on (open_questions.rs).
    pub(super) open_question_jump_revision: Option<u64>,
    pub(super) question_flash: Option<super::open_questions::QuestionFlash>,
    /// The "Previous question" pill's attention ping (question_ping.rs).
    pub(super) question_ping: super::question_ping::QuestionPing,
    /// The ? pill's question list: its hover and folded sections (questions_list.rs).
    pub(super) question_list: super::questions_list::QuestionList,
    /// The find the row being drawn gives its text views, set only while a searched row is drawn
    /// (search.rs, `begin_row_find`).
    pub(super) row_find: Option<gpui_component::text::TextFind>,
    /// The same find for the row's one-line labels (tool rows, thinking headlines), which are not
    /// text views (search.rs, `label_highlights`).
    pub(super) row_label_find: Option<super::search::RowLabelFind>,
    /// The fold state last reported to the core while the find bar is open (search.rs, `sync_search_folds`).
    pub(super) search_folds_sent: Option<serde_json::Value>,
    /// Where the transcript's `#123` references link to: the project's GitHub issues URL, or `None`
    /// when its `origin` is not on GitHub (markdown_links.rs).
    pub(super) issue_link_base: Option<gpui::SharedString>,
    /// The working card's Links pill, read from the sidebar store's links for this session
    /// (links_pill.rs).
    pub(super) links_pill: Option<ghostex_gx_core::SessionLinksChatPill>,
    /// Open or closed as this pane last asked the shared runtime, held until its snapshot agrees.
    pub(super) search_pending_open: Option<bool>,
    /// The chat zoom this view's transcript rows were last measured at (zoom.rs).
    pub(super) drawn_zoom_percent: Option<f32>,
    pub(crate) draft: String,
    pub(crate) draft_revision: u64,
    pub(crate) draft_id: String,
    /// A send left this composer and has not settled; Enter does not wait for it (the core holds
    /// later sends behind it), so only summaries read it.
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
    /// Which transcript row asked for each detail, and the rows drawn since the last sync (row_details.rs).
    pub(super) detail_rows: super::row_details::DetailRows,
    pub(super) detail_sync_scheduled: bool,
    /// Armed Delayed Send / Close After Done labels drawn on the working card, set by the app (session_chat_armed_actions.rs).
    pub(crate) armed_actions: Value,
    pub(crate) collapsed: HashSet<String>,
    /// How each fenced block the reader has touched wraps; the rest follow `code_wrap_default`.
    pub(super) code_wrap: HashMap<String, bool>,
    /// React's remembered last choice (session-chat-code-wrap.ts (deleted 2026-10-01)): the blocks that
    /// scroll into view after a toggle start the way the reader last asked for.
    pub(super) code_wrap_default: bool,
    /// The long fenced blocks the reader expanded to their full height (keyed like `code_wrap`).
    pub(super) code_expanded: HashSet<String>,
    /// The composer's dictation session, made the first time the reader presses Dictate.
    pub(crate) dictation: super::dictation::Dictation,
    pub(crate) list: gpui::ListState,
    /// The transcript's own cached view, created on the first draw (transcript_host.rs).
    pub(super) transcript_host: Option<Entity<super::transcript_host::TranscriptHost>>,
    /// The composer's own view, which keystrokes notify instead of the chat (composer_host.rs).
    pub(super) composer_host: Option<Entity<super::composer_host::ComposerHost>>,
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
    /// The whole chat view's own focus (`render.rs`), taken by a click on the transcript's empty space.
    pub(super) surface_focus: gpui::FocusHandle,
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
    /// A redraw is booked for the API error card's countdown (api_error_card.rs).
    pub(super) api_error_tick: std::cell::Cell<bool>,
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
        Self::release_chat_images_with_view(cx);
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
            input_focus_listeners: Vec::new(),
            option_menu: None,
            menu_toggle: Default::default(),
            pill_bounds: Default::default(),
            working_card: Default::default(),
            pills_strip: Default::default(),
            context_editor_window: Default::default(),
            save_markdown_window: Default::default(),
            reply_popover: Default::default(),
            rewind_window: Default::default(),
            image_viewer: Default::default(),
            table_preview: Default::default(),
            images: Default::default(),
            mermaid: Default::default(),
            visual: Default::default(),
            in_work_fold: false,
            hide_file_changes: false,
            disclosure_motion: Default::default(),
            in_subagent: false,
            maximized_window: None,
            maximized_wanted: false,
            maximized_opening: false,
            pane_hidden: false,
            main_window: None,
            drawn_native_view: None,
            drawn_window_size: gpui::Size::default(),
            model_pill_bounds: Default::default(),
            menu_outside_pane: false,
            menu_side: None,
            composer_button_bounds: Default::default(),
            pending_model_menu: None,
            window_subscription: None,
            _app_modal_subscription: cx
                .observe_global::<super::frosted_overlay_window::AppModalOverChat>(|_, cx| {
                    cx.notify()
                }),
            send_hold_task: None,
            send_hold_fired: false,
            stop_cooldown_task: None,
            composer_measurements: None,
            composer_held_key: None,
            paste_started: None,
            composer_animation: Default::default(),
            context_status_scroll: Default::default(),
            bounds: Default::default(),
            settled_text_width: None,
            held_text_width: None,
            scrollbar_track: Default::default(),
            transcript_scrolled_at: None,
            scroll_bottom_drawn: Default::default(),
            input_needs_sync: false,
            input_placeholder: String::new(),
            input_undoable: false,
            input_caret: None,
            composer_references: Vec::new(),
            composer_reference_draft: None,
            composer_reference_click: 0,
            composer_reference_task: None,
            composer_image_hover: None,
            composer_caret_image: None,
            async_answer_input: None,
            async_answer_subscription: None,
            async_answer_echo: Default::default(),
            answer_input: None,
            answer_subscription: None,
            terminal_dialog_input: None,
            terminal_dialog_key_focus: cx.focus_handle(),
            terminal_picker_input: None,
            note_input: None,
            note_subscription: None,
            form_inputs: Default::default(),
            search_input: None,
            search_subscription: None,
            search_scrolled_revision: -1,
            files_links_input: None,
            files_links_subscription: None,
            files_links_input_window: None,
            files_links_menu_pending: false,
            modal_form_inputs: Default::default(),
            form_in_modal: Default::default(),
            resource_modal_pending: false,
            resource_modal_closing: false,
            session_details: Default::default(),
            files_links_closing: false,
            files_links_jump_revision: 0,
            open_question_jump_revision: None,
            question_flash: None,
            question_ping: Default::default(),
            question_list: Default::default(),
            row_find: None,
            row_label_find: None,
            search_folds_sent: None,
            issue_link_base: None,
            links_pill: None,
            search_pending_open: None,
            drawn_zoom_percent: None,
            draft: String::new(),
            draft_revision: 0,
            pending_send: false,
            composer_ready: false,
            expanded: HashSet::new(),
            nested_scrolls: Default::default(),
            row_details: Value::Null,
            detail_demand: Default::default(),
            detail_sent: Default::default(),
            detail_rows: Default::default(),
            detail_sync_scheduled: false,
            armed_actions: Value::Array(Vec::new()),
            collapsed: HashSet::new(),
            code_wrap: HashMap::new(),
            code_wrap_default: false,
            code_expanded: HashSet::new(),
            dictation: None,
            list,
            transcript_host: None,
            composer_host: None,
            transcript_reveal: Default::default(),
            transcript_inset: 0.0,
            minimap: Default::default(),
            subagent_list,
            subagent_focus: cx.focus_handle(),
            subagent_focused: false,
            surface_focus: cx.focus_handle(),
            pane_focused: false,
            composer_focused: false,
            short_pane_composer_open: false,
            focus_requested: false,
            subscriptions: Vec::new(),
            last_render: None,
            last_notified: None,
            notify_scheduled: false,
            api_error_tick: std::cell::Cell::new(false),
            under_workarea_header: false,
        }
    }

    /// CDXC:SessionChat 2026-10-22 DECISION: The composer lays out each line whose first strong character is right-to-left (Arabic, Hebrew, Persian…) right-to-left and right-aligned, "like it works in text area in the web", i.e. `dir="auto"` per line; left-to-right lines stay as they are. The chat's other text fields do the same.
    pub(crate) fn ensure_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_none() {
            let draft = self.draft.clone();
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .submit_on_enter(true)
                    .auto_grow(3, 7)
                    .auto_text_direction(true)
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
                // GPUI calls this in the window that last drew the chat, and the chat's own popups
                // (this list, the scroll pill) draw it too; the field's focus is only known in the
                // field's window, so another window keeps the focus the listeners reported.
                if this.input_window != Some(window.window_handle().window_id()) {
                    if this.composer_focused {
                        this.update_suggestion_selection(cx);
                    }
                } else if input.read(cx).focus_handle(cx).is_focused(window) {
                    // A focus edge the listeners below missed is caught up by the field's next change.
                    // The field holding its window's focus while that window is inactive (an app
                    // modal is up) is not a gain: the listeners report it once the window is active.
                    if window.is_window_active() {
                        this.composer_focus_gained(window, cx);
                    }
                    this.update_suggestion_selection(cx);
                } else {
                    this.composer_focus_lost(cx);
                }
                let caret_image = this.composer_caret_image(cx);
                if this.composer_caret_image != caret_image {
                    this.composer_caret_image = caret_image;
                    this.notify_composer(cx);
                }
            }));
            self.input_subscription =
                Some(
                    cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| {
                        if let InputEvent::Change = event {
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
                            this.notify_composer(cx);
                        }
                    }),
                );
            // CDXC:SessionChat 2026-09-27 WHY:
            // The field's own `InputEvent::Focus` and `Blur` come from focus listeners it registers on the window it was created in, and one chat view is drawn in the main window or in the floating sessions panel's window. Focusing the composer in the other window emitted nothing, so the chat never counted its composer as focused and the `$`, `@` and `/` list stayed shut while the keyboard still moved through it. The listeners are therefore bound to the window the chat is drawn in, and a new window starts from its own focus state. GPUI switches a new focus listener on only after a deferred tick, so a chat that creates its field and focuses it in the same frame (Cmd+Shift+O, a new agent) lost the edge about half the time; the field's observer above reconciles the flag with the real focus on every change, so the first `$` typed still opens the list. GPUI also hands its focus listeners an empty focus path while their window is not the key window, so the window losing key status (as it did for a moment each time the list's own window opened) reached the chat as a blur and shut the list it had just opened; a blur while the field still holds its window's focus is not a blur of the composer. And GPUI calls the observer in the window that last read the chat while drawing, which is the list's own window once it is open, so asking that window about the field's focus answered "not focused" and shut the list after one frame (the flicker); the observer only judges focus in the field's own window.
            let focus = input.read(cx).focus_handle(cx);
            self.composer_focused = focus.is_focused(window);
            self.input_focus_listeners = vec![
                cx.on_focus(&focus, window, |this, window, cx| {
                    this.composer_focus_gained(window, cx);
                }),
                cx.on_blur(&focus, window, {
                    let focus = focus.clone();
                    move |this, window, cx| {
                        // GPUI reports its window going inactive as a blur too; the field still
                        // holds the window's focus then (see the CDXC above).
                        if !focus.is_focused(window) {
                            this.composer_focus_lost(cx);
                        }
                    }
                }),
            ];
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
        /*
        CDXC:SessionChat 2026-10-01 DECISION:
        User: typing in the chat's search field must never be moved to the chat box when they did not click it. A queued composer focus (a keyboard handoff, a draft the core put back, a send's reset) lands only while no other field of this chat holds the keyboard; the find bar, the note and the answer fields keep it until the user leaves them (Escape, a click, the close button).
        */
        if self.focus_requested {
            self.focus_requested = false;
            if let Some(input) = &self.input {
                let composer = input.read(cx).focus_handle(cx);
                if composer.is_focused(window) || !self.chat_text_field_focused(window, cx) {
                    composer.focus(window, cx);
                }
            }
        }
    }

    /// The composer took the keyboard. A second report of the same focus does nothing.
    fn composer_focus_gained(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer_focused {
            return;
        }
        super::focus::reclaim_keyboard_focus(window);
        self.composer_focused = true;
        self.sync_suggestion_window(cx);
        self.invoke(json!({"type":"composerExpand","editor":true}), cx);
        cx.emit(NativeChatEvent::ComposerFocused);
        cx.notify();
    }

    /// The composer lost the keyboard. A second report of the same blur does nothing.
    fn composer_focus_lost(&mut self, cx: &mut Context<Self>) {
        if !self.composer_focused {
            return;
        }
        self.composer_focused = false;
        self.short_pane_composer_open = false;
        self.sync_suggestion_window(cx);
        self.save_draft(cx);
        // A short pane's box collapses again once it loses focus (composer_scroll.rs).
        cx.notify();
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

    /// Adds `content` after what is already in the chat box (a review comment pill, an
    /// annotation), so a second one joins the first instead of replacing it.
    pub(crate) fn append_prompt(&mut self, content: &str, cx: &mut Context<Self>) {
        let next =
            ghostex_gx_chat_core::composer::review_comments::append_to_draft(&self.draft, content);
        self.replace_draft(&next, false, false, cx);
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
    /// The state is applied either way; only a visible view asks for a redraw, and a parked one paints the latest state when it comes back.
    /// CDXC:SessionChat 2026-09-19 WHY:
    /// A streaming agent produced a runtime output several times a frame, and each one redrew the whole window, which re-lays out every visible transcript row; with a few agents streaming that was most of the UI thread.
    /// Redraws from runtime output are coalesced to one per 50ms; the last output in a burst still paints, only never sooner than that.
    /// CDXC:SessionChat 2026-09-24 WHY:
    /// The model pop-up opened from terminal view belongs to a hidden chat view and repaints only by observing it, so an open pop-up counts as shown; otherwise its tab and search changes reached the runtime while the pop-up kept painting the state it opened with. Supersedes the same rule for the retired full-screen picker.
    /// CDXC:SessionChat 2026-09-28 WHY:
    /// Elapsed time since a draw cannot tell whether a pane is hidden: an idle visible chat stopped painting model results after one second until a hover redrew it. Use the pane's existing visibility state. Claude's first-run cards on a new Windows install stayed blank the same way.
    fn notify_if_shown(&mut self, cx: &mut Context<Self>) {
        const NOTIFY_MIN_INTERVAL: Duration = Duration::from_millis(50);
        if self.option_menu.is_none() && (self.pane_hidden || self.last_render.is_none()) {
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

    fn apply_output(&mut self, output: DrainedFrame, cx: &mut Context<Self>) {
        let DrainedFrame {
            envelope: mut output,
            items_splice,
            subagent_splice,
        } = output;
        if let Some(splice) = items_splice {
            // The host ships only the changed window of transcript items (see transcriptItemsSplice in native-host.ts).
            let items = Arc::make_mut(&mut self.items);
            let start = splice.start.min(items.len());
            let end = start.saturating_add(splice.delete_count).min(items.len());
            let inserted_len = splice.items.len();
            items.splice(start..end, splice.items);
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
        if let Some(splice) = subagent_splice {
            self.apply_subagent_splice(splice);
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
            self.retain_launch_welcome(&mut snapshot);
            if let Some(measured) = self
                .composer_measurements
                .as_ref()
                .and_then(|value| serde_json::from_value(value.clone()).ok())
            {
                snapshot["composerOverflow"] =
                    json!(ghostex_gx_chat_core::composer::layout::fit_composer_controls(&measured));
            }
            let selection_only =
                super::suggestions::only_selection_moved(&self.snapshot, &snapshot);
            let ring_moved = ["enabled", "ring"]
                .iter()
                .any(|key| self.snapshot["sessionDetails"][key] != snapshot["sessionDetails"][key]);
            self.snapshot = Arc::new(snapshot);
            if ring_moved {
                cx.emit(NativeChatEvent::SessionDetailsRing);
            }
            if selection_only {
                self.repaint_suggestions(cx);
            } else {
                if self.snapshot["suggestions"].is_null() {
                    self.suggestions.clear_hover();
                }
                self.adopt_status_line_reservation();
                self.open_pending_model_menu(cx);
                self.sync_context_editor_window(cx);
                self.sync_save_markdown_window(cx);
                self.sync_rewind_window(cx);
                self.sync_suggestion_window(cx);
                self.load_earlier_if_near_top(cx);
                self.notify_if_shown(cx);
            }
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
                    // Text that went in before the chat box was ready (a review comment pill)
                    // joins the restored draft the way a later one would.
                    let draft = match &local_draft {
                        Some(local) => ghostex_gx_chat_core::composer::review_comments::append_to_draft(&restored, local),
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
                // The page card's picture (`page_card.rs`), which the view keeps for itself.
                Some("host") if request["method"] == "pageThumbnail" => self.receive_page_thumbnail(&request["params"], cx),
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
                // CDXC:SessionChat 2026-10-17 WHY: The chat's own notices (a Q-tag or Files and links jump that cannot reach its message, a coordinator thread that will not open) went to the window's gpui-component notification layer, which the desktop never shows over its panes, so they vanished. They go out as the raw `{type: "toast"}` app message, which the desktop (`session_chat/host_actions.rs`) and the web build (`chat_host.rs`) both draw in the app's toast window, like dictation's failure toast.
                Some("toast") => {
                    let message = request["params"]["message"].as_str().unwrap_or_default().to_string();
                    if !message.is_empty() {
                        let level = if request["params"]["level"] == "error" { "error" } else { "info" };
                        cx.emit(NativeChatEvent::Host(json!({"type": "toast", "level": level, "title": message})));
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

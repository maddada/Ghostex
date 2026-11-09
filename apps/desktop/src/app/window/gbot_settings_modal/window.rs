//! The GBot Settings window: a GBot's Profile, Instructions, Department, Memory, Routines and
//! Computer on a Settings-style tab rail, opened from the bot's row.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q2104 A): a GBot's settings open in "its own window, from the bot's row", and "the window has the same look as Settings": Settings' palette and font, a left tab rail headed by the bot's icon in its colour and its name, and Settings' page titles and grouped rows.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! Settings applies each change at once, but every GBot edit is a change record, a SOUL.md write and, for a new name, a project rename, so typing in Name or Instructions must not save per keystroke. The window edits a copy and Save sends only the fields that changed in one `edit`; like the orchestrator editors (CDXC:Coordinators 2026-10-14 in window/coordinator_details_modal.rs) it stays open while the save runs, keeps the edits and shows gxserver's sentence when the save is refused, and closes on success.
//! SEE-ALSO: apps/desktop/src/app/gbot_settings_modal_lifecycle.rs (open, save), server/src/gbots/endpoint.rs (`show`, `edit`).
use super::super::native_modal_kit::*;
use super::super::settings_modal::palette::{SETTINGS_FONT, SettingsPalette};
use super::memory::{GbotMemoryReply, GbotMemoryTab, MemoryOpTag};
use super::model::*;
use super::parts::kit_palette;
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle,
    Focusable as _, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    ScrollHandle, Styled as _, Subscription, WeakEntity, Window, div, px,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState, TextareaState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value};
use std::rc::Rc;

const CANCEL: &str = "Cancel";
const SAVE: &str = "Save";
const SAVING: &str = "Saving…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GbotSettingsTab {
    Profile,
    Instructions,
    Department,
    Memory,
    Routines,
    Computer,
    BotApi,
}

impl GbotSettingsTab {
    pub(crate) const ALL: [Self; 7] = [
        Self::Profile,
        Self::Instructions,
        Self::Department,
        Self::Memory,
        Self::Routines,
        Self::Computer,
        Self::BotApi,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::Instructions => "Instructions",
            Self::Department => "Department",
            Self::Memory => "Memory",
            Self::Routines => "Routines",
            Self::Computer => "Computer",
            Self::BotApi => "Bot API",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Profile => "titlebar/user.svg",
            Self::Instructions => "titlebar/file-text.svg",
            Self::Department => "titlebar/users.svg",
            Self::Memory => "titlebar/brain.svg",
            Self::Routines => "titlebar/clock.svg",
            Self::Computer => "titlebar/device-laptop.svg",
            Self::BotApi => "titlebar/key.svg",
        }
    }
}

/// Where a save's outcome goes: the window that sent it.
pub(crate) struct GbotSettingsSaveReply {
    view: WeakEntity<GpuiGbotSettingsModalWindow>,
    window: AnyWindowHandle,
}

impl GbotSettingsSaveReply {
    /// Hands the outcome to the window: success closes it, a refusal shows gxserver's sentence and
    /// keeps the edits. False when the window is gone (cancelled meanwhile).
    pub(crate) fn finish(self, result: Result<(), String>, cx: &mut App) -> bool {
        let view = self.view;
        self.window
            .update(cx, |_root, window, cx| {
                view.update(cx, |modal, cx| modal.finish_save(result, window, cx))
            })
            .is_ok_and(|delivered| delivered.is_ok())
    }
}

pub(crate) enum GbotSettingsModalCommand {
    /// The `edit` fields that changed (never empty), for `bot_id`.
    Save {
        bot_id: String,
        changes: Map<String, Value>,
        reply: GbotSettingsSaveReply,
    },
    /// The Memory tab (memory.rs): read the memory (`memoryShow`); save MEMORY.md (`memoryEdit`
    /// with `base`, the hash it was read at); undo change `change` (`memoryUndo`). Each answers
    /// through `reply` with `memoryShow` as it is afterwards, or gxserver's refusal.
    MemoryLoad {
        bot_id: String,
        reply: GbotMemoryReply,
    },
    MemorySave {
        bot_id: String,
        text: String,
        base: String,
        reply: GbotMemoryReply,
    },
    MemoryUndo {
        bot_id: String,
        change: String,
        reply: GbotMemoryReply,
    },
    /// The Routines tab (routines.rs): `op` (none for a plain read), then `routines` as it is
    /// afterwards, or gxserver's refusal.
    Routines {
        bot_id: String,
        op: Option<Value>,
        reply: super::routines::GbotRoutinesReply,
    },
    /// The Department tab (department.rs): each of `ops` in order (none for a plain read), then
    /// `{departments, bot}` as they are afterwards, or the first refusal.
    Departments {
        bot_id: String,
        ops: Vec<Value>,
        reply: super::department::GbotDepartmentReply,
    },
    /// The Computer tab (computer.rs): `op` (none for a plain read), then `computer` as it is
    /// afterwards, or gxserver's refusal.
    Computer {
        bot_id: String,
        op: Option<Value>,
        reply: super::computer::GbotComputerReply,
    },
    /// The Bot API tab (bot_api.rs): `op` (none for a plain read), then the API's state as gxserver
    /// answers it (the key in it when one was just made), or gxserver's refusal.
    BotApi {
        bot_id: String,
        op: Option<Value>,
        reply: super::bot_api::GbotApiReply,
    },
    /// Shows a memory file in Finder.
    Reveal {
        path: String,
    },
    Cancel,
}

pub(crate) type GbotSettingsModalHost = Rc<dyn Fn(GbotSettingsModalCommand, &mut App)>;

pub(crate) struct GbotSettingsModalConfig {
    pub(crate) record: GbotSettingsRecord,
    /// `(agentId, name)` the bot may run on (`gbot_agent_choices`).
    pub(crate) agents: Vec<(String, String)>,
    pub(crate) palette: ModalPalette,
    /// The tab it opens on.
    pub(crate) tab: GbotSettingsTab,
    /// A `memoryShow` answer already read (opening on the Memory tab), so the tab draws at once.
    pub(crate) memory: Option<Value>,
    /// The Department tab's `{departments, bot}` already read, so the tab draws at once.
    pub(crate) departments: Option<Value>,
}

pub(crate) struct GpuiGbotSettingsModalWindow {
    pub(super) host: GbotSettingsModalHost,
    pub(super) palette: SettingsPalette,
    /// The kit controls' palette in Settings' tones.
    pub(super) kit: ModalPalette,
    pub(super) original: GbotSettingsRecord,
    pub(super) draft: GbotSettingsRecord,
    pub(super) agents: Vec<(String, String)>,
    pub(super) tab: GbotSettingsTab,
    pub(super) name_input: Entity<InputState>,
    pub(super) title_input: Entity<InputState>,
    pub(super) description_input: Entity<TextareaState>,
    pub(super) model_input: Entity<InputState>,
    pub(super) soul_input: Entity<TextareaState>,
    pub(super) agent_select: ModalSelect,
    /// Profile's Voice picker (voice.rs).
    pub(super) voice_select: ModalSelect,
    pub(super) page_scroll: ScrollHandle,
    /// A save is on its way to gxserver.
    pub(super) saving: bool,
    /// Why the last save was refused, shown beside the buttons.
    pub(super) error: Option<String>,
    pub(super) memory: GbotMemoryTab,
    pub(super) pending_memory_op: Option<MemoryOpTag>,
    pub(super) routines: super::routines::GbotRoutinesTab,
    pub(super) department: super::department::GbotDepartmentTab,
    pub(super) computer: super::computer::GbotComputerTab,
    pub(super) bot_api: super::bot_api::GbotApiTab,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// Which draft field a text control writes.
#[derive(Clone, Copy)]
enum Field {
    Name,
    Title,
    Description,
    Model,
    Soul,
}

impl GpuiGbotSettingsModalWindow {
    pub(crate) fn new(
        config: GbotSettingsModalConfig,
        host: GbotSettingsModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let record = config.record;
        let input = |value: &str, placeholder: &'static str, window: &mut Window, cx: &mut App| {
            let value = value.to_string();
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        let name_input = input(&record.name, "e.g. Researcher", window, cx);
        let title_input = input(&record.title, "e.g. Finds and compares options", window, cx);
        let model_input = input(&record.model, "The agent's default", window, cx);
        let description = record.description.clone();
        let description_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 6)
                .placeholder("What this GBot is for, in a sentence or two.")
                .default_value(description)
        });
        let soul = record.soul.clone();
        let soul_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(
                    "How this GBot works, e.g. Keep a running shortlist and say why each option made it.",
                )
                .default_value(soul)
        });
        let mut subscriptions = Vec::new();
        for (state, field) in [
            (&name_input, Field::Name),
            (&title_input, Field::Title),
            (&model_input, Field::Model),
        ] {
            subscriptions.push(cx.subscribe_in(
                state,
                window,
                move |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value().to_string();
                        this.set_field(field, value, cx);
                    }
                },
            ));
        }
        for (state, field) in [
            (&description_input, Field::Description),
            (&soul_input, Field::Soul),
        ] {
            subscriptions.push(cx.subscribe_in(
                state,
                window,
                move |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value().to_string();
                        this.set_field(field, value, cx);
                    }
                },
            ));
        }
        let department = super::department::GbotDepartmentTab::new(
            record.department_id.clone(),
            record.is_lead,
            window,
            cx,
        );
        // Its fields enable Save Department and Add as they are typed in.
        subscriptions.push(cx.subscribe_in(
            &department.name_input,
            window,
            |_this: &mut Self, _, _: &InputEvent, _, cx| cx.notify(),
        ));
        subscriptions.push(cx.subscribe_in(
            &department.new_name_input,
            window,
            |_this: &mut Self, _, _: &InputEvent, _, cx| cx.notify(),
        ));
        subscriptions.push(cx.subscribe_in(
            &department.instructions_input,
            window,
            |_this: &mut Self, _, _: &InputEvent, _, cx| cx.notify(),
        ));
        let mut department = department;
        if let Some(answer) = config.departments.clone() {
            department.requested = true;
            department.preload(answer);
        } else if config.tab == GbotSettingsTab::Department {
            cx.defer_in(window, |this: &mut Self, window, cx| {
                this.ensure_department(window, cx)
            });
        }
        let palette = SettingsPalette::resolve(&config.palette);
        let mut memory = GbotMemoryTab::new(window, cx);
        if let Some(view) = config
            .memory
            .as_ref()
            .and_then(super::memory_model::GbotMemoryView::from_show)
        {
            memory.view = Some(view);
            memory.requested = true;
        } else if config.tab == GbotSettingsTab::Memory {
            cx.defer_in(window, |this: &mut Self, window, cx| {
                this.ensure_memory(window, cx)
            });
        }
        if config.tab == GbotSettingsTab::Routines {
            cx.defer_in(window, |this: &mut Self, window, cx| {
                this.ensure_routines(window, cx)
            });
        }
        if config.tab == GbotSettingsTab::Computer {
            cx.defer_in(window, |this: &mut Self, window, cx| {
                this.ensure_computer(window, cx)
            });
        }
        if config.tab == GbotSettingsTab::BotApi {
            cx.defer_in(window, |this: &mut Self, window, cx| {
                this.ensure_bot_api(window, cx)
            });
        }
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            kit: kit_palette(&palette),
            palette,
            original: record.clone(),
            draft: record,
            agents: config.agents,
            tab: config.tab,
            name_input,
            title_input,
            description_input,
            model_input,
            soul_input,
            agent_select: ModalSelect::new(),
            voice_select: ModalSelect::new(),
            page_scroll: ScrollHandle::new(),
            saving: false,
            error: None,
            memory,
            pending_memory_op: None,
            routines: Default::default(),
            department,
            computer: Default::default(),
            bot_api: Default::default(),
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    fn set_field(&mut self, field: Field, value: String, cx: &mut Context<Self>) {
        let slot = match field {
            Field::Name => &mut self.draft.name,
            Field::Title => &mut self.draft.title,
            Field::Description => &mut self.draft.description,
            Field::Model => &mut self.draft.model,
            Field::Soul => &mut self.draft.soul,
        };
        *slot = value;
        cx.notify();
    }

    /// The fields Save would send.
    pub(super) fn changes(&self) -> Map<String, Value> {
        self.draft.changes_from(&self.original)
    }

    pub(super) fn select_tab(
        &mut self,
        tab: GbotSettingsTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.agent_select.close();
        self.voice_select.close();
        self.page_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        if tab == GbotSettingsTab::Memory {
            self.ensure_memory(window, cx);
        }
        if tab == GbotSettingsTab::Routines {
            self.ensure_routines(window, cx);
        }
        if tab == GbotSettingsTab::Computer {
            self.ensure_computer(window, cx);
        }
        if tab == GbotSettingsTab::BotApi {
            self.ensure_bot_api(window, cx);
        }
        if tab == GbotSettingsTab::Department {
            self.ensure_department(window, cx);
        }
        if tab == GbotSettingsTab::Instructions {
            self.soul_input
                .update(cx, |input, cx| input.focus(window, cx));
        } else {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(GbotSettingsModalCommand::Cancel, cx);
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Cmd/Ctrl+Enter in the memory editor saves the memory, not the profile.
        if self.tab == GbotSettingsTab::Memory && self.memory.editing {
            self.save_memory(window, cx);
            return;
        }
        if self.saving {
            return;
        }
        let changes = self.changes();
        if changes.is_empty() || self.draft.name.trim().is_empty() {
            return;
        }
        let reply = GbotSettingsSaveReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
        };
        self.saving = true;
        self.error = None;
        self.agent_select.close();
        self.voice_select.close();
        cx.notify();
        (self.host)(
            GbotSettingsModalCommand::Save {
                bot_id: self.original.bot_id.clone(),
                changes,
                reply,
            },
            cx,
        );
    }

    fn finish_save(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => window.remove_window(),
            Err(message) => {
                self.saving = false;
                self.error = Some(message);
                cx.notify();
            }
        }
    }

    pub(super) fn toggle_agent_select(&mut self, cx: &mut Context<Self>) {
        let selected = self.agent_index();
        self.voice_select.close();
        self.agent_select.toggle(selected);
        cx.notify();
    }

    pub(super) fn agent_index(&self) -> Option<usize> {
        self.agents
            .iter()
            .position(|(id, _)| *id == self.draft.agent_id)
    }

    pub(super) fn choose_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some((id, _)) = self.agents.get(index) {
            self.draft.agent_id = id.clone();
        }
        self.agent_select.close();
        cx.notify();
    }

    fn single_line_focused(&self, window: &Window, cx: &App) -> bool {
        [&self.name_input, &self.title_input, &self.model_input]
            .iter()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Enter saves from a one-line field; in Description and Instructions it is a new line, and
    /// Cmd/Ctrl+Enter saves from anywhere.
    fn on_enter_action(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if !action.secondary && !self.single_line_focused(window, cx) {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        self.save(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.agent_select.open || self.voice_select.open {
            self.agent_select.close();
            self.voice_select.close();
            cx.notify();
            return;
        }
        if self.tab == GbotSettingsTab::Memory && self.memory.editing {
            self.cancel_memory_edit(cx);
            return;
        }
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        match self.agent_select.handle_key(key, self.agents.len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_agent(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match self.voice_select.handle_key(key, self.voice_labels().len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_voice(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match key {
            "escape" => self.cancel(window, cx),
            "enter" if event.keystroke.modifiers.secondary(cx) && !event.is_held => {
                self.save(window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.kit;
        let unchanged = self.changes().is_empty() || self.draft.name.trim().is_empty();
        let cancel = modal_action_button(
            &p,
            "gbot-settings-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let save = modal_action_button(
            &p,
            "gbot-settings-save",
            if self.saving { SAVING } else { SAVE },
            None,
            ModalButtonTone::Primary,
            self.saving || unchanged,
            |this, window, cx| this.save(window, cx),
            cx,
        );
        h_flex()
            .w_full()
            .flex_shrink_0()
            .pr(px(16.0))
            .gap(px(12.0))
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .children(self.error.clone().map(|error| modal_error(&p, error))),
            )
            .child(h_flex().flex_shrink_0().w(px(104.0)).child(cancel))
            .child(h_flex().flex_shrink_0().w(px(104.0)).child(save))
            .into_any_element()
    }
}

impl Render for GpuiGbotSettingsModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let rail = self.render_rail(cx);
        let page = match self.tab {
            GbotSettingsTab::Profile => self.render_profile(window, cx),
            GbotSettingsTab::Instructions => self.render_instructions(window, cx),
            GbotSettingsTab::Department => self.render_department(window, cx),
            GbotSettingsTab::Memory => self.render_memory(window, cx),
            GbotSettingsTab::Routines => self.render_routines(cx),
            GbotSettingsTab::Computer => self.render_computer(cx),
            GbotSettingsTab::BotApi => self.render_bot_api(cx),
        };
        let footer = self.render_footer(cx);
        let menu = self
            .render_agent_menu(window, cx)
            .or_else(|| self.render_voice_menu(window, cx));
        div()
            .id("ghostex-gpui-gbot-settings-modal")
            .size_full()
            .overflow_hidden()
            .bg(hsla(p.surface))
            .font_family(SETTINGS_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .capture_action(cx.listener(Self::on_enter_action))
            .capture_action(cx.listener(Self::on_escape_action))
            .child(
                v_flex().size_full().pt(px(20.0)).child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .items_stretch()
                        // No right padding: the page's scroll area reaches the window's edge so its
                        // bar sits by it, as in Settings; the page and footer pad it back.
                        .pl(px(16.0))
                        .pb(px(16.0))
                        .gap(px(16.0))
                        .child(rail)
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .gap(px(14.0))
                                .child(page)
                                .child(footer),
                        ),
                ),
            )
            .children(menu)
    }
}

impl ModalCornerClose for GpuiGbotSettingsModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }

    /// Unsaved edits or a save on its way keep the window open on a click away.
    fn keeps_open_on_click_away(&self, _window: &Window, cx: &App) -> bool {
        self.saving
            || !self.changes().is_empty()
            || self.memory_edited(cx)
            || self.department_edited(cx)
    }
}

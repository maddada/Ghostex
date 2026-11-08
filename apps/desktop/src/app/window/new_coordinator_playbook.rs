//! The New Coordinator dialog's Playbook overrides: the row that shows the user's overrides (the
//! instructions every coordinator reads after Ghostex's playbook, ranked above it), the small
//! editor page its Edit button opens, and the read-only playbook page its View playbook button
//! opens. Ghostex's playbook itself is never edited.
//! See CDXC:Coordinators 2026-11-07 in server/src/coordinators/playbook.rs.
//!
//! CDXC:Coordinators 2026-11-08 DECISION:
//! User, of the New Coordinator dialog's "Playbook overrides: None. Coordinators follow Ghostex's playbook as it ships. [Edit]" row: "this ux here sucks, there's no way to see the actual current" playbook. View playbook (here and beside Edit in Settings > Chat > Playbook overrides) opens a scrollable, read-only page of what `ghostex coordinator guide` prints for the project (`/api/readCoordinatorGuide`, which records no read without a session): Ghostex's playbook and the reply format the user chose, then the user's overrides in their own box marked as theirs, with Edit overrides to change them.
use super::*;
use gpui::StatefulInteractiveElement as _;
use gpui::{SharedString, Styled as _};
use std::time::Duration;

const FIELD_OVERRIDES: &str = "Playbook overrides";
const OVERRIDES_TITLE: &str = "Playbook overrides";
pub(crate) const OVERRIDES_PLACEHOLDER: &str =
    "One instruction per line, e.g.\nNever push without asking.\nIgnore the board rules.";
pub(crate) const OVERRIDES_DESCRIPTION: &str = "Your own instructions for every coordinator, read after Ghostex's playbook. They take precedence over it, and Ghostex's playbook keeps getting its updates.";
pub(crate) const OVERRIDES_HINT: &str =
    "Every coordinator, existing ones too, follows them from its next message.";
const EDIT: &str = "Edit";
pub(crate) const VIEW_PLAYBOOK: &str = "View playbook";
pub(crate) const PLAYBOOK_VIEW_TITLE: &str = "Coordinator playbook";
pub(crate) const PLAYBOOK_VIEW_DESCRIPTION: &str = "What every coordinator reads, top to bottom: Ghostex's playbook with the reply format you chose, then your overrides, which take precedence. Ghostex's part comes with Ghostex and updates with it.";
pub(crate) const EDIT_OVERRIDES: &str = "Edit overrides";
const GHOSTEX_PART_TITLE: &str = "Ghostex's playbook";
const GHOSTEX_PART_LABEL: &str = "Read-only";
const OVERRIDES_PART_TITLE: &str = "Your overrides";
const OVERRIDES_PART_LABEL: &str = "Yours";
const NO_OVERRIDES: &str =
    "None yet. Edit overrides adds instructions every coordinator follows over Ghostex's playbook.";
const GUIDE_LOADING: &str = "Loading the playbook…";
/// The heading the guide puts the user's overrides under (server/src/coordinators/role.rs
/// `PLAYBOOK_OVERRIDES_HEADING`); the page shows them in their own box instead.
const GUIDE_OVERRIDES_HEADING: &str = "## The user's overrides";
const BACK: &str = "Back";
const SAVE: &str = "Save";
const SAVING: &str = "Saving…";
const USE_GHOSTEX: &str = "Use Ghostex's Playbook";
const USE_GHOSTEX_CONFIRM: &str = "Click again to switch";
/// How long Use Ghostex's Playbook stays armed for its confirming click.
const USE_GHOSTEX_ARM_TIMEOUT: Duration = Duration::from_secs(4);

/// The user's overrides as `/api/readCoordinatorPlaybook` answers them, with the full copies
/// saved before overrides that are still stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrchestratorPlaybookInfo {
    pub(crate) overrides: String,
    /// Where each old full copy applies: "all projects" or a project's name.
    pub(crate) old_copies: Vec<String>,
    /// Coordinators in the dialog's project read an old full copy, not Ghostex's playbook.
    pub(crate) old_copy_in_use: bool,
}

impl OrchestratorPlaybookInfo {
    /// `None` for a gxserver older than overrides (it answers no `overrides`): the row stays out.
    pub(crate) fn from_view(view: &Value) -> Option<Self> {
        let overrides = view.get("overrides")?;
        Some(Self {
            overrides: overrides
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
            old_copies: old_copy_places(view),
            old_copy_in_use: view
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|source| source != "shipped"),
        })
    }
}

/// The places of a view's `oldCopies`: "all projects", or each project's name.
pub(crate) fn old_copy_places(view: &Value) -> Vec<String> {
    view.get("oldCopies")
        .and_then(Value::as_array)
        .map(|copies| {
            copies
                .iter()
                // A copy turned off is kept but not followed.
                .filter(|copy| copy.get("enabled").and_then(Value::as_bool) != Some(false))
                .map(|copy| {
                    let text = |key: &str| {
                        copy.get(key)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .trim()
                            .to_string()
                    };
                    if text("scope") == "allProjects" {
                        "all projects".to_string()
                    } else if text("name").is_empty() {
                        "a project".to_string()
                    } else {
                        text("name")
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// [`old_copies_note`] for an editor without the Use Ghostex's Playbook button: it says where the
/// button is.
pub(crate) fn old_copies_note_elsewhere(places: &[String]) -> Option<String> {
    old_copies_note(places).map(|note| {
        note.replace(
            "Use Ghostex's Playbook saves it",
            "Use Ghostex's Playbook in Settings > Chat > Playbook overrides saves it",
        )
    })
}

/// The note about full copies saved before overrides, or `None` when there are none.
pub(crate) fn old_copies_note(places: &[String]) -> Option<String> {
    if places.is_empty() {
        return None;
    }
    let list = match places {
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        [] => String::new(),
    };
    Some(format!(
        "An edited full playbook saved before overrides is still in use for {list}, without Ghostex's updates. Use Ghostex's Playbook saves it as a Markdown file and moves every coordinator to Ghostex's playbook."
    ))
}

/// The row's summary: none, or the overrides' first line.
pub(crate) fn overrides_summary(overrides: &str) -> String {
    overrides
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "None. Coordinators follow Ghostex's playbook as it ships.".to_string())
}

/// Ghostex's part of a guide (`/api/readCoordinatorGuide`): the playbook and the reply format,
/// without the user's overrides that end it.
pub(crate) fn guide_ghostex_part(guide: &str) -> String {
    let marker = format!("\n{GUIDE_OVERRIDES_HEADING}\n");
    guide
        .find(&marker)
        .map_or(guide, |at| &guide[..at])
        .trim()
        .to_string()
}

/// The colours the read-only playbook page draws with, from either dialog's palette.
#[derive(Clone, Copy)]
pub(crate) struct PlaybookViewColors {
    pub(crate) light: bool,
    pub(crate) foreground: gpui::Rgba,
    pub(crate) muted: gpui::Rgba,
    pub(crate) panel: gpui::Rgba,
    pub(crate) hairline: gpui::Rgba,
    pub(crate) destructive: gpui::Rgba,
}

/// The read-only playbook page's body: Ghostex's part (`guide`: loading, read, or why it could not
/// be read) as Markdown, then the user's overrides in their own box marked as theirs. The caller
/// puts it in a scroll area.
pub(crate) fn playbook_view_body(
    c: PlaybookViewColors,
    id: &str,
    guide: Option<&Result<String, String>>,
    overrides: &str,
) -> AnyElement {
    let title_row = |title: &'static str, label: &'static str| {
        h_flex()
            .w_full()
            .justify_between()
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(hsla(c.muted))
                    .child(title),
            )
            .child(
                div()
                    .px(px(7.0))
                    .py(px(1.0))
                    .rounded(px(999.0))
                    .border_1()
                    .border_color(hsla(c.hairline))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(hsla(c.muted))
                    .child(label),
            )
    };
    let boxed = |child: AnyElement| {
        div()
            .w_full()
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(c.hairline))
            .bg(hsla(c.panel))
            .child(child)
    };
    let note = |text: String, color: gpui::Rgba| {
        div()
            .text_size(px(13.0))
            .line_height(px(19.0))
            .text_color(hsla(color))
            .child(text)
            .into_any_element()
    };
    let ghostex = match guide {
        None => note(GUIDE_LOADING.to_string(), c.muted),
        Some(Err(message)) => note(message.clone(), c.destructive),
        Some(Ok(text)) => {
            // Headings a step above the 13px text, and inline code as an ink wash chip, so the
            // page reads like the dialog around it in light and dark.
            let ink = if c.light { 0x000000 } else { 0xffffff };
            let mut style = gpui_component::text::TextViewStyle::default()
                .default_cursor(true)
                .paragraph_gap(gpui::rems(0.6))
                .heading_font_size(|level, _| match level {
                    1 => px(18.0),
                    2 => px(16.0),
                    3 => px(14.0),
                    _ => px(13.0),
                })
                .inline_code(gpui::HighlightStyle {
                    color: Some(hsla(c.foreground)),
                    background_color: Some(hsla(modal_rgba(ink, 0.08))),
                    ..Default::default()
                });
            style.is_dark = !c.light;
            gpui_component::text::TextView::markdown(
                SharedString::from(format!("{id}-ghostex-part")),
                text.clone(),
            )
            .parse_synchronously(true)
            .selectable(true)
            .w_full()
            .min_w_0()
            .style(style)
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(hsla(c.foreground))
            .into_any_element()
        }
    };
    let overrides = overrides.trim();
    let yours = if overrides.is_empty() {
        note(NO_OVERRIDES.to_string(), c.muted)
    } else {
        div()
            .w_full()
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(hsla(c.foreground))
            .whitespace_normal()
            .children(
                overrides
                    .lines()
                    .map(|line| div().min_h(px(20.0)).child(line.to_string())),
            )
            .into_any_element()
    };
    v_flex()
        .w_full()
        .gap(px(8.0))
        .child(title_row(GHOSTEX_PART_TITLE, GHOSTEX_PART_LABEL))
        .child(boxed(ghostex))
        .child(div().h(px(8.0)))
        .child(title_row(OVERRIDES_PART_TITLE, OVERRIDES_PART_LABEL))
        .child(boxed(yours))
        .into_any_element()
}

/// What a save changes.
pub(crate) enum PlaybookChange {
    Overrides(String),
    /// Saves every old full copy as a Markdown file and removes them.
    UseGhostex,
}

/// Where a save's outcome goes: the dialog that sent it, in its own window.
pub(crate) struct NewCoordinatorPlaybookReply {
    view: WeakEntity<GpuiNewCoordinatorModalWindow>,
    window: AnyWindowHandle,
    /// Switching to Ghostex's playbook keeps the editor open; saving overrides goes back.
    stay: bool,
}

impl NewCoordinatorPlaybookReply {
    /// Hands the outcome (gxserver's new view, or why it refused) to the dialog.
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) {
        let (view, stay) = (self.view, self.stay);
        let _ = self.window.update(cx, |_root, window, cx| {
            view.update(cx, |dialog, cx| {
                dialog.finish_playbook_change(result, stay, window, cx)
            })
        });
    }
}

/// The editor page the Edit button opens in place of the form.
pub(super) struct PlaybookEditor {
    pub(super) draft: String,
    saving: bool,
    error: Option<String>,
    /// Where Use Ghostex's Playbook saved the old copies.
    saved_note: Option<String>,
    /// Use Ghostex's Playbook was clicked once and waits for its confirming click.
    switch_armed: bool,
    /// Bumped on every arming, so an older timeout cannot disarm a newer one.
    switch_generation: u64,
}

impl GpuiNewCoordinatorModalWindow {
    /// The overrides, once the host has read them.
    pub(crate) fn set_playbook(
        &mut self,
        info: Option<OrchestratorPlaybookInfo>,
        cx: &mut Context<Self>,
    ) {
        self.playbook = info;
        cx.notify();
    }

    /// Ghostex's part of the guide, once the host has read it (or why it could not).
    pub(crate) fn set_playbook_guide(
        &mut self,
        guide: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        self.playbook_guide = Some(guide.map(|guide| guide_ghostex_part(&guide)));
        cx.notify();
    }

    /// Turns the dialog into the read-only playbook page.
    pub(crate) fn open_playbook_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.playbook.is_none() {
            return;
        }
        self.close_menus();
        self.viewing = true;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_playbook_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.viewing = false;
        self.name_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// The playbook page: its header and the scrolling read-only playbook.
    pub(super) fn render_playbook_view(&self) -> Vec<AnyElement> {
        let p = self.palette;
        let overrides = self
            .playbook
            .as_ref()
            .map(|info| info.overrides.clone())
            .unwrap_or_default();
        let colors = PlaybookViewColors {
            light: p.light,
            foreground: p.foreground,
            muted: p.muted,
            panel: p.panel,
            hairline: p.hairline,
            destructive: p.destructive,
        };
        vec![
            modal_header(&p, PLAYBOOK_VIEW_TITLE, Some(PLAYBOOK_VIEW_DESCRIPTION)),
            div()
                .id("new-coordinator-playbook-view-scroll")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(playbook_view_body(
                    colors,
                    "new-coordinator-playbook-view",
                    self.playbook_guide.as_ref(),
                    &overrides,
                ))
                .into_any_element(),
        ]
    }

    /// Back on the left; Edit overrides on the right.
    pub(super) fn render_playbook_view_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let back = modal_action_button(
            &p,
            "new-coordinator-playbook-view-back",
            BACK,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.close_playbook_view(window, cx),
            cx,
        );
        let edit = modal_action_button(
            &p,
            "new-coordinator-playbook-view-edit",
            EDIT_OVERRIDES,
            None,
            ModalButtonTone::Primary,
            false,
            |this, window, cx| {
                this.viewing = false;
                this.open_playbook_editor(window, cx)
            },
            cx,
        );
        h_flex()
            .w_full()
            .gap(px(8.0))
            .child(div().flex_1())
            .child(h_flex().w(px(96.0)).child(back))
            .child(h_flex().w(px(140.0)).child(edit))
            .into_any_element()
    }

    pub(crate) fn open_playbook_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(info) = self.playbook.clone() else {
            return;
        };
        self.close_menus();
        self.editor = Some(PlaybookEditor {
            draft: info.overrides.clone(),
            saving: false,
            error: None,
            saved_note: None,
            switch_armed: false,
            switch_generation: 0,
        });
        self.playbook_input.update(cx, |input, cx| {
            input.set_value(info.overrides, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn close_playbook_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor = None;
        self.name_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn send_playbook_change(
        &mut self,
        change: PlaybookChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.saving {
            return;
        }
        editor.saving = true;
        editor.error = None;
        let reply = NewCoordinatorPlaybookReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
            stay: matches!(change, PlaybookChange::UseGhostex),
        };
        cx.notify();
        (self.host)(
            NewCoordinatorModalCommand::SavePlaybook { change, reply },
            cx,
        );
    }

    pub(super) fn save_playbook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        let text = editor.draft.trim().to_string();
        self.send_playbook_change(PlaybookChange::Overrides(text), window, cx);
    }

    /// The first click arms Use Ghostex's Playbook; the confirming one saves the old copies as
    /// files and removes them. Leaving the button or waiting disarms it.
    fn press_use_ghostex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.switch_armed {
            editor.switch_armed = false;
            self.send_playbook_change(PlaybookChange::UseGhostex, window, cx);
            return;
        }
        editor.switch_armed = true;
        editor.switch_generation += 1;
        let generation = editor.switch_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(USE_GHOSTEX_ARM_TIMEOUT)
                .await;
            let _ = this.update(cx, |this, cx| this.disarm_use_ghostex(Some(generation), cx));
        })
        .detach();
        cx.notify();
    }

    fn disarm_use_ghostex(&mut self, generation: Option<u64>, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut()
            && editor.switch_armed
            && generation.is_none_or(|armed_at| armed_at == editor.switch_generation)
        {
            editor.switch_armed = false;
            cx.notify();
        }
    }

    /// Saved overrides go back to the form, which now shows them; switching to Ghostex's playbook
    /// stays on the editor and says where the old copies went; a refusal keeps the editor open
    /// with gxserver's message.
    fn finish_playbook_change(
        &mut self,
        result: Result<Value, String>,
        stay: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(view) => {
                if let Some(info) = OrchestratorPlaybookInfo::from_view(&view) {
                    self.playbook = Some(info);
                }
                if !stay {
                    self.close_playbook_editor(window, cx);
                    return;
                }
                if let Some(editor) = self.editor.as_mut() {
                    editor.saving = false;
                    let saved: Vec<&str> = view
                        .get("savedFiles")
                        .and_then(Value::as_array)
                        .map(|files| files.iter().filter_map(Value::as_str).collect())
                        .unwrap_or_default();
                    editor.saved_note = (!saved.is_empty()).then(|| {
                        format!(
                            "Every coordinator follows Ghostex's playbook now. The old playbook is saved in {}.",
                            saved.join(", ")
                        )
                    });
                }
                cx.notify();
            }
            Err(message) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.saving = false;
                    editor.error = Some(message);
                }
                cx.notify();
            }
        }
    }

    /// The overrides' first line (or none) and the Edit button.
    pub(super) fn render_playbook_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let info = self.playbook.as_ref()?;
        let p = self.palette;
        let lines = info
            .overrides
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        let label = if info.old_copy_in_use {
            "Old edited playbook in use".to_string()
        } else if lines > 1 {
            format!("{lines} lines")
        } else {
            String::new()
        };
        let edit = modal_action_button(
            &p,
            "new-coordinator-playbook-edit",
            EDIT,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.open_playbook_editor(window, cx),
            cx,
        );
        let view = modal_action_button(
            &p,
            "new-coordinator-playbook-view",
            VIEW_PLAYBOOK,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.open_playbook_view(window, cx),
            cx,
        );
        let summary_color = if info.overrides.is_empty() {
            p.muted
        } else {
            p.foreground
        };
        Some(
            v_flex()
                .w_full()
                .gap(px(8.0))
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .child(modal_section_title(&p, FIELD_OVERRIDES))
                        .child(
                            div()
                                .id("new-coordinator-playbook-source")
                                .text_size(px(12.0))
                                .text_color(p.muted)
                                .child(label),
                        ),
                )
                .child(
                    h_flex()
                        .id("new-coordinator-playbook")
                        .w_full()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(p.hairline)
                        .bg(p.raised)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(px(13.0))
                                .line_height(px(18.0))
                                .text_color(summary_color)
                                .child(overrides_summary(&info.overrides)),
                        )
                        .child(div().flex_shrink_0().w(px(124.0)).child(view))
                        .child(div().flex_shrink_0().w(px(64.0)).child(edit)),
                )
                .into_any_element(),
        )
    }

    /// The editor page: its header, the overrides box, and notes on old copies.
    pub(super) fn render_playbook_page(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = self.palette;
        let Some(editor) = self.editor.as_ref() else {
            return Vec::new();
        };
        let info = self.playbook.clone().unwrap_or_default();
        let mut column = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(modal_text_area(
                &p,
                &self.playbook_input,
                None,
                editor.saving,
                window,
                cx,
            ))
            .child(modal_hint(&p, OVERRIDES_HINT));
        if let Some(note) = &editor.saved_note {
            column = column.child(modal_hint(&p, note.clone()));
        } else if let Some(note) = old_copies_note(&info.old_copies) {
            column = column.child(modal_hint(&p, note));
        }
        if let Some(error) = &editor.error {
            column = column.child(modal_error(&p, error.clone()));
        }
        vec![
            modal_header(&p, OVERRIDES_TITLE, Some(OVERRIDES_DESCRIPTION.to_string())),
            column.into_any_element(),
        ]
    }

    /// Use Ghostex's Playbook on the left while old copies exist, Back and Save on the right.
    pub(super) fn render_playbook_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let Some(editor) = self.editor.as_ref() else {
            return div().into_any_element();
        };
        let has_old = self
            .playbook
            .as_ref()
            .is_some_and(|info| !info.old_copies.is_empty());
        let back = modal_action_button(
            &p,
            "new-coordinator-playbook-back",
            BACK,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.close_playbook_editor(window, cx),
            cx,
        );
        let save = modal_action_button(
            &p,
            "new-coordinator-playbook-save",
            if editor.saving { SAVING } else { SAVE },
            None,
            ModalButtonTone::Primary,
            editor.saving,
            |this, window, cx| this.save_playbook(window, cx),
            cx,
        );
        let mut footer = h_flex().w_full().gap(px(8.0));
        if has_old {
            let switch = modal_action_button(
                &p,
                "new-coordinator-playbook-use-ghostex",
                if editor.switch_armed {
                    USE_GHOSTEX_CONFIRM
                } else {
                    USE_GHOSTEX
                },
                None,
                if editor.switch_armed {
                    ModalButtonTone::Primary
                } else {
                    ModalButtonTone::Neutral
                },
                editor.saving,
                |this, window, cx| this.press_use_ghostex(window, cx),
                cx,
            );
            footer = footer.child(
                h_flex()
                    .id("new-coordinator-playbook-use-ghostex-wrap")
                    .w(px(196.0))
                    .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            this.disarm_use_ghostex(None, cx);
                        }
                    }))
                    .child(switch),
            );
        }
        footer
            .child(div().flex_1())
            .child(h_flex().w(px(96.0)).child(back))
            .child(h_flex().w(px(96.0)).child(save))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_older_gxserver_without_overrides_leaves_the_row_out() {
        assert_eq!(
            OrchestratorPlaybookInfo::from_view(&json!({ "allProjects": {} })),
            None
        );
        let info = OrchestratorPlaybookInfo::from_view(&json!({
            "overrides": { "text": " Never push without asking.\nBe brief. " },
            "source": "project",
            "oldCopies": [
                { "scope": "allProjects", "enabled": true },
                { "scope": "project", "name": "My App", "enabled": true },
            ],
        }))
        .expect("info");
        assert_eq!(info.overrides, "Never push without asking.\nBe brief.");
        assert!(info.old_copy_in_use);
        assert_eq!(info.old_copies, vec!["all projects", "My App"]);
    }

    #[test]
    fn the_row_shows_the_first_line_or_none() {
        assert!(overrides_summary("").starts_with("None."));
        assert_eq!(
            overrides_summary("\n  Never push without asking.\nBe brief."),
            "Never push without asking."
        );
        assert_eq!(old_copies_note(&[]), None);
        assert!(
            old_copies_note(&["all projects".into(), "My App".into(), "Docs".into()])
                .expect("note")
                .contains("for all projects, My App and Docs,")
        );
    }
}

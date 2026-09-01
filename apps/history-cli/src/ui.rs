use crate::model::Agent;
use crate::model::Session;
use crate::model::TranscriptBlock;
use crate::model::TranscriptKind;
use crate::scan;
use chrono::DateTime;
use chrono::Local;
use crossterm::event;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableMouseCapture;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use crossterm::execute;
use crossterm::terminal;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use ratatui::Terminal;
use std::io;
use std::io::Stdout;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
use textwrap::Options;
use unicode_width::UnicodeWidthStr;

/*
CDXC:GhostexHistoryTui 2026-06-25-19:49:
The history viewer should feel like Codex's current resume picker plus transcript overlay, with Enter opening transcripts instead of resuming sessions.
Run in alternate screen mode, keep typed search always active on the list, mirror Codex hotkeys, and render the transcript pager with the same header/bottom percent shape.

CDXC:GhostexHistoryTui 2026-06-25-19:49:
List rows should be dense enough for repeated browsing across several agents while still exposing the same core metadata Codex shows: title/preview, agent, updated time, cwd/project, and session id.

CDXC:GhostexHistoryTui 2026-06-25-21:32:
The transcript overlay should follow Codex's pager contract, not just offer raw line scrolling.
Keep Esc/Left/Right reserved for jumping among user-authored messages, close only with q/Ctrl+C/Ctrl+T, and render the Codex-style three-row hint footer under the pager bar.

CDXC:GhostexHistoryTui 2026-06-25-21:54:
Ctrl+R should resume the selected session in its owning agent, matching Zehn's command matrix while preserving Enter as transcript-open.
Leave alternate-screen/raw terminal mode before spawning the agent so resumed Claude, Codex, Pi, Cursor, or Grok sessions own a normal interactive terminal.

CDXC:GhostexHistoryTui 2026-06-25-22:03:
The Ctrl+R resume shortcut must be visible in the hotkey footer on both the session list and transcript pager.
Keep it on the primary hint row instead of the bottom-most footer row so compact Ghostex panes still advertise resume.

CDXC:GhostexHistoryTui 2026-06-25-22:11:
The session list header should match Codex's transcript-mode page chrome instead of using a cyan app-title treatment.
Render the list title and session count through the same dark slash overlay header used by the transcript pager.

CDXC:GhostexHistoryTui 2026-06-25-22:13:
The session list header should be named "Agent history" rather than "View agent history" so the first row reads like a page title, not an action label.

CDXC:GhostexHistoryTui 2026-06-25-22:21:
Transcript mode must capture wheel and trackpad scroll events instead of treating the pager as keyboard-only.
Use the same three-line wheel step as the Ghostex terminal TUI while preserving Codex-style one-line keyboard arrows, full-page keys, and half-page Ctrl+U/Ctrl+D behavior.

CDXC:GhostexHistoryTui 2026-06-25-22:29:
Left/right transcript message selection should match Codex CLI's selected user-cell styling, not a generic reversed text span.
Render user transcript blocks with Codex's user-message background shape, dim/bold prompt marker, and reversed user-message style only when a prompt is selected.
*/

const PICKER_CHROME_HEIGHT: u16 = 8;
const FOOTER_HEIGHT: u16 = 4;
const ROW_GAP_COMFORTABLE: usize = 1;
const TRANSCRIPT_HINT_HEIGHT: u16 = 3;
const MOUSE_SCROLL_LINES: usize = 3;
const LIVE_PREFIX_COLS: usize = 2;
const USER_MESSAGE_BG_DARK: (u8, u8, u8) = (30, 30, 30);

pub struct App {
    accept_all_resume: bool,
    sessions: Vec<Session>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    scroll_top: usize,
    density: Density,
    toolbar_focus: ToolbarControl,
    filter_mode: FilterMode,
    sort_key: SortKey,
    current_cwd: String,
    expanded: Option<usize>,
    screen: Screen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Density {
    Comfortable,
    Dense,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ToolbarControl {
    Filter,
    Sort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FilterMode {
    All,
    Cwd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SortKey {
    Updated,
    Created,
}

enum Screen {
    List,
    Transcript {
        session_index: usize,
        scroll: usize,
        highlight_block: Option<usize>,
    },
}

enum AppAction {
    Continue,
    Quit,
    Resume(usize),
}

pub fn run(sessions: Vec<Session>, accept_all_resume: bool) -> io::Result<()> {
    let mut terminal = enter_terminal()?;
    let current_cwd = std::env::current_dir()
        .ok()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut app = App::new(sessions, current_cwd, accept_all_resume);
    let action_result = app.run(&mut terminal);
    leave_terminal(&mut terminal)?;
    match action_result? {
        AppAction::Continue | AppAction::Quit => Ok(()),
        AppAction::Resume(session_index) => app.resume_session(session_index),
    }
}

impl App {
    fn new(sessions: Vec<Session>, current_cwd: String, accept_all_resume: bool) -> Self {
        let mut app = Self {
            accept_all_resume,
            sessions,
            filtered: Vec::new(),
            query: String::new(),
            selected: 0,
            scroll_top: 0,
            density: Density::Comfortable,
            toolbar_focus: ToolbarControl::Filter,
            filter_mode: FilterMode::All,
            sort_key: SortKey::Updated,
            current_cwd,
            expanded: None,
            screen: Screen::List,
        };
        app.apply_filter_and_sort();
        app
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<AppAction> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            if !event::poll(Duration::from_millis(250))? {
                continue;
            }
            let action = match event::read()? {
                Event::Key(key) => {
                    if matches!(key.kind, KeyEventKind::Release) {
                        AppAction::Continue
                    } else {
                        self.handle_key(key)
                    }
                }
                Event::Mouse(mouse) => self.handle_mouse(mouse),
                _ => AppAction::Continue,
            };
            match action {
                AppAction::Continue => {}
                action => return Ok(action),
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>) {
        match self.screen {
            Screen::List => self.draw_list(frame),
            Screen::Transcript {
                session_index,
                scroll,
                highlight_block,
            } => self.draw_transcript(frame, session_index, scroll, highlight_block),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match self.screen {
            Screen::List => self.handle_list_key(key),
            Screen::Transcript { .. } => self.handle_transcript_key(key),
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> AppAction {
        match self.screen {
            Screen::List => AppAction::Continue,
            Screen::Transcript { .. } => self.handle_transcript_mouse(mouse),
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> AppAction {
        if is_ctrl_char(key, 'c') {
            return AppAction::Quit;
        }
        if is_ctrl_char(key, 'r') {
            return self.resume_selected_action();
        }
        if is_ctrl_char(key, 't') || key.code == KeyCode::Enter {
            self.open_selected_transcript();
            return AppAction::Continue;
        }
        if is_ctrl_char(key, 'e') {
            self.toggle_expanded();
            return AppAction::Continue;
        }
        if is_ctrl_char(key, 'o') {
            self.toggle_density();
            return AppAction::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                if self.query.is_empty() {
                    return AppAction::Quit;
                }
                self.query.clear();
                self.apply_filter_and_sort();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.apply_filter_and_sort();
            }
            KeyCode::Tab => self.toolbar_focus = next_toolbar(self.toolbar_focus),
            KeyCode::BackTab => self.toolbar_focus = previous_toolbar(self.toolbar_focus),
            KeyCode::Left | KeyCode::Right => self.change_toolbar_value(),
            KeyCode::Up if list_plain_nav_allowed(key) => self.move_selection(-1),
            KeyCode::Down if list_plain_nav_allowed(key) => self.move_selection(1),
            KeyCode::PageUp => self.page_selection(-1),
            KeyCode::PageDown => self.page_selection(1),
            KeyCode::Home => self.jump_selection_top(),
            KeyCode::End => self.jump_selection_bottom(),
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selection(-1)
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selection(1)
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selection(-1)
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selection(1)
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.page_selection(-1)
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.page_selection(1)
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.query.push(c);
                self.apply_filter_and_sort();
            }
            _ => {}
        }
        AppAction::Continue
    }

    fn handle_transcript_key(&mut self, key: KeyEvent) -> AppAction {
        let Screen::Transcript {
            session_index,
            mut scroll,
            mut highlight_block,
        } = self.screen
        else {
            return AppAction::Continue;
        };
        let (terminal_width, area_height) = terminal::size()
            .map(|(width, height)| (width, transcript_content_height(height).max(1) as usize))
            .unwrap_or((80, 10));
        if is_ctrl_char(key, 'r') {
            return AppAction::Resume(session_index);
        }
        if is_ctrl_char(key, 'c') || is_ctrl_char(key, 't') || key.code == KeyCode::Char('q') {
            self.screen = Screen::List;
            return AppAction::Continue;
        }
        let layout = transcript_layout(
            &self.sessions[session_index].blocks,
            terminal_width,
            highlight_block,
        );
        let max_scroll = layout.lines.len().saturating_sub(area_height);
        scroll = if scroll == usize::MAX {
            max_scroll
        } else {
            scroll.min(max_scroll)
        };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => scroll = scroll.saturating_add(1),
            KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                scroll = scroll.saturating_sub(area_height)
            }
            KeyCode::PageUp => scroll = scroll.saturating_sub(area_height),
            KeyCode::PageDown | KeyCode::Char(' ') => scroll = scroll.saturating_add(area_height),
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                scroll = scroll.saturating_sub(area_height)
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                scroll = scroll.saturating_add(area_height)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                scroll = scroll.saturating_sub(area_height.saturating_add(1) / 2)
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                scroll = scroll.saturating_add(area_height.saturating_add(1) / 2)
            }
            KeyCode::Home => scroll = 0,
            KeyCode::End => scroll = usize::MAX,
            KeyCode::Esc | KeyCode::Left => {
                highlight_block = previous_user_block(&layout.user_blocks, highlight_block);
                if let Some(block_index) = highlight_block {
                    scroll = ensure_block_visible(scroll, &layout, block_index, area_height);
                } else if key.code == KeyCode::Esc {
                    self.screen = Screen::List;
                    return AppAction::Continue;
                }
            }
            KeyCode::Right => {
                highlight_block = next_user_block(&layout.user_blocks, highlight_block);
                if let Some(block_index) = highlight_block {
                    scroll = ensure_block_visible(scroll, &layout, block_index, area_height);
                }
            }
            KeyCode::Enter if highlight_block.is_some() => {
                /*
                 * CDXC:GhostexHistoryTui 2026-06-25-21:52:
                 * Codex uses Enter from a selected transcript prompt to edit the live composer.
                 * ghostex-history is a read-only cross-agent browser, so consume the key to preserve the Codex pager contract without mutating or resuming archived sessions.
                 */
            }
            _ => {}
        }
        self.screen = Screen::Transcript {
            session_index,
            scroll,
            highlight_block,
        };
        AppAction::Continue
    }

    fn handle_transcript_mouse(&mut self, mouse: MouseEvent) -> AppAction {
        if !matches!(
            mouse.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) {
            return AppAction::Continue;
        }
        let Screen::Transcript {
            session_index,
            mut scroll,
            highlight_block,
        } = self.screen
        else {
            return AppAction::Continue;
        };
        let (terminal_width, area_height) = terminal::size()
            .map(|(width, height)| (width, transcript_content_height(height).max(1) as usize))
            .unwrap_or((80, 10));
        let layout = transcript_layout(
            &self.sessions[session_index].blocks,
            terminal_width,
            highlight_block,
        );
        let max_scroll = layout.lines.len().saturating_sub(area_height);
        scroll = if scroll == usize::MAX {
            max_scroll
        } else {
            scroll.min(max_scroll)
        };
        scroll = match mouse.kind {
            MouseEventKind::ScrollUp => {
                apply_scroll_delta(scroll, -(MOUSE_SCROLL_LINES as isize), max_scroll)
            }
            MouseEventKind::ScrollDown => {
                apply_scroll_delta(scroll, MOUSE_SCROLL_LINES as isize, max_scroll)
            }
            _ => scroll,
        };
        self.screen = Screen::Transcript {
            session_index,
            scroll,
            highlight_block,
        };
        AppAction::Continue
    }

    fn draw_list(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(area.height.saturating_sub(PICKER_CHROME_HEIGHT)),
                Constraint::Length(FOOTER_HEIGHT),
            ])
            .split(area);
        let header = inner_x(chunks[0], 1);
        render_slash_header(
            frame,
            header,
            &format!(
                "{}  {} sessions",
                spaced_header_title("Agent history"),
                self.filtered.len()
            ),
        );
        let search = inner_x(chunks[2], 1);
        frame.render_widget(Paragraph::new(self.search_line(search.width)), search);
        let list = inner_x(chunks[4], 2);
        self.draw_rows(frame, list);
        self.draw_footer(frame, chunks[5], list.height);
    }

    fn draw_rows(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let content_rows = area.height as usize;
        self.ensure_selected_visible(content_rows);
        if self.filtered.is_empty() {
            frame.render_widget(
                Paragraph::new("No sessions found").style(Style::default().fg(Color::DarkGray)),
                area,
            );
            return;
        }

        let mut y = area.y;
        if self.scroll_top > 0 && y < area.bottom() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "more above",
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
        let mut used = usize::from(self.scroll_top > 0);
        for (visible_offset, session_index) in self.filtered[self.scroll_top..].iter().enumerate() {
            if used >= content_rows {
                break;
            }
            let row_index = self.scroll_top + visible_offset;
            let selected = row_index == self.selected;
            let expanded = self.expanded == Some(*session_index);
            let row_lines = self.session_row_lines(*session_index, selected, expanded, area.width);
            for line in row_lines {
                if y >= area.bottom() {
                    break;
                }
                frame.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
                y += 1;
                used += 1;
            }
            if self.density == Density::Comfortable && y < area.bottom() {
                y += ROW_GAP_COMFORTABLE as u16;
                used += ROW_GAP_COMFORTABLE;
            }
        }
        if self.has_more_below(content_rows) && area.bottom() > area.y {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "more below",
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
            );
        }
    }

    fn draw_footer(&self, frame: &mut Frame<'_>, area: Rect, list_height: u16) {
        if area.height == 0 {
            return;
        }
        let separator = "─".repeat(area.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                separator,
                Style::default().fg(Color::DarkGray),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );
        let progress = self.footer_progress(list_height);
        let progress_width = progress.width() as u16;
        if progress_width < area.width {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    progress,
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(
                    area.x + area.width - progress_width.saturating_add(1),
                    area.y,
                    progress_width,
                    1,
                ),
            );
        }
        let hints = [
            ("enter", "open"),
            ("ctrl+r", "resume"),
            (
                "esc",
                if self.query.is_empty() {
                    "quit"
                } else {
                    "clear"
                },
            ),
            ("ctrl+c", "quit"),
        ];
        let hints2 = [
            ("tab", "focus"),
            ("left/right", "option"),
            ("ctrl+t", "transcript"),
            ("ctrl+e", "preview"),
        ];
        let hints3 = [
            (
                "ctrl+o",
                match self.density {
                    Density::Comfortable => "dense",
                    Density::Dense => "comfortable",
                },
            ),
            ("pgup/pgdn", "page"),
        ];
        self.render_hint_line(frame, area, 1, &hints);
        self.render_hint_line(frame, area, 2, &hints2);
        self.render_hint_line(frame, area, 3, &hints3);
    }

    fn render_hint_line(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        offset: u16,
        hints: &[(&str, &str)],
    ) {
        let y = area.y.saturating_add(offset);
        if y >= area.bottom() {
            return;
        }
        let mut spans = vec![Span::raw(" ")];
        for (index, (key, label)) in hints.iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw("   "));
            }
            spans.push(Span::styled(*key, Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(*label, Style::default().fg(Color::DarkGray)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, y, area.width, 1),
        );
    }

    fn draw_transcript(
        &mut self,
        frame: &mut Frame<'_>,
        session_index: usize,
        scroll: usize,
        highlight_block: Option<usize>,
    ) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let Some(session) = self.sessions.get(session_index) else {
            self.screen = Screen::List;
            return;
        };
        let top_height = area.height.saturating_sub(TRANSCRIPT_HINT_HEIGHT);
        let top_area = Rect::new(area.x, area.y, area.width, top_height);
        let hint_area = Rect::new(
            area.x,
            area.y.saturating_add(top_height),
            area.width,
            area.height.saturating_sub(top_height),
        );
        let content_area = Rect::new(
            top_area.x,
            top_area.y.saturating_add(1),
            top_area.width,
            top_area.height.saturating_sub(2),
        );
        render_slash_header(
            frame,
            Rect::new(top_area.x, top_area.y, top_area.width, 1),
            "T R A N S C R I P T",
        );

        let layout = transcript_layout(&session.blocks, content_area.width, highlight_block);
        let max_scroll = layout
            .lines
            .len()
            .saturating_sub(content_area.height as usize);
        let scroll = if scroll == usize::MAX {
            max_scroll
        } else {
            scroll.min(max_scroll)
        };
        self.screen = Screen::Transcript {
            session_index,
            scroll,
            highlight_block,
        };
        for (row, line) in layout
            .lines
            .iter()
            .skip(scroll)
            .take(content_area.height as usize)
            .enumerate()
        {
            frame.render_widget(
                Paragraph::new(line.clone()),
                Rect::new(
                    content_area.x,
                    content_area.y + row as u16,
                    content_area.width,
                    1,
                ),
            );
        }
        let drawn_rows = layout.lines.len().saturating_sub(scroll) as u16;
        for y in content_area.y + drawn_rows.min(content_area.height)..content_area.bottom() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "~",
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(content_area.x, y, content_area.width, 1),
            );
        }
        self.draw_transcript_bottom(top_area, frame, scroll, max_scroll);
        self.draw_transcript_hints(frame, hint_area, highlight_block.is_some());
    }

    fn draw_transcript_bottom(
        &self,
        area: Rect,
        frame: &mut Frame<'_>,
        scroll: usize,
        max_scroll: usize,
    ) {
        let y = area.bottom().saturating_sub(1);
        let separator = "─".repeat(area.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                separator,
                Style::default().fg(Color::DarkGray),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        let percent = if max_scroll == 0 {
            100
        } else {
            ((scroll.min(max_scroll) as f32 / max_scroll as f32) * 100.0).round() as u8
        };
        let label = format!(" {percent}% ");
        let label_width = label.width() as u16;
        if label_width < area.width {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    label,
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(
                    area.x + area.width - label_width.saturating_add(1),
                    y,
                    label_width,
                    1,
                ),
            );
        }
    }

    fn draw_transcript_hints(&self, frame: &mut Frame<'_>, area: Rect, highlight_active: bool) {
        self.render_hint_line(
            frame,
            area,
            0,
            &[
                ("ctrl+r", "to resume"),
                ("↑/↓", "to scroll"),
                ("pgup/pgdn", "to page"),
                ("home/end", "to jump"),
            ],
        );
        let mut hints = vec![("q", "to quit")];
        if highlight_active {
            hints.push(("esc/←", "to edit prev"));
            hints.push(("→", "to edit next"));
            hints.push(("enter", "to edit message"));
        } else {
            hints.push(("esc", "to edit prev"));
        }
        self.render_hint_line(frame, area, 1, &hints);
    }

    fn search_line(&self, width: u16) -> Line<'static> {
        let search_text = if self.query.is_empty() {
            Span::styled("Type to search", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw(format!("Search: {}", self.query))
        };
        let toolbar = self.toolbar_line(width);
        let search_width = search_text.content.width();
        let toolbar_width = toolbar.width();
        let spacer = width
            .saturating_sub((search_width + toolbar_width) as u16)
            .max(2) as usize;
        let mut spans = vec![search_text, Span::raw(" ".repeat(spacer))];
        spans.extend(toolbar.spans);
        Line::from(spans)
    }

    fn toolbar_line(&self, _width: u16) -> Line<'static> {
        let mut spans = Vec::new();
        spans.push(Span::styled(
            "Filter: ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(toolbar_value(
            "Cwd",
            self.filter_mode == FilterMode::Cwd,
            self.toolbar_focus == ToolbarControl::Filter,
        ));
        spans.push(toolbar_value(
            "All",
            self.filter_mode == FilterMode::All,
            self.toolbar_focus == ToolbarControl::Filter,
        ));
        spans.push(Span::raw("   "));
        spans.push(Span::styled("Sort: ", Style::default().fg(Color::DarkGray)));
        spans.push(toolbar_value(
            "Updated",
            self.sort_key == SortKey::Updated,
            self.toolbar_focus == ToolbarControl::Sort,
        ));
        spans.push(toolbar_value(
            "Created",
            self.sort_key == SortKey::Created,
            self.toolbar_focus == ToolbarControl::Sort,
        ));
        Line::from(spans)
    }

    fn session_row_lines(
        &self,
        session_index: usize,
        selected: bool,
        expanded: bool,
        width: u16,
    ) -> Vec<Line<'static>> {
        let session = &self.sessions[session_index];
        let marker_style = if selected {
            selected_session_style().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let indicator = match (selected, expanded) {
            (true, true) => "⌄ ",
            (true, false) => "❯ ",
            (false, _) => "  ",
        };
        let title_width = width.saturating_sub(18) as usize;
        let title = truncate_display(session.display_title(), title_width);
        let mut lines = vec![
            Line::from(vec![
                Span::styled(indicator, marker_style),
                Span::styled(agent_label(session.agent), agent_style(session.agent)),
                Span::raw("  "),
                Span::styled(
                    title,
                    if selected {
                        selected_session_style()
                    } else {
                        Style::default()
                    },
                ),
            ]),
            self.session_meta_line(session, selected),
        ];
        if selected {
            lines = apply_session_row_style(lines, selected_session_style(), width);
        }
        if expanded {
            lines.extend(preview_lines(&session.blocks, width.saturating_sub(4)));
        }
        lines
    }

    fn session_meta_line(&self, session: &Session, selected: bool) -> Line<'static> {
        let date = format_timestamp(session.updated_at);
        let project = if session.project.is_empty() {
            session.path.to_string_lossy().to_string()
        } else {
            session.project.clone()
        };
        let style = if selected {
            selected_session_style()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Line::from(vec![
            Span::raw("    "),
            Span::styled(date, style),
            Span::raw("  "),
            Span::styled(truncate_display(&project, 64), style),
            Span::raw("  "),
            Span::styled(truncate_display(&session.id, 32), style),
        ])
    }

    fn footer_progress(&self, list_height: u16) -> String {
        let position = if self.filtered.is_empty() {
            0
        } else {
            self.selected + 1
        };
        let percent = self.list_percent(list_height as usize);
        format!(" {position}/{} - {percent}% ", self.filtered.len())
    }

    fn list_percent(&self, list_height: usize) -> u8 {
        if self.filtered.is_empty() {
            return 100;
        }
        let max_scroll = self.filtered.len().saturating_sub(list_height.max(1));
        if max_scroll == 0 {
            return 100;
        }
        ((self.scroll_top.min(max_scroll) as f32 / max_scroll as f32) * 100.0).round() as u8
    }

    fn open_selected_transcript(&mut self) {
        let Some(session_index) = self.filtered.get(self.selected).copied() else {
            return;
        };
        if !self.sessions[session_index].transcript_loaded {
            if let Ok(blocks) = scan::load_transcript(&self.sessions[session_index]) {
                self.sessions[session_index].blocks = blocks;
                self.sessions[session_index].transcript_loaded = true;
            }
        }
        self.screen = Screen::Transcript {
            session_index,
            scroll: usize::MAX,
            highlight_block: None,
        };
    }

    fn resume_selected_action(&self) -> AppAction {
        self.filtered
            .get(self.selected)
            .copied()
            .map(AppAction::Resume)
            .unwrap_or(AppAction::Continue)
    }

    fn resume_session(&self, session_index: usize) -> io::Result<()> {
        let Some(session) = self.sessions.get(session_index) else {
            return Ok(());
        };
        resume_agent_session(session, self.accept_all_resume)
    }

    fn toggle_expanded(&mut self) {
        let Some(session_index) = self.filtered.get(self.selected).copied() else {
            return;
        };
        self.expanded = if self.expanded == Some(session_index) {
            None
        } else {
            Some(session_index)
        };
    }

    fn toggle_density(&mut self) {
        self.density = match self.density {
            Density::Comfortable => Density::Dense,
            Density::Dense => Density::Comfortable,
        };
    }

    fn change_toolbar_value(&mut self) {
        match self.toolbar_focus {
            ToolbarControl::Filter => {
                self.filter_mode = match self.filter_mode {
                    FilterMode::All => FilterMode::Cwd,
                    FilterMode::Cwd => FilterMode::All,
                };
                self.apply_filter_and_sort();
            }
            ToolbarControl::Sort => {
                self.sort_key = match self.sort_key {
                    SortKey::Updated => SortKey::Created,
                    SortKey::Created => SortKey::Updated,
                };
                self.apply_filter_and_sort();
            }
        }
    }

    fn apply_filter_and_sort(&mut self) {
        self.filtered = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| self.filter_session(session))
            .map(|(index, _)| index)
            .collect();
        let sessions = &self.sessions;
        let sort_key = self.sort_key;
        self.filtered.sort_by(|a, b| {
            let left = &sessions[*a];
            let right = &sessions[*b];
            match sort_key {
                SortKey::Updated => right.updated_at.cmp(&left.updated_at),
                SortKey::Created => right
                    .created_at
                    .unwrap_or(right.updated_at)
                    .cmp(&left.created_at.unwrap_or(left.updated_at)),
            }
            .then_with(|| left.agent.label().cmp(right.agent.label()))
            .then_with(|| left.display_title().cmp(right.display_title()))
        });
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        self.scroll_top = self.scroll_top.min(self.filtered.len().saturating_sub(1));
    }

    fn filter_session(&self, session: &Session) -> bool {
        if self.filter_mode == FilterMode::Cwd
            && (self.current_cwd.is_empty() || session.project != self.current_cwd)
        {
            return false;
        }
        self.query.is_empty() || session.matches_query(&self.query)
    }

    fn move_selection(&mut self, direction: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let max = self.filtered.len().saturating_sub(1);
        self.selected = if direction.is_negative() {
            self.selected.saturating_sub(direction.unsigned_abs())
        } else {
            self.selected.saturating_add(direction as usize).min(max)
        };
    }

    fn page_selection(&mut self, direction: isize) {
        let step = 10;
        self.move_selection(direction * step);
    }

    fn jump_selection_top(&mut self) {
        self.selected = 0;
    }

    fn jump_selection_bottom(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self.filtered.len() - 1;
        }
    }

    fn ensure_selected_visible(&mut self, height: usize) {
        if self.filtered.is_empty() {
            self.scroll_top = 0;
            return;
        }
        let height = height.max(1);
        if self.selected < self.scroll_top {
            self.scroll_top = self.selected;
        } else if self.selected >= self.scroll_top + height {
            self.scroll_top = self.selected.saturating_sub(height - 1);
        }
    }

    fn has_more_below(&self, height: usize) -> bool {
        self.scroll_top + height < self.filtered.len()
    }
}

fn render_slash_header(frame: &mut Frame<'_>, area: Rect, title: &str) {
    let header_bg = "/ ".repeat(area.width as usize / 2);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            header_bg,
            Style::default().fg(Color::DarkGray),
        ))),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("/ {title}"),
            Style::default().fg(Color::DarkGray),
        ))),
        area,
    );
}

fn spaced_header_title(title: &str) -> String {
    title
        .split_whitespace()
        .map(|word| {
            word.to_ascii_uppercase()
                .chars()
                .map(|ch| ch.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

struct TranscriptLayout {
    lines: Vec<Line<'static>>,
    block_starts: Vec<usize>,
    user_blocks: Vec<usize>,
}

fn transcript_layout(
    blocks: &[TranscriptBlock],
    width: u16,
    highlight_block: Option<usize>,
) -> TranscriptLayout {
    let width = width.max(8) as usize;
    let mut lines = Vec::new();
    let mut block_starts = Vec::with_capacity(blocks.len());
    let mut user_blocks = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        block_starts.push(lines.len());
        if block.kind == TranscriptKind::User {
            user_blocks.push(index);
        }
        let highlighted = highlight_block == Some(index);
        lines.extend(transcript_block_lines(block, width, highlighted));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No transcript content available",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
    }
    TranscriptLayout {
        lines,
        block_starts,
        user_blocks,
    }
}

fn transcript_block_lines(
    block: &TranscriptBlock,
    width: usize,
    highlighted: bool,
) -> Vec<Line<'static>> {
    match block.kind {
        TranscriptKind::User => user_transcript_lines(&block.text, width, highlighted),
        TranscriptKind::Thinking => styled_transcript_lines(
            &block.text,
            width,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
        TranscriptKind::Tool => {
            styled_transcript_lines(&block.text, width, Style::default().fg(Color::DarkGray))
        }
        TranscriptKind::Agent => styled_transcript_lines(&block.text, width, Style::default()),
    }
}

fn user_transcript_lines(text: &str, width: usize, highlighted: bool) -> Vec<Line<'static>> {
    let wrap_width = width.saturating_sub(LIVE_PREFIX_COLS + 1).max(1);
    let style = user_message_style(highlighted);
    let mut out = vec![Line::from("").style(style)];
    for (index, line) in wrap_text(text, wrap_width).into_iter().enumerate() {
        let prefix = if index == 0 { "› " } else { "  " };
        out.push(
            Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default().add_modifier(Modifier::BOLD | Modifier::DIM),
                ),
                Span::styled(line, style),
            ])
            .style(style),
        );
    }
    out.push(Line::from("").style(style));
    out
}

fn styled_transcript_lines(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    wrap_text(text, width)
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style)))
        .collect()
}

fn user_message_style(highlighted: bool) -> Style {
    let style = Style::default().bg(Color::Rgb(
        USER_MESSAGE_BG_DARK.0,
        USER_MESSAGE_BG_DARK.1,
        USER_MESSAGE_BG_DARK.2,
    ));
    if highlighted {
        style.add_modifier(Modifier::REVERSED)
    } else {
        style
    }
}

fn previous_user_block(user_blocks: &[usize], current: Option<usize>) -> Option<usize> {
    if user_blocks.is_empty() {
        return None;
    }
    let Some(current) = current else {
        return user_blocks.last().copied();
    };
    let position = user_blocks
        .iter()
        .position(|block| *block == current)
        .unwrap_or_else(|| {
            user_blocks
                .iter()
                .position(|block| *block > current)
                .unwrap_or(user_blocks.len())
        });
    user_blocks
        .get(position.saturating_sub(1))
        .copied()
        .or_else(|| user_blocks.first().copied())
}

fn next_user_block(user_blocks: &[usize], current: Option<usize>) -> Option<usize> {
    if user_blocks.is_empty() {
        return None;
    }
    let Some(current) = current else {
        return user_blocks.last().copied();
    };
    let position = user_blocks
        .iter()
        .position(|block| *block == current)
        .unwrap_or_else(|| {
            user_blocks
                .iter()
                .position(|block| *block > current)
                .unwrap_or(user_blocks.len().saturating_sub(1))
        });
    user_blocks
        .get(position.saturating_add(1))
        .copied()
        .or_else(|| user_blocks.last().copied())
}

fn ensure_block_visible(
    scroll: usize,
    layout: &TranscriptLayout,
    block_index: usize,
    height: usize,
) -> usize {
    let Some(&first) = layout.block_starts.get(block_index) else {
        return scroll;
    };
    let last = layout
        .block_starts
        .iter()
        .enumerate()
        .skip(block_index + 1)
        .find_map(|(_, start)| Some(*start))
        .unwrap_or(layout.lines.len())
        .saturating_sub(1);
    let height = height.max(1);
    let current_bottom = scroll.saturating_add(height.saturating_sub(1));
    if first < scroll {
        first
    } else if last > current_bottom {
        last.saturating_sub(height.saturating_sub(1))
    } else {
        scroll
    }
}

fn apply_scroll_delta(scroll: usize, delta: isize, max_scroll: usize) -> usize {
    if delta.is_negative() {
        scroll.saturating_sub(delta.unsigned_abs()).min(max_scroll)
    } else {
        scroll.saturating_add(delta as usize).min(max_scroll)
    }
}

fn transcript_content_height(viewport_height: u16) -> u16 {
    viewport_height
        .saturating_sub(TRANSCRIPT_HINT_HEIGHT)
        .saturating_sub(2)
}

fn selected_session_style() -> Style {
    Style::default().fg(Color::Yellow)
}

fn apply_session_row_style(
    lines: Vec<Line<'static>>,
    style: Style,
    width: u16,
) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|mut line| {
            let padding = (width as usize).saturating_sub(line.width());
            if padding > 0 {
                line.spans.push(Span::styled(" ".repeat(padding), style));
            }
            line.style = line.style.patch(style);
            line
        })
        .collect()
}

fn preview_lines(blocks: &[TranscriptBlock], width: u16) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut recent = blocks
        .iter()
        .rev()
        .filter(|block| block.kind != TranscriptKind::Tool)
        .take(4)
        .collect::<Vec<_>>();
    recent.reverse();
    for block in recent {
        let label = format!("{}: ", block.kind.label());
        let text = truncate_display(
            &clean_one_line(&block.text),
            width.saturating_sub(label.width() as u16) as usize,
        );
        out.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(label, Style::default().fg(Color::DarkGray)),
            Span::styled(text, Style::default().fg(Color::Gray)),
        ]));
    }
    out
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let options = Options::new(width).break_words(false);
    let mut out = Vec::new();
    for paragraph in text.lines() {
        if paragraph.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        out.extend(
            textwrap::wrap(paragraph, options.clone())
                .into_iter()
                .map(|line| line.into_owned()),
        );
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn resume_agent_session(session: &Session, accept_all: bool) -> io::Result<()> {
    if session.id.trim().is_empty() {
        println!(
            "ghostex-history: no session id recorded for this {} session",
            session.agent
        );
        return Ok(());
    }

    let argv = resume_argv(session.agent, &session.id, accept_all);
    let project = session.project.trim();
    let project_dir = (!project.is_empty() && Path::new(project).is_dir()).then_some(project);

    if let Some(project_dir) = project_dir {
        println!(
            "\x1b[90m-> resuming {} session {} in {}\x1b[0m",
            session.agent, session.id, project_dir
        );
    } else if project.is_empty() {
        println!(
            "\x1b[90m-> resuming {} session {}\x1b[0m",
            session.agent, session.id
        );
    } else {
        println!(
            "\x1b[90m-> resuming {} session {} (project {} missing - using current dir)\x1b[0m",
            session.agent, session.id, project
        );
    }
    io::stdout().flush()?;

    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(project_dir) = project_dir {
        command.current_dir(project_dir);
    }
    if let Err(error) = command.status() {
        println!("ghostex-history: failed to launch {} ({error})", argv[0]);
        println!("Run manually:");
        if !project.is_empty() {
            println!("  cd {}", shell_quote(project));
        }
        println!("  {}", shell_join(&argv));
    }
    Ok(())
}

fn resume_argv(agent: Agent, session: &str, accept_all: bool) -> Vec<String> {
    let session = session.to_string();
    match (agent, accept_all) {
        (Agent::Claude, _) => vec![
            "claude".into(),
            "--dangerously-skip-permissions".into(),
            "--resume".into(),
            session,
        ],
        (Agent::Codex, _) => vec!["codex".into(), "--yolo".into(), "resume".into(), session],
        (Agent::Cursor, true) => vec![
            "cursor-agent".into(),
            "--yolo".into(),
            "--resume".into(),
            session,
        ],
        (Agent::Grok, true) => vec![
            "grok".into(),
            "--permission-mode".into(),
            "bypassPermissions".into(),
            "--resume".into(),
            session,
        ],
        (Agent::Pi, true) | (Agent::Pi, false) => vec!["pi".into(), "--session".into(), session],
        (Agent::Cursor, false) => vec!["cursor-agent".into(), "--resume".into(), session],
        (Agent::Grok, false) => vec!["grok".into(), "--resume".into(), session],
    }
}

fn shell_join(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(ch, '-' | '_' | '.' | '/' | ':' | '=' | '+' | ',')
        })
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn enter_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn leave_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()
}

fn is_ctrl_char(key: KeyEvent, c: char) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch == c)
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn list_plain_nav_allowed(key: KeyEvent) -> bool {
    !matches!(key.code, KeyCode::Char(_))
}

fn next_toolbar(control: ToolbarControl) -> ToolbarControl {
    match control {
        ToolbarControl::Filter => ToolbarControl::Sort,
        ToolbarControl::Sort => ToolbarControl::Filter,
    }
}

fn previous_toolbar(control: ToolbarControl) -> ToolbarControl {
    next_toolbar(control)
}

fn toolbar_value(label: &'static str, active: bool, focused: bool) -> Span<'static> {
    if active {
        let style = if focused {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        };
        Span::styled(format!("[{label}]"), style)
    } else {
        Span::styled(format!(" {label} "), Style::default().fg(Color::DarkGray))
    }
}

fn inner_x(area: Rect, pad: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(pad),
        area.y,
        area.width.saturating_sub(pad.saturating_mul(2)),
        area.height,
    )
}

fn agent_label(agent: Agent) -> String {
    format!("{:<6}", agent.label())
}

fn agent_style(agent: Agent) -> Style {
    let color = match agent {
        Agent::Claude => Color::Rgb(255, 136, 76),
        Agent::Codex => Color::Rgb(88, 166, 255),
        Agent::Cursor => Color::Rgb(139, 154, 255),
        Agent::Grok => Color::Rgb(115, 231, 156),
        Agent::Pi => Color::Rgb(248, 173, 7),
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn format_timestamp(ts: i64) -> String {
    DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&Local).format("%b %-d %H:%M").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn clean_one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_display(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let next_width = out.width() + ch.to_string().width();
        if next_width + 3 > width {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(kind: TranscriptKind, text: &str) -> TranscriptBlock {
        TranscriptBlock::new(kind, text, None).expect("test transcript block")
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn transcript_layout_indexes_user_blocks_and_uses_codex_prompt_prefix() {
        let layout = transcript_layout(
            &[
                block(TranscriptKind::User, "first prompt"),
                block(TranscriptKind::Agent, "first answer"),
                block(TranscriptKind::User, "second prompt"),
            ],
            80,
            None,
        );

        assert_eq!(layout.block_starts, vec![0, 4, 6]);
        assert_eq!(layout.user_blocks, vec![0, 2]);
        assert_eq!(line_text(&layout.lines[1]), "› first prompt");
        assert_eq!(line_text(&layout.lines[7]), "› second prompt");
    }

    #[test]
    fn codex_message_jump_helpers_select_latest_then_pin_at_edges() {
        let user_blocks = [0, 2, 5];

        assert_eq!(previous_user_block(&user_blocks, None), Some(5));
        assert_eq!(previous_user_block(&user_blocks, Some(5)), Some(2));
        assert_eq!(previous_user_block(&user_blocks, Some(0)), Some(0));
        assert_eq!(next_user_block(&user_blocks, None), Some(5));
        assert_eq!(next_user_block(&user_blocks, Some(2)), Some(5));
        assert_eq!(next_user_block(&user_blocks, Some(5)), Some(5));
    }

    #[test]
    fn selected_user_transcript_lines_use_codex_reversed_user_cell_style() {
        let lines = user_transcript_lines("selected prompt", 80, true);
        let style = user_message_style(true);

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].style, style);
        assert_eq!(line_text(&lines[1]), "› selected prompt");
        assert_eq!(lines[1].style, style);
        assert_eq!(lines[2].style, style);
        assert!(lines[1].style.add_modifier.contains(Modifier::REVERSED));
        assert!(lines[1].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD | Modifier::DIM));
    }

    #[test]
    fn scroll_delta_matches_mouse_wheel_step_and_clamps() {
        assert_eq!(apply_scroll_delta(20, -3, 100), 17);
        assert_eq!(apply_scroll_delta(20, 3, 100), 23);
        assert_eq!(apply_scroll_delta(1, -3, 100), 0);
        assert_eq!(apply_scroll_delta(99, 3, 100), 100);
        assert_eq!(apply_scroll_delta(140, -3, 100), 100);
    }

    #[test]
    fn resume_argv_matches_zehn_agent_commands() {
        assert_eq!(
            resume_argv(Agent::Codex, "codex-session", false),
            ["codex", "--yolo", "resume", "codex-session"]
        );
        assert_eq!(
            resume_argv(Agent::Codex, "codex-session", true),
            ["codex", "--yolo", "resume", "codex-session"]
        );
        assert_eq!(
            resume_argv(Agent::Claude, "claude-session", true),
            [
                "claude",
                "--dangerously-skip-permissions",
                "--resume",
                "claude-session"
            ]
        );
        assert_eq!(
            resume_argv(Agent::Pi, "pi-session", true),
            ["pi", "--session", "pi-session"]
        );
        assert_eq!(
            resume_argv(Agent::Cursor, "cursor-session", true),
            ["cursor-agent", "--yolo", "--resume", "cursor-session"]
        );
        assert_eq!(
            resume_argv(Agent::Grok, "grok-session", true),
            [
                "grok",
                "--permission-mode",
                "bypassPermissions",
                "--resume",
                "grok-session"
            ]
        );
    }

    #[test]
    fn list_header_title_uses_codex_spaced_style() {
        assert_eq!(
            spaced_header_title("Agent history"),
            "A G E N T  H I S T O R Y"
        );
    }
}

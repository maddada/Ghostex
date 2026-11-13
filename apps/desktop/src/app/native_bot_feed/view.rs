//! The feed page: the channel list on the left (bot filter, sort toggle, `#all`, the groups with
//! their jobs' channels, + New group, then the jobs in no group), the selected channel's runs on
//! the right, newest at the bottom. Right-clicking a job's channel moves it to a group or out of
//! one; right-clicking a group renames or deletes it.

use super::model::{
    BotFeedJob, FeedRow, channel_slug, feed_bots, feed_rows, parse_feed, run_clock,
};
use super::storage::{read_feed_prefs, write_feed_prefs};
use crate::GhostexGpuiApp;
use crate::app::consts::{
    COMMAND_ICON_CHEVRON_RIGHT, TITLEBAR_ICON_CHEVRON_DOWN, TITLEBAR_ICON_MESSAGES,
};
use crate::app::context_menu::GpuiContextMenu;
use crate::app::gx_store::gx_rpc;
use crate::app::helpers::{
    CHROME_LIGHT_APPEARANCE, glass_clear, gpui_open_external_http_url, workspace_background_color,
    workspace_tab_agent_icon_accent_color,
};
use crate::app::model::TitlebarMode;
use crate::app::native_automate::{AutomatePalette, ICON_ALERT, empty_state, icon};
use crate::app::native_chat::appearance::ChatAppearance;
use crate::app::native_chat::markdown_style::{highlight_theme, text_style};
use crate::app::native_chat::queue::grip;
use crate::app::view_skeletons::{ViewSkeletonKind, render_view_skeleton};
use crate::app::window::native_modal_kit::capture_child_bounds;
use crate::notification_feed::notification_feed_badge_label;
use chrono::NaiveDate;
use ghostex_gx_core::bot_feed::{
    FeedChannel, FeedChannelKind, FeedPrefs, FeedSort, ShownChannels, channel_unread,
    feed_channels, feed_runs, resolve_selection, shown_channels,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Entity, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, ListAlignment, ListOffset, ListState, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, Point, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, WeakEntity, Window, div,
    list, px, rgb,
};
use gpui_component::input::{Escape, Input, InputEvent, InputState};
use gpui_component::text::{TextView, TextViewStyle};
use gpui_component::{h_flex, v_flex};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// How often the showing feed reads the runs again. Hermes cron jobs run a minute apart at most.
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Everything the page paints with, resolved once per render instead of once per message.
struct FeedStyle {
    palette: AutomatePalette,
    markdown: TextViewStyle,
    chat: ChatAppearance,
}

impl FeedStyle {
    /// The Automate page's palette and the chat's Markdown style, so runs read like chat messages.
    fn resolve(glass: bool) -> Self {
        let chat = ChatAppearance::current(&serde_json::Value::Null).on_window_glass(glass);
        let mut markdown = text_style(&chat);
        markdown.is_dark = !chat.light;
        markdown.highlight_theme = highlight_theme(chat.light);
        Self {
            palette: AutomatePalette::resolve(glass),
            markdown,
            chat,
        }
    }
}

/// A pick in the bot filter menu; `None` is every bot.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct FilterBotFeed {
    bot: Option<String>,
}

/// Move to group… on a job channel's right-click menu: opens the list of groups for this job where
/// the right-click landed.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct ShowMoveMenu {
    job: String,
    position: Point<Pixels>,
}

/// A pick in the list of groups.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct MoveFeedJob {
    job: String,
    group: String,
}

/// Remove from group on a job channel's right-click menu.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct RemoveFeedJob {
    job: String,
}

/// Rename group… on a group's right-click menu.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct RenameFeedGroup {
    group: String,
}

/// Delete group on a group's right-click menu.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct DeleteFeedGroup {
    group: String,
}

/// Hide #all on its right-click menu, or Show #all on the Feeds title's.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct ShowAllChannel {
    show: bool,
}

/// What the name field names: a new group, drawn where + New group is, or the group keyed here,
/// drawn in its row.
#[derive(Clone, PartialEq)]
enum NamingTarget {
    New,
    Rename(String),
}

struct ChannelNaming {
    target: NamingTarget,
    input: Entity<InputState>,
    _subscription: Subscription,
}

/// What the channel list and the header paint for one channel, made in `refresh` rather than per
/// frame, since the page redraws on every scroll of its message list.
struct ChannelLabel {
    key: SharedString,
    slug: SharedString,
    /// Beside the name: a group's job count, or while every bot is listed a loose job's bot.
    detail: Option<SharedString>,
    unread: usize,
    /// The header's line: a job's bot and schedule, or a group's bots and jobs.
    header: SharedString,
}

/// A channel being dragged into a new place in the manual order, drawn as its `#name` pill. It
/// lands only on a channel of the same kind (`FeedPrefs::move_channel`).
#[derive(Clone)]
struct ChannelDrag {
    key: SharedString,
    kind: FeedChannelKind,
    slug: SharedString,
}

impl Render for ChannelDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crate::app::helpers::drag_ghost(format!("# {}", self.slug), None, None, 1.0)
    }
}

pub(crate) struct NativeBotFeedView {
    app: WeakEntity<GhostexGpuiApp>,
    /// `None` until the first load answers, while the page shows its skeleton. gxserver's order,
    /// newest activity first.
    jobs: Option<Vec<BotFeedJob>>,
    /// The jobs in their channels (`feed_channels`), rebuilt when the jobs or the grouping change.
    channels: Vec<FeedChannel>,
    /// One per channel, in the same order.
    labels: Vec<ChannelLabel>,
    /// The channels the list shows, in its order (`shown_channels`).
    shown: ShownChannels,
    prefs: FeedPrefs,
    naming: Option<ChannelNaming>,
    /// The selected channel's key; `None` is `#all`.
    selected: Option<SharedString>,
    rows: Vec<FeedRow>,
    /// The day `rows` were labelled on, so "Today" moves on at midnight.
    rows_day: NaiveDate,
    list: ListState,
    /// Keeps the channel list's place while the feed is closed: an untracked scroll lives only in
    /// the last drawn frame.
    channel_scroll: ScrollHandle,
    /// The bot filter's bounds, which its menu opens below.
    bot_trigger: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The page holds focus while its menu is open, so the pick is dispatched to it.
    focus: FocusHandle,
    /// Why the first load failed; a later failure keeps the runs already shown instead.
    error: Option<String>,
    glass: bool,
    style: FeedStyle,
    load_generation: u64,
    _refresh_task: Task<()>,
}

impl NativeBotFeedView {
    /// The page, reading the runs every [`REFRESH_INTERVAL`] while it is the view showing, so a
    /// closed feed reads nothing; `host.rs` reads them again the moment it opens.
    pub(crate) fn new(app: WeakEntity<GhostexGpuiApp>, cx: &mut Context<Self>) -> Self {
        let refresh_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH_INTERVAL).await;
                let alive = this.update(cx, |this, cx| {
                    let shown = this
                        .app
                        .upgrade()
                        .is_some_and(|app| app.read(cx).bot_feed_showing());
                    if shown {
                        this.load(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            app,
            jobs: None,
            channels: Vec::new(),
            labels: Vec::new(),
            shown: ShownChannels::default(),
            prefs: read_feed_prefs(),
            naming: None,
            selected: None,
            rows: Vec::new(),
            rows_day: chrono::Local::now().date_naive(),
            list: ListState::new(0, ListAlignment::Bottom, px(600.0)),
            channel_scroll: ScrollHandle::new(),
            bot_trigger: Rc::new(Cell::new(None)),
            focus: cx.focus_handle(),
            error: None,
            glass: false,
            style: FeedStyle::resolve(false),
            load_generation: 0,
            _refresh_task: refresh_task,
        }
    }

    /// True when the page must be redrawn because the window glass changed.
    pub(crate) fn sync(&mut self, glass: bool) -> bool {
        std::mem::replace(&mut self.glass, glass) != glass
    }

    /// Reads the feed again; the page keeps what it shows until the answer arrives.
    pub(crate) fn load(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/listBotFeed", serde_json::json!({}))
                .await
                .map_err(|error| error.to_string())
                .and_then(parse_feed);
            let _ = this.update(cx, |this, cx| {
                if this.load_generation != generation {
                    return;
                }
                match result {
                    Ok(jobs) => this.apply(jobs, cx),
                    Err(error) if this.jobs.is_none() => {
                        this.error = Some(error);
                        cx.notify();
                    }
                    Err(_) => {}
                }
            });
        })
        .detach();
    }

    /// Takes a fresh answer. An unchanged feed redraws nothing; a changed one keeps the reader's
    /// place in the runs unless they sit at the newest, where the new runs then show.
    fn apply(&mut self, jobs: Vec<BotFeedJob>, cx: &mut Context<Self>) {
        let today = chrono::Local::now().date_naive();
        if self.jobs.as_ref() == Some(&jobs) && self.rows_day == today {
            return;
        }
        let anchor = self.scroll_anchor();
        self.jobs = Some(jobs);
        self.error = None;
        let placed = self.regroup();
        let started = self
            .prefs
            .start_reading(self.jobs.as_deref().unwrap_or_default());
        self.refresh(anchor, placed | started, cx);
        cx.notify();
    }

    /// Puts the jobs in their channels again, and in Manual order places the channels it has not
    /// placed yet at the bottom. True when that changed the saved order.
    fn regroup(&mut self) -> bool {
        self.channels = feed_channels(self.jobs(), &self.prefs);
        self.prefs.sort == FeedSort::Manual && self.prefs.place_new_channels(&self.channels)
    }

    /// What each row and the header say, once the channels, the bot filter or the read runs changed.
    fn relabel(&mut self) {
        self.labels = self
            .channels
            .iter()
            .map(|channel| {
                let shown: Vec<usize> = self.prefs.shown_jobs(self.jobs(), channel).collect();
                ChannelLabel {
                    key: channel.key.clone().into(),
                    slug: channel_slug(channel),
                    detail: match &channel.kind {
                        FeedChannelKind::Group => Some(match shown.len() {
                            1 => "1 job".into(),
                            count => format!("{count} jobs").into(),
                        }),
                        FeedChannelKind::Job { group: None } if self.prefs.bot.is_none() => {
                            Some(self.jobs()[channel.jobs[0]].bot_name.to_lowercase().into())
                        }
                        FeedChannelKind::Job { .. } => None,
                    },
                    unread: channel_unread(self.jobs(), channel, &self.prefs),
                    header: self.channel_detail(channel, &shown).into(),
                }
            })
            .collect();
    }

    /// Orders the channels the list shows again.
    fn reshow(&mut self) {
        self.shown = shown_channels(self.jobs(), &self.channels, &self.prefs);
    }

    fn jobs(&self) -> &[BotFeedJob] {
        self.jobs.as_deref().unwrap_or_default()
    }

    fn channel_index(&self, key: &str) -> Option<usize> {
        self.channels.iter().position(|channel| channel.key == key)
    }

    fn selected_index(&self) -> Option<usize> {
        self.channel_index(self.selected.as_ref()?)
    }

    fn is_shown(&self, key: &str) -> bool {
        self.channel_index(key)
            .is_some_and(|index| self.shown.rows.contains(&index))
    }

    /// The group whose name field is open.
    fn renaming(&self) -> Option<&str> {
        match &self.naming.as_ref()?.target {
            NamingTarget::Rename(group) => Some(group),
            NamingTarget::New => None,
        }
    }

    fn bot_name(&self, profile: &str) -> SharedString {
        self.jobs()
            .iter()
            .find(|job| job.profile == profile)
            .map_or_else(|| profile.to_string().into(), |job| job.bot_name.clone())
    }

    /// These jobs' bots by name, each once, in the jobs' order.
    fn bots_of(&self, jobs: &[usize]) -> Vec<SharedString> {
        let mut bots: Vec<SharedString> = Vec::new();
        for &job in jobs {
            let bot = &self.jobs()[job].bot_name;
            if !bots.contains(bot) {
                bots.push(bot.clone());
            }
        }
        bots
    }

    /// The row at the top of the list and how far into it, or `None` while the list sits at its
    /// newest run (or cannot scroll), where a reload keeps showing the newest.
    fn scroll_anchor(&self) -> Option<(SharedString, Pixels)> {
        if self.list.is_scrolled_to_end() != Some(false) {
            return None;
        }
        let top = self.list.logical_scroll_top();
        Some((
            self.rows.get(top.item_ix)?.key(self.jobs()),
            top.offset_in_item,
        ))
    }

    /// Rebuilds the list and the message rows, reads the selected channel's runs, and saves the
    /// feed's state once when `save` (the caller changed it) or the reading did. The runs start at
    /// the newest unless `anchor` names a row to stay on.
    fn refresh(&mut self, anchor: Option<(SharedString, Pixels)>, save: bool, cx: &gpui::App) {
        self.rows_day = chrono::Local::now().date_naive();
        self.reshow();
        self.selected = resolve_selection(
            &self.channels,
            &self.shown,
            &self.prefs,
            self.selected.as_deref(),
        )
        .map(Into::into);
        if self.renaming().is_some_and(|group| !self.is_shown(group)) {
            self.naming = None;
        }
        let read = self.prefs.read_channel(
            self.jobs.as_deref().unwrap_or_default(),
            &self.channels,
            self.selected.as_deref(),
        );
        if save || read {
            write_feed_prefs(&self.prefs, cx);
        }
        self.relabel();
        let runs = feed_runs(
            self.jobs(),
            &self.channels,
            self.selected.as_deref(),
            &self.prefs,
        );
        self.rows = feed_rows(self.jobs(), runs, self.rows_day);
        self.list.reset(self.rows.len());
        if let Some((key, offset_in_item)) = anchor
            && let Some(item_ix) = self.rows.iter().position(|row| row.key(self.jobs()) == key)
        {
            self.list.scroll_to(ListOffset {
                item_ix,
                offset_in_item,
            });
        }
    }

    fn select(&mut self, selected: Option<SharedString>, cx: &mut Context<Self>) {
        if self.selected == selected {
            return;
        }
        self.selected = selected;
        self.refresh(None, false, cx);
        cx.notify();
    }

    fn set_sort(&mut self, sort: FeedSort, cx: &mut Context<Self>) {
        if self.prefs.sort == sort {
            return;
        }
        self.prefs.sort = sort;
        // The first switch to Manual starts from the newest-activity order the list showed.
        if sort == FeedSort::Manual {
            self.prefs.place_new_channels(&self.channels);
        }
        write_feed_prefs(&self.prefs, cx);
        self.reshow();
        cx.notify();
    }

    fn filter_by_bot(
        &mut self,
        action: &FilterBotFeed,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prefs.bot == action.bot {
            return;
        }
        self.prefs.bot = action.bot.clone();
        self.refresh(None, true, cx);
        cx.notify();
    }

    fn show_bot_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(trigger) = self.bot_trigger.get() else {
            return;
        };
        let choice =
            |bot: Option<String>| -> Box<dyn gpui::Action> { Box::new(FilterBotFeed { bot }) };
        let all = GpuiContextMenu::new().menu_with_check(
            "All bots",
            self.prefs.bot.is_none(),
            choice(None),
        );
        let menu = feed_bots(self.jobs())
            .into_iter()
            .fold(all, |menu, (profile, name)| {
                let checked = self.prefs.bot.as_ref() == Some(&profile);
                menu.menu_with_check(name, checked, choice(Some(profile)))
            });
        self.focus.focus(window, cx);
        menu.toggle_below(trigger, window, cx);
    }

    fn move_channel(&mut self, moved: &str, target: &str, cx: &mut Context<Self>) {
        if self.prefs.move_channel(moved, target, &self.channels) {
            write_feed_prefs(&self.prefs, cx);
            self.reshow();
            cx.notify();
        }
    }

    /// Right-click on a channel: a job's channel offers Move to group…, and Remove from group while
    /// it is in one; a group offers Rename group… and Delete group.
    ///
    /// CDXC:Bots 2026-09-27 WHY:
    /// The list of groups is a second menu rather than a submenu: the shared menu's popup window is only as big as the menu, and a submenu drawn beside it never showed (Sven's try-it). Go back to a submenu once the shared menu can show one.
    fn show_channel_menu(
        &mut self,
        key: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.channel_index(key) else {
            return;
        };
        let key = key.to_string();
        let menu = match &self.channels[index].kind {
            FeedChannelKind::Group => GpuiContextMenu::new()
                .menu(
                    "Rename group…",
                    Box::new(RenameFeedGroup { group: key.clone() }),
                )
                .menu("Delete group", Box::new(DeleteFeedGroup { group: key })),
            FeedChannelKind::Job { group } => {
                let menu = GpuiContextMenu::new().menu(
                    "Move to group…",
                    Box::new(ShowMoveMenu {
                        job: key.clone(),
                        position,
                    }),
                );
                match group {
                    Some(_) => menu.menu("Remove from group", Box::new(RemoveFeedJob { job: key })),
                    None => menu,
                }
            }
        };
        self.show_menu(menu, position, window, cx);
    }

    /// Opens a right-click menu; the page holds focus while it is open, so the pick is dispatched
    /// to it.
    fn show_menu(
        &mut self,
        menu: GpuiContextMenu,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        menu.show(position, window, cx);
    }

    /// Every group but the one the job is in, or a pointer to + New group when there is none.
    fn show_move_menu(
        &mut self,
        action: &ShowMoveMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let groups: Vec<usize> = self
            .prefs
            .move_targets(&action.job)
            .filter_map(|group| self.channel_index(&group.key))
            .collect();
        let menu = if groups.is_empty() {
            let hint = if self.prefs.groups.is_empty() {
                "Make a group with + New group first"
            } else {
                "No other group: make one with + New group"
            };
            GpuiContextMenu::new().menu_with_disabled(hint, true, Box::new(gpui::NoAction {}))
        } else {
            groups
                .into_iter()
                .fold(GpuiContextMenu::new(), |menu, index| {
                    menu.menu(
                        format!("# {}", self.labels[index].slug),
                        Box::new(MoveFeedJob {
                            job: action.job.clone(),
                            group: self.channels[index].key.clone(),
                        }),
                    )
                })
        };
        self.show_menu(menu, action.position, window, cx);
    }

    fn move_feed_job(&mut self, action: &MoveFeedJob, _: &mut Window, cx: &mut Context<Self>) {
        if self.prefs.move_job(&action.job, &action.group) {
            self.save_grouping(cx);
        }
    }

    fn remove_feed_job(&mut self, action: &RemoveFeedJob, _: &mut Window, cx: &mut Context<Self>) {
        if self.prefs.remove_job(&action.job) {
            self.save_grouping(cx);
        }
    }

    fn rename_feed_group(
        &mut self,
        action: &RenameFeedGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_naming(NamingTarget::Rename(action.group.clone()), window, cx);
    }

    fn delete_feed_group(
        &mut self,
        action: &DeleteFeedGroup,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prefs.delete_group(&action.group) {
            self.save_grouping(cx);
        }
    }

    /// Hide #all on `#all`'s right-click; on the Feeds title's, Show #all, ticked while it shows.
    fn show_all_channel(
        &mut self,
        action: &ShowAllChannel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prefs.hide_all != action.show {
            return;
        }
        self.prefs.hide_all = !action.show;
        self.refresh(None, true, cx);
        cx.notify();
    }

    /// Puts the jobs in their channels again after the grouping changed, and saves it.
    fn save_grouping(&mut self, cx: &mut Context<Self>) {
        let anchor = self.scroll_anchor();
        self.regroup();
        self.refresh(anchor, true, cx);
        cx.notify();
    }

    /// A group's arrow.
    fn toggle_group(&mut self, group: &str, cx: &mut Context<Self>) {
        self.prefs.toggle_collapsed(group);
        let anchor = self.scroll_anchor();
        self.refresh(anchor, true, cx);
        cx.notify();
    }

    /// The name field of + New group, or of Rename group… in the group's row. Enter, or clicking
    /// away, saves the name; Escape or a blank name changes nothing.
    fn begin_naming(&mut self, target: NamingTarget, window: &mut Window, cx: &mut Context<Self>) {
        // A right-click leaves shell focus on the Agents pane, and the root sends typed keys to
        // whatever shell focus names, so the page takes shell focus before its field takes keys.
        let _ = self.app.update(cx, |app, cx| {
            app.focus_project_editor_surface(TitlebarMode::BotFeed, window, cx);
        });
        let current = match &target {
            NamingTarget::Rename(group) => self
                .channel_index(group)
                .map(|index| self.channels[index].name.clone()),
            NamingTarget::New => None,
        };
        let input = cx.new(|cx| {
            let input = InputState::new(window, cx).placeholder("new-group");
            match current {
                Some(name) => input.default_value(name),
                None => input,
            }
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    this.finish_naming(true, cx);
                }
            },
        );
        input.update(cx, |input, cx| input.focus(window, cx));
        self.naming = Some(ChannelNaming {
            target,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// A new group is selected once it is made.
    fn finish_naming(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some(naming) = self.naming.take() else {
            return;
        };
        if save {
            let name = naming.input.read(cx).value().to_string();
            match naming.target {
                NamingTarget::New => {
                    if let Some(group) = self.prefs.make_group(&name) {
                        self.selected = Some(group.into());
                        self.save_grouping(cx);
                    }
                }
                NamingTarget::Rename(group) => {
                    if self.prefs.rename_group(&group, &name) {
                        self.save_grouping(cx);
                    }
                }
            }
        }
        cx.notify();
    }

    fn render_bot_filter(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.style.palette;
        let label = match &self.prefs.bot {
            Some(profile) => self.bot_name(profile),
            None => "All bots".into(),
        };
        h_flex()
            .on_children_prepainted(capture_child_bounds(self.bot_trigger.clone(), 0))
            .child(
                h_flex()
                    .id("bot-feed-bot-filter")
                    .role(gpui::Role::Button)
                    .aria_label("Filter by bot")
                    .h(px(24.0))
                    .px(px(6.0))
                    .gap(px(4.0))
                    .rounded(px(6.0))
                    .text_size(px(12.0))
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(|this| this.bg(p.hover))
                    .on_click(cx.listener(|this, _, window, cx| this.show_bot_menu(window, cx)))
                    .child(div().max_w(px(110.0)).truncate().child(label))
                    .child(icon(TITLEBAR_ICON_CHEVRON_DOWN, 12.0, p.muted)),
            )
            .into_any_element()
    }

    /// "Newest activity" and "Manual", drawn like the Automate page's tabs.
    fn render_sort_toggle(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.style.palette;
        let hover = p.foreground.opacity(0.8);
        h_flex()
            .gap(px(2.0))
            .children(
                [
                    (FeedSort::Activity, "Newest activity"),
                    (FeedSort::Manual, "Manual"),
                ]
                .map(|(sort, label)| {
                    let active = self.prefs.sort == sort;
                    div()
                        .id(("bot-feed-sort", sort as usize))
                        .role(gpui::Role::Button)
                        .aria_selected(active)
                        .h(px(24.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .when(active, |this| this.bg(p.selected).text_color(p.foreground))
                        .when(!active, |this| {
                            this.text_color(p.muted)
                                .hover(move |this| this.text_color(hover))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.set_sort(sort, cx)))
                        .child(label)
                }),
            )
            .into_any_element()
    }

    /// `#all` (no channel) or one channel: a group with its arrow and job count, a job's channel
    /// indented under its group, or a job's channel in no group with its bot. In Manual order a
    /// channel shows its grip and can be dragged onto another of its kind to take its place.
    fn render_channel_row(&self, channel: Option<usize>, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.style.palette;
        let label = channel.map(|index| &self.labels[index]);
        let kind = channel.map(|index| self.channels[index].kind.clone());
        let key = label.map(|label| label.key.clone());
        let slug = label.map_or(SharedString::new_static("all"), |label| label.slug.clone());
        let unread = label.map_or(0, |label| label.unread);
        let detail = label.and_then(|label| label.detail.clone());
        let group = key.clone().filter(|_| kind == Some(FeedChannelKind::Group));
        let nested = matches!(kind, Some(FeedChannelKind::Job { group: Some(_) }));
        let selected = self.selected == key;
        let draggable = key
            .clone()
            .zip(kind)
            .filter(|_| self.prefs.sort == FeedSort::Manual);
        let slug_for_drag = slug.clone();
        let drop_highlight = p.selected;
        h_flex()
            .id(SharedString::from(format!(
                "bot-feed-channel-{}",
                key.as_deref().unwrap_or("all")
            )))
            .role(gpui::Role::Button)
            .aria_label(format!("#{slug}"))
            .aria_selected(selected)
            .h(px(28.0))
            .px(px(8.0))
            .when(nested, |row| row.pl(px(30.0)))
            .gap(px(6.0))
            .rounded(px(6.0))
            .cursor_default()
            .hover(|row| row.bg(p.hover))
            .when(selected, |row| row.bg(p.selected))
            .when(draggable.is_some(), |row| row.child(grip(&self.style.chat)))
            .when_some(group, |row, group| {
                row.child(self.render_group_arrow(group, cx))
            })
            .child(div().text_color(p.muted).child("#"))
            // A channel with unread runs reads bold, one read and not selected reads muted.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(unread > 0, |name| name.font_weight(FontWeight::SEMIBOLD))
                    .when(unread == 0 && !selected, |name| name.text_color(p.muted))
                    .child(slug.clone()),
            )
            .when_some(detail, |row, detail| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .text_color(p.muted)
                        .child(detail),
                )
            })
            .when(unread > 0, |row| row.child(unread_pill(p, unread)))
            .on_mouse_down(MouseButton::Right, {
                let key = key.clone();
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    match &key {
                        Some(key) => this.show_channel_menu(key, event.position, window, cx),
                        None => this.show_menu(
                            GpuiContextMenu::new()
                                .menu("Hide #all", Box::new(ShowAllChannel { show: false })),
                            event.position,
                            window,
                            cx,
                        ),
                    }
                })
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(key.clone(), cx)))
            .when_some(draggable, |row, (target, kind)| {
                let drag = ChannelDrag {
                    key: target.clone(),
                    kind: kind.clone(),
                    slug: slug_for_drag,
                };
                row.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .drag_over::<ChannelDrag>(move |style, drag, _, _| {
                        if drag.kind == kind {
                            style.bg(drop_highlight)
                        } else {
                            style
                        }
                    })
                    .on_drop(cx.listener(move |this, drag: &ChannelDrag, _, cx| {
                        this.move_channel(&drag.key, &target, cx);
                    }))
            })
            .into_any_element()
    }

    /// A group's arrow: pointing right while the group is collapsed, down while it is open.
    fn render_group_arrow(&self, group: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let collapsed = self.prefs.is_collapsed(&group);
        div()
            .id(SharedString::from(format!("bot-feed-group-arrow-{group}")))
            .role(gpui::Role::Button)
            .aria_label(if collapsed {
                "Expand group"
            } else {
                "Collapse group"
            })
            .flex_shrink_0()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_group(&group, cx);
            }))
            .child(
                icon(COMMAND_ICON_CHEVRON_RIGHT, 12.0, self.style.palette.muted)
                    .with_transformation(gpui::Transformation::rotate(gpui::percentage(
                        if collapsed { 0.0 } else { 0.25 },
                    ))),
            )
            .into_any_element()
    }

    /// The name field of + New group or Rename group…, drawn like a channel row.
    fn render_naming_row(&self, naming: &ChannelNaming, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.style.palette;
        h_flex()
            .h(px(28.0))
            .px(px(8.0))
            .gap(px(6.0))
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                cx.stop_propagation();
                this.finish_naming(false, cx);
            }))
            .child(div().text_color(p.muted).child("#"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&naming.input).h(px(24.0))),
            )
            .into_any_element()
    }

    fn render_new_group_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.style.palette;
        h_flex()
            .id("bot-feed-new-group")
            .role(gpui::Role::Button)
            .h(px(28.0))
            .px(px(8.0))
            .gap(px(6.0))
            .rounded(px(6.0))
            .text_color(p.muted)
            .cursor_pointer()
            .hover(|row| row.bg(p.hover))
            .on_click(cx.listener(|this, _, window, cx| {
                this.begin_naming(NamingTarget::New, window, cx);
            }))
            .child("+")
            .child("New group")
            .into_any_element()
    }

    fn render_channels(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.style.palette;
        let (shown, loose) = (&self.shown.rows, self.shown.loose);
        let mut rows = Vec::with_capacity(shown.len() + 4);
        if !self.prefs.hide_all {
            rows.push(self.render_channel_row(None, cx));
        }
        let renaming = self.renaming();
        for &index in &shown[..loose] {
            rows.push(match &self.naming {
                Some(naming) if renaming == Some(self.channels[index].key.as_str()) => {
                    self.render_naming_row(naming, cx)
                }
                _ => self.render_channel_row(Some(index), cx),
            });
        }
        rows.push(match &self.naming {
            Some(naming) if naming.target == NamingTarget::New => {
                self.render_naming_row(naming, cx)
            }
            _ => self.render_new_group_row(cx),
        });
        if loose < shown.len() {
            rows.push(
                div()
                    .h(px(1.0))
                    .mx(px(8.0))
                    .my(px(6.0))
                    .bg(p.border)
                    .into_any_element(),
            );
        }
        for &index in &shown[loose..] {
            rows.push(self.render_channel_row(Some(index), cx));
        }
        v_flex()
            .w(px(240.0))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(p.border)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .px(px(8.0))
                    .pt(px(10.0))
                    .pb(px(6.0))
                    .gap(px(6.0))
                    .child(
                        h_flex()
                            .h(px(28.0))
                            .pl(px(8.0))
                            .gap(px(6.0))
                            .child(icon(TITLEBAR_ICON_MESSAGES, 14.0, p.muted))
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            let hidden = this.prefs.hide_all;
                                            let menu = GpuiContextMenu::new().menu_with_check(
                                                "Show #all",
                                                !hidden,
                                                Box::new(ShowAllChannel { show: hidden }),
                                            );
                                            this.show_menu(menu, event.position, window, cx);
                                        }),
                                    )
                                    .child("Feeds"),
                            )
                            .child(self.render_bot_filter(cx)),
                    )
                    .child(self.render_sort_toggle(cx)),
            )
            .child(
                v_flex()
                    .id("bot-feed-channels")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.channel_scroll)
                    .px(px(8.0))
                    .pb(px(8.0))
                    .gap(px(2.0))
                    .children(rows),
            )
            .into_any_element()
    }

    fn render_channel_header(&self) -> AnyElement {
        let p = &self.style.palette;
        let (slug, detail) = match self.selected_index() {
            Some(index) => (
                self.labels[index].slug.clone(),
                self.labels[index].header.clone(),
            ),
            None => (
                "all".into(),
                match &self.prefs.bot {
                    Some(profile) => format!("Every run from {}", self.bot_name(profile)).into(),
                    None => "Every run from every bot".into(),
                },
            ),
        };
        h_flex()
            .h(px(44.0))
            .flex_shrink_0()
            .px(px(16.0))
            .gap(px(10.0))
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("# {slug}")),
            )
            .child(div().min_w_0().truncate().text_color(p.muted).child(detail))
            .into_any_element()
    }

    /// A job's own channel: its bot and schedule. A group: the bots and names of `shown`, its jobs
    /// the bot filter shows.
    fn channel_detail(&self, channel: &FeedChannel, shown: &[usize]) -> String {
        match (&channel.kind, shown) {
            (FeedChannelKind::Job { .. }, [job]) => {
                let job = &self.jobs()[*job];
                match &job.schedule {
                    Some(schedule) => format!("{} · {schedule}", job.bot_name),
                    None => job.bot_name.to_string(),
                }
            }
            (_, []) => "No jobs yet".to_string(),
            (_, jobs) => {
                let bots = self.bots_of(jobs);
                let bots: Vec<&str> = bots.iter().map(AsRef::as_ref).collect();
                let names: Vec<&str> = jobs
                    .iter()
                    .map(|&job| self.jobs()[job].name.as_str())
                    .collect();
                format!("{} · {}", bots.join(", "), names.join(", "))
            }
        }
    }

    fn render_row(&self, index: usize) -> AnyElement {
        let p = &self.style.palette;
        match self.rows.get(index) {
            Some(FeedRow::Day { label, .. }) => h_flex()
                .px(px(16.0))
                .pt(px(14.0))
                .pb(px(6.0))
                .gap(px(10.0))
                .text_size(px(11.0))
                .text_color(p.muted)
                .child(div().flex_1().h(px(1.0)).bg(p.border))
                .child(label.clone())
                .child(div().flex_1().h(px(1.0)).bg(p.border))
                .into_any_element(),
            Some(FeedRow::Run(entry)) => {
                let channel = &self.channels[entry.channel];
                let job = &self.jobs()[entry.job];
                let run = &job.runs[entry.run];
                let meta =
                    |text: SharedString| div().text_size(px(12.0)).text_color(p.muted).child(text);
                let header = h_flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(job.bot_name.clone()),
                    )
                    .when(self.selected.is_none(), |header| {
                        header.child(meta(format!("#{}", self.labels[entry.channel].slug).into()))
                    })
                    // A group says which of its jobs ran.
                    .when(channel.kind == FeedChannelKind::Group, |header| {
                        header.child(meta(job.name.clone().into()))
                    })
                    .child(meta(run_clock(&run.run_at).to_string().into()))
                    .when(run.failed, |header| {
                        header.child(
                            div()
                                .px(px(6.0))
                                .rounded(px(4.0))
                                .text_size(px(11.0))
                                .text_color(p.danger)
                                .bg(p.danger.opacity(0.14))
                                .child("failed"),
                        )
                    });
                h_flex()
                    .items_start()
                    .px(px(16.0))
                    .py(px(6.0))
                    .gap(px(12.0))
                    .child(bot_tile(&job.bot_name))
                    .child(v_flex().flex_1().min_w_0().gap(px(2.0)).child(header).when(
                        !run.body.is_empty(),
                        |column| {
                            column.child(
                                TextView::markdown(run.id.clone(), run.body.clone())
                                    .min_w_0()
                                    .max_w(gpui::relative(1.0))
                                    // Cron output is agent-written, so only web links open, and
                                    // only on a primary click.
                                    .on_link_click(|href, event, _, _| {
                                        if event.standard_click() {
                                            let _ = gpui_open_external_http_url(href);
                                        }
                                    })
                                    .selectable(true)
                                    .style(self.style.markdown.clone())
                                    .text_size(px(14.0))
                                    .line_height(px(22.0))
                                    .text_color(self.style.chat.prose),
                            )
                        },
                    ))
                    .into_any_element()
            }
            None => div().into_any_element(),
        }
    }

    fn render_messages(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.style.palette;
        let body = if self.rows.is_empty() {
            // Every job has a delivered run, so an empty list with jobs is an empty group or a bot
            // filter whose bot has none left.
            let empty_group = self
                .selected_index()
                .is_some_and(|index| self.channels[index].jobs.is_empty());
            let (title, description) = if self.jobs().is_empty() {
                (
                    "No cron runs yet",
                    "Runs from every Hermes profile's cron jobs show up here once a job delivers one.",
                )
            } else if empty_group {
                (
                    "No jobs in this group yet",
                    "Right-click a job's channel and pick Move to group… to add it here.",
                )
            } else {
                (
                    "No runs from this bot",
                    "Its runs show up here once one of its cron jobs delivers one.",
                )
            };
            empty_state(p, TITLEBAR_ICON_MESSAGES, title, description, None)
        } else {
            list(
                self.list.clone(),
                cx.processor(|this, index, _window, _cx| this.render_row(index)),
            )
            .flex_1()
            .min_h_0()
            .w_full()
            .pb(px(12.0))
            .into_any_element()
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(self.render_channel_header())
            .child(body)
            .into_any_element()
    }
}

/// A channel's unread runs on an accent pill, capped at 99+ like the notification bell's.
fn unread_pill(p: &AutomatePalette, unread: usize) -> AnyElement {
    div()
        .flex_shrink_0()
        .min_w(px(16.0))
        .h(px(16.0))
        .px(px(5.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(p.accent.opacity(0.18))
        .text_color(p.accent)
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(notification_feed_badge_label(unread))
        .into_any_element()
}

/// The bot's first letter on a Hermes-yellow square, the bot row's tile at message size.
fn bot_tile(bot_name: &str) -> AnyElement {
    div()
        .size(px(32.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(rgb(workspace_tab_agent_icon_accent_color("hermes-agent")))
        .text_color(rgb(0x111111))
        .font_weight(FontWeight::SEMIBOLD)
        .child(
            bot_name
                .trim()
                .chars()
                .next()
                .map(|letter| letter.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_owned()),
        )
        .into_any_element()
}

impl Render for NativeBotFeedView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Same skeleton and background as the Automate page, whose layout the feed shares.
        if self.jobs.is_none() && self.error.is_none() {
            let light = CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
            return render_view_skeleton(
                ViewSkeletonKind::Automate,
                "view-skeleton-bot-feed",
                glass_clear(if light {
                    rgb(0xffffff).into()
                } else {
                    workspace_background_color()
                }),
            );
        }
        self.style = FeedStyle::resolve(self.glass);
        let p = &self.style.palette;
        let page = h_flex()
            .id("bot-feed")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::filter_by_bot))
            .on_action(cx.listener(Self::show_move_menu))
            .on_action(cx.listener(Self::move_feed_job))
            .on_action(cx.listener(Self::remove_feed_job))
            .on_action(cx.listener(Self::rename_feed_group))
            .on_action(cx.listener(Self::delete_feed_group))
            .on_action(cx.listener(Self::show_all_channel))
            .size_full()
            .items_start()
            .bg(p.page)
            .font_family(p.font.clone())
            .text_size(px(13.0))
            .text_color(p.foreground);
        match &self.error {
            Some(error) => page
                .child(empty_state(
                    p,
                    ICON_ALERT,
                    "The feed could not be read",
                    error.clone(),
                    None,
                ))
                .into_any_element(),
            None => page
                .child(self.render_channels(cx))
                .child(self.render_messages(cx))
                .into_any_element(),
        }
    }
}

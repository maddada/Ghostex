// Keep-alive for projects the user just left: the terminals, chat pages, and
// view page that were visible when a project was parked stay running for the
// Settings-owned window instead of being released on the switch. With Keep views
// loaded on, every page of the project's own views (Code, its windows, Files) stays
// until its view is put to sleep.

use crate::app::model::*;
use crate::*;
use std::time::{Duration, Instant};

/// A Code, Kanban, Automate, or Docs page that was visible when its project was left, kept loaded
/// until the same runtime URL asks for it again or the keep-alive window ends.
pub(crate) struct ParkedProjectWorkareaSurface {
    pub(crate) owned: ProjectWorkareaRuntimeCefSurface,
    pub(crate) slot_key: ProjectWorkareaCefSurfaceSlotKey,
    pub(crate) parked_at: Instant,
    /// The project that was left, when the page was parked on a project switch.
    pub(crate) project_id: Option<String>,
    /// The page a background Space opens on, which outlives the keep-alive window
    /// (`refresh_space_kept_parked_workarea_surfaces`).
    pub(crate) space_kept: bool,
    /// A page of the project's own views, parked while Keep views loaded is on: kept for that
    /// project while the setting stays on, until its view is put to sleep or its Auto Sleep time
    /// passes.
    pub(crate) kept_loaded: bool,
}

impl ParkedProjectWorkareaSurface {
    fn kept(&self, keep: Option<Duration>, keep_views_loaded: bool) -> bool {
        self.space_kept
            || (self.kept_loaded && keep_views_loaded)
            || project_keep_alive_active(Some(self.parked_at), keep)
    }
}

/// True while `parked_at` is still inside the keep-alive window; `None` for either means no keep-alive.
pub(crate) fn project_keep_alive_active(
    parked_at: Option<Instant>,
    keep: Option<Duration>,
) -> bool {
    match (parked_at, keep) {
        (Some(parked_at), Some(keep)) => parked_at.elapsed() < keep,
        _ => false,
    }
}

/// Whether a parked page may go back to `wanted`: any page of the same address, except that an
/// extra Code window's page belongs to that window alone.
///
/// CDXC:CodeEditor 2026-10-25 WHY:
/// Every empty Code window has the same address (`/?ew=true`) and two windows can have the same folder open, in one project or in two. Matched by address alone, a project's new window took another project's live page, and the page's own messages (title, address, File > Close Window) still named the window it was made for. A Code window's id is never reused, so the slot is the page's owner; the project's own Code page and its windows never take each other's page either.
fn parked_page_is_for_slot(
    parked: ProjectWorkareaCefSurfaceSlotKey,
    wanted: ProjectWorkareaCefSurfaceSlotKey,
) -> bool {
    let code_window = |slot: ProjectWorkareaCefSurfaceSlotKey| {
        matches!(slot, ProjectWorkareaCefSurfaceSlotKey::CodeWindow(_))
    };
    if code_window(parked) || code_window(wanted) {
        parked == wanted
    } else {
        true
    }
}

fn keep_alive_remaining(parked_at: Option<Instant>, keep: Option<Duration>) -> Option<Duration> {
    let keep = keep?;
    keep.checked_sub(parked_at?.elapsed())
        .filter(|remaining| !remaining.is_zero())
}

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-19 DECISION:
    /// User: switching Spaces (which switches projects) must keep the previous project live in the background instead of closing and relaunching it on every swap, for a slider-chosen number of minutes.
    /// Only what was visible when the project was left is kept: its visible terminal viewers, visible chat pages, and the open Code/Kanban/Automate/Docs page. Hidden tabs keep the 2026-09-13 release rules, and 0 minutes restores those rules for everything.
    pub(crate) fn project_switch_keep_alive(&self) -> Option<Duration> {
        shared_settings::shared_sidebar_settings_snapshot().project_switch_keep_alive()
    }

    /// The Agents terminal viewers currently on screen, captured before a project switch parks them.
    pub(crate) fn agents_terminal_keep_alive_viewer_sessions(&self) -> HashSet<TerminalSessionId> {
        if self.project_switch_keep_alive().is_none() {
            return HashSet::new();
        }
        self.agents_gpui_engine_terminals
            .keys()
            .copied()
            .filter(|id| self.agents_terminal_viewer_is_visible(*id))
            .collect()
    }

    pub(crate) fn park_project_workarea_runtime_cef_surface(
        &mut self,
        owned: ProjectWorkareaRuntimeCefSurface,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        was_visible: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.park_project_workarea_page(owned, slot_key, was_visible) {
            self.ensure_project_keep_alive_expiry_scheduled(cx);
        }
    }

    /// Parks a page by the keep-alive rules without arming the expiry timer; `true` when it went to
    /// the timed list.
    fn park_project_workarea_page(
        &mut self,
        owned: ProjectWorkareaRuntimeCefSurface,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        was_visible: bool,
    ) -> bool {
        if owned.runtime_url.project_view_key.is_some() {
            self.park_custom_project_view(owned);
            return false;
        }
        if !was_visible || self.project_switch_keep_alive().is_none() {
            return false;
        }
        let project_id = self.agents_workspace_project_id.clone();
        self.push_parked_project_workarea_surface(owned, slot_key, project_id, false);
        true
    }

    /// Adds a page to the parked list for `project_id`. A page held for Keep views loaded
    /// (`kept_loaded`) replaces the one held earlier for the same project and slot, so a project
    /// keeps at most one page per slot however long it stays away, and never takes the place of
    /// another project's held page.
    fn push_parked_project_workarea_surface(
        &mut self,
        owned: ProjectWorkareaRuntimeCefSurface,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        project_id: Option<String>,
        kept_loaded: bool,
    ) {
        self.parked_project_workarea_surfaces.retain(|parked| {
            let same_owner = parked.project_id == project_id;
            let same_page = (same_owner || !(kept_loaded || parked.kept_loaded))
                && parked.owned.matches_runtime_url(&owned.runtime_url)
                && parked_page_is_for_slot(parked.slot_key, slot_key);
            let replaced_hold =
                kept_loaded && parked.kept_loaded && same_owner && parked.slot_key == slot_key;
            !(same_page || replaced_hold)
        });
        self.parked_project_workarea_surfaces
            .push(ParkedProjectWorkareaSurface {
                owned,
                slot_key,
                parked_at: Instant::now(),
                project_id,
                space_kept: false,
                kept_loaded,
            });
    }

    /// CDXC:Workarea 2026-09-27 WHY:
    /// The page on screen is parked here, before the incoming project's view state is applied. The ordinary prune runs after that swap, when `active_mode` already belongs to the incoming project, so it asked the incoming project whether the page was visible: the page the user left was closed whenever the other project showed a different view, and coming back rebuilt it behind its skeleton. With keep-alive off the prune keeps its old rules.
    /// CDXC:Workarea 2026-09-30 DECISION:
    /// User: the page a background Space opens on is not slept, so swiping between Spaces shows it instantly (CDXC:Spaces 2026-09-30). It stays past the keep-alive slider for as long as that Space opens on it; every other parked page, the side panel's active view included, still closes when the slider's minutes run out, and 0 minutes parks nothing. Supersedes the 2026-09-27 rule that the slider was the limit for every parked page.
    /// CDXC:Workarea 2026-10-26 WHY:
    /// With Keep views loaded on, every page of the project's own views (Code, its windows, Files) is parked here, the hidden ones too, and waits for that project whatever the slider says: an opt-in exception to keeping only the page on screen (CDXC:Workarea 2026-09-19) and to the slider's limit above, which stay the default. Website views go to their project as before. An app-wide page (Work, an installed extension, a website with one address) stays where it is, even on screen: its address does not change with the project, so the switch keeps it, and it goes on receiving its context and bridge messages.
    pub(crate) fn park_project_workarea_surfaces_before_switch(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let keep_views_loaded = self.project_editor_auto_sleep_policy.keep_views_loaded;
        if !keep_views_loaded && self.project_switch_keep_alive().is_none() {
            return;
        }
        let leaving = self
            .project_workarea_runtime_cef_surfaces
            .iter()
            .filter(|(slot, owned)| {
                if keep_views_loaded {
                    !owned.is_app_wide(**slot)
                } else {
                    self.project_workarea_runtime_cef_surface_may_be_visible(**slot)
                }
            })
            .map(|(slot, _)| *slot)
            .collect::<Vec<_>>();
        for slot_key in leaving {
            let Some(owned) = self.project_workarea_runtime_cef_surfaces.remove(&slot_key) else {
                continue;
            };
            owned
                .surface
                .update(cx, |surface, _| surface.set_visible(false));
            if keep_views_loaded && slot_key.holds_project_page() {
                let project_id = self.agents_workspace_project_id.clone();
                self.push_parked_project_workarea_surface(owned, slot_key, project_id, true);
            } else {
                self.park_project_workarea_page(owned, slot_key, true);
            }
        }
        self.ensure_project_keep_alive_expiry_scheduled(cx);
    }

    pub(crate) fn take_parked_project_workarea_runtime_cef_surface(
        &mut self,
        runtime_url: &ProjectWorkareaRealRuntimeUrl,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> Option<ProjectWorkareaRuntimeCefSurface> {
        if let Some(owned) = self.take_custom_project_view(runtime_url) {
            return Some(owned);
        }
        let keep = self.project_switch_keep_alive();
        let keep_views_loaded = self.project_editor_auto_sleep_policy.keep_views_loaded;
        let project_id = self.agents_workspace_project_id.as_deref();
        let index = self
            .parked_project_workarea_surfaces
            .iter()
            .position(|parked| {
                parked.owned.matches_runtime_url(runtime_url)
                    && parked_page_is_for_slot(parked.slot_key, slot_key)
                    // A held page goes back only to its own project.
                    && (!parked.kept_loaded || parked.project_id.as_deref() == project_id)
                    && parked.kept(keep, keep_views_loaded)
            })?;
        Some(self.parked_project_workarea_surfaces.remove(index).owned)
    }

    /// A new page now serves `slot_key` for the project on screen, so the page held for it with
    /// another address (a published page from before a theme change, an editor that restarted)
    /// goes, as leaving the project again would replace it: a project keeps one held page per slot.
    pub(crate) fn release_held_page_replaced_by(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) {
        let project_id = self.agents_workspace_project_id.as_deref();
        self.parked_project_workarea_surfaces.retain(|parked| {
            !(parked.kept_loaded
                && parked.slot_key == slot_key
                && parked.project_id.as_deref() == project_id)
        });
    }

    /// Closes what `project_id` still has parked for `mode`, or for every view with `None`: a page
    /// kept for the project's return or held for Keep views loaded, and its parked website pages.
    /// Putting a view of the project on screen to sleep or closing its tab releases that view's
    /// (it came back showing another view); Sleep on a project that is not on screen releases all
    /// of them (`sleep_project_views`).
    pub(crate) fn release_parked_project_pages(
        &mut self,
        project_id: Option<&str>,
        mode: Option<TitlebarMode>,
    ) {
        let for_mode = |view: TitlebarMode| mode.is_none_or(|mode| mode == view);
        self.parked_project_workarea_surfaces.retain(|parked| {
            !(parked.project_id.as_deref() == project_id
                && for_mode(parked.slot_key.titlebar_mode()))
        });
        let Some(project_id) = project_id else {
            return;
        };
        let views = self
            .project_views
            .parked_views()
            .filter(|(_, owner, view, _)| *owner == project_id && for_mode(*view))
            .map(|(key, ..)| key.to_owned())
            .collect::<Vec<_>>();
        for key in views {
            self.project_views.close_parked_view(&key);
        }
    }

    /// Whether `project_id` has a parked page that `release_parked_project_pages` would close.
    pub(crate) fn holds_parked_project_pages(&self, project_id: &str) -> bool {
        self.parked_project_workarea_surfaces
            .iter()
            .any(|parked| parked.project_id.as_deref() == Some(project_id))
            || self
                .project_views
                .parked_views()
                .any(|(_, owner, ..)| owner == project_id)
    }

    /// Turning Keep views loaded on treats what the keep-alive minutes still keep as if it had been
    /// left with the setting on: the newest page per project and slot of a project's own views is
    /// held for that project, and the newest app-wide page per slot goes back among the live pages,
    /// where the switch would have left it, unless its slot has a live page or another address now
    /// (the Issues page is kept without one, as the prune keeps it). Pages are only ever added at the
    /// end, so walking back meets the newest first.
    pub(crate) fn hold_parked_project_workarea_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        let parked = std::mem::take(&mut self.parked_project_workarea_surfaces);
        let mut remaining = Vec::with_capacity(parked.len());
        let mut held = HashSet::new();
        let mut urls = HashMap::new();
        let mut restored = false;
        for mut page in parked.into_iter().rev() {
            let slot_key = page.slot_key;
            let back_live = page.owned.is_app_wide(slot_key)
                && !self
                    .project_workarea_runtime_cef_surfaces
                    .contains_key(&slot_key)
                && match urls
                    .entry(slot_key)
                    .or_insert_with(|| self.project_workarea_runtime_url_for_slot(slot_key))
                {
                    Some(url) => page.owned.matches_runtime_url(url),
                    None => {
                        slot_key == ProjectWorkareaCefSurfaceSlotKey::Work
                            && self.work_view_page_kept_hidden(&page.owned)
                    }
                };
            if back_live {
                self.project_workarea_runtime_cef_surfaces
                    .insert(slot_key, page.owned);
                restored = true;
            } else {
                page.kept_loaded = slot_key.holds_project_page()
                    && held.insert((page.project_id.clone(), slot_key));
                remaining.push(page);
            }
        }
        remaining.reverse();
        self.parked_project_workarea_surfaces = remaining;
        if restored {
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
            self.broadcast_extension_context_changes(cx);
        }
    }

    /// Closes the parked pages of the Code view (its own window's and every extra window's), as
    /// stopping or losing the editor does for the pages on screen.
    pub(crate) fn release_parked_code_editor_pages(&mut self) -> bool {
        let before = self.parked_project_workarea_surfaces.len();
        self.parked_project_workarea_surfaces
            .retain(|parked| !parked.slot_key.is_code_editor());
        self.parked_project_workarea_surfaces.len() != before
    }

    /// The parked page of one extra Code window, whichever project is on screen.
    pub(crate) fn parked_code_window_page_mut(
        &mut self,
        id: CodeWindowId,
    ) -> Option<&mut ParkedProjectWorkareaSurface> {
        self.parked_project_workarea_surfaces
            .iter_mut()
            .find(|parked| parked.slot_key == ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id))
    }

    /// Closes the parked page of one extra Code window.
    pub(crate) fn release_parked_code_window_page(&mut self, id: CodeWindowId) {
        self.parked_project_workarea_surfaces
            .retain(|parked| parked.slot_key != ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id));
    }

    pub(crate) fn expire_parked_project_workarea_surfaces(&mut self) {
        let keep = self.project_switch_keep_alive();
        let keep_views_loaded = self.project_editor_auto_sleep_policy.keep_views_loaded;
        // Dropping the entity closes the page, exactly as the prune did before keep-alive.
        self.parked_project_workarea_surfaces
            .retain(|parked| parked.kept(keep, keep_views_loaded));
    }

    /// Marks the parked pages the background Spaces open on, and closes the ones that were held only
    /// for a Space that no longer opens on them and are past the keep-alive window.
    pub(crate) fn refresh_space_kept_parked_workarea_surfaces(
        &mut self,
        space_kept: &crate::app::web_page_sleep::SpaceKeptWebPages,
    ) {
        let mut released = false;
        for parked in &mut self.parked_project_workarea_surfaces {
            let kept = parked.project_id.as_deref().is_some_and(|project_id| {
                space_kept.keeps_workarea_slot(project_id, parked.slot_key)
            });
            released |= parked.space_kept && !kept;
            parked.space_kept = kept;
        }
        if released {
            self.expire_parked_project_workarea_surfaces();
        }
    }

    fn next_project_keep_alive_expiry(&self) -> Option<Duration> {
        let keep = self.project_switch_keep_alive();
        let keep_views_loaded = self.project_editor_auto_sleep_policy.keep_views_loaded;
        let workarea = self
            .parked_project_workarea_surfaces
            .iter()
            .filter(|parked| !parked.space_kept && !(parked.kept_loaded && keep_views_loaded))
            .filter_map(|parked| keep_alive_remaining(Some(parked.parked_at), keep));
        let terminals = self
            .parked_agents_terminal_runtimes_by_project
            .values()
            .filter(|parked| !parked.kept_alive_viewer_sessions.is_empty())
            .filter_map(|parked| keep_alive_remaining(parked.parked_at, keep));
        workarea.chain(terminals).min()
    }

    /// One timer covers every kept project: it fires at the earliest expiry, runs the ordinary
    /// release passes (which now see the window as closed), and re-arms for the next project. A
    /// deadline earlier than the armed one (Keep views loaded turned off) arms a new timer, and
    /// the later one finds itself replaced when it fires.
    pub(crate) fn ensure_project_keep_alive_expiry_scheduled(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(delay) = self.next_project_keep_alive_expiry() else {
            return;
        };
        let deadline = Instant::now() + delay;
        if self
            .project_keep_alive_expiry_at
            .is_some_and(|armed| armed <= deadline)
        {
            return;
        }
        self.project_keep_alive_expiry_generation =
            self.project_keep_alive_expiry_generation.wrapping_add(1);
        let generation = self.project_keep_alive_expiry_generation;
        self.project_keep_alive_expiry_at = Some(deadline);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(delay + Duration::from_millis(100))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.project_keep_alive_expiry_generation != generation {
                    return;
                }
                this.project_keep_alive_expiry_at = None;
                this.expire_parked_project_workarea_surfaces();
                this.release_unused_agents_gpui_terminal_viewers(false, &HashSet::new(), cx);
                this.ensure_project_keep_alive_expiry_scheduled(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

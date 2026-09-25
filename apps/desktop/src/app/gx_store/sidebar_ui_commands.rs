//! Turning a command the renderer sends into the intent it is, so the sidebar's own state moves in
//! Rust in the same frame the click happens.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! The write to client storage carries only what this state changed (gx-core sidebar_ui/diff.rs).
//! Until the old projection was deleted the command was also sent on to it, so both sides applied
//! the same command to equivalent state.
//!
//! Four commands are deliberately not identical to the TypeScript, and they are listed together
//! here because the list belongs beside the code that decides it rather than only in a review:
//! `collectionAction:select` and `collectionAction:toggleProjects` act on the rows and groups the
//! list DRAWS, where `nativeCollectionGroups` acted on the collection's membership including its
//! filtered and hidden projects; and `sidebarAction:toggleProjects` likewise leaves hidden projects
//! alone. A sidebar slot hotkey used to be the fourth: it arrives as `gpuiProjectSlotHotkey` and
//! still never reaches this file, but since M5 piece 7c it reaches `sidebar_ui_paths.rs`, which
//! clears the multi-selection the way the hotkey does. The full list, with the reveal and the Space
//! differences, is in docs/2026-09-19/rust-core/PROGRESS.md.

use ghostex_gx_core::{SectionId, SidebarUiIntent, ToggleAllProjectsInput};
use serde_json::Value;

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Applies what this command does to the sidebar's own state. Returns whether it moved.
    pub(crate) fn gx_store_note_sidebar_command(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(intent) = self.sidebar_command_intent(command) else {
            return false;
        };
        self.gx_store_apply_sidebar_ui_intent(intent, cx);
        // Whether the command WAS one of the sidebar's own, not whether it changed anything: the
        // caller uses it to decide there is nothing left for the runtime to be told
        // (gx_store/sidebar_runtime_route.rs).
        true
    }

    fn sidebar_command_intent(&mut self, command: &Value) -> Option<SidebarUiIntent> {
        let text = |key: &str| command.get(key).and_then(Value::as_str).map(str::to_string);
        match command.get("type").and_then(Value::as_str)? {
            "toggleGroup" => Some(SidebarUiIntent::ToggleGroupCollapsed {
                group_id: text("groupId")?,
            }),
            // The three below carry the session-list storage id in `groupId`, which is what the
            // collapse state has always been keyed by below a project row.
            "toggleList" => Some(SidebarUiIntent::ToggleSessionListExpanded {
                storage_id: text("groupId")?,
            }),
            "toggleHoverActions" => Some(SidebarUiIntent::ToggleHoverActions {
                storage_id: text("groupId")?,
            }),
            "toggleSection" => Some(SidebarUiIntent::ToggleSection {
                storage_id: text("groupId")?,
                section: section_id(command.get("section").and_then(Value::as_str)?)?,
            }),
            "selectMachine" => Some(SidebarUiIntent::SelectMachine {
                machine_id: text("machineId")?,
            }),
            "selectSpace" => Some(SidebarUiIntent::SelectSpace {
                space_id: text("spaceId")?,
            }),
            "toggleTagFilter" => Some(SidebarUiIntent::ToggleTagFilter { tag: text("tag")? }),
            "projectMembership" if command.get("action") == Some(&Value::from("hide")) => {
                let group_id = text("groupId")?;
                Some(match self.sidebar_group_hidden(&group_id) {
                    true => SidebarUiIntent::UnhideGroup { group_id },
                    false => SidebarUiIntent::HideGroup { group_id },
                })
            }
            "sidebarAction" => match command.get("action").and_then(Value::as_str)? {
                "showHidden" => Some(SidebarUiIntent::ToggleShowHidden),
                "toggleProjects" => {
                    Some(SidebarUiIntent::ToggleAllProjects(ToggleAllProjectsInput {
                        machine_id: self.gx_store.sidebar_ui.selected_machine_id().to_string(),
                        group_ids: self.sidebar_drawn_project_group_ids(),
                    }))
                }
                _ => None,
            },
            "collectionAction" => self.collection_intent(command),
            "selectSession" => self.selection_intent(command),
            "batch" if command.get("clearSelection") == Some(&Value::Bool(true)) => {
                Some(SidebarUiIntent::SetSelectedSessions {
                    session_ids: Vec::new(),
                })
            }
            _ => None,
        }
    }

    fn collection_intent(&mut self, command: &Value) -> Option<SidebarUiIntent> {
        let collection_id = command.get("collectionId").and_then(Value::as_str)?;
        let storage_id = self.sidebar_collection_storage_id(collection_id)?;
        match command.get("action").and_then(Value::as_str)? {
            "toggle" => Some(SidebarUiIntent::ToggleCollectionCollapsed { storage_id }),
            "hide" => Some(match self.sidebar_collection_hidden(&storage_id) {
                true => SidebarUiIntent::UnhideCollection { storage_id },
                false => SidebarUiIntent::HideCollection { storage_id },
            }),
            "select" => Some(SidebarUiIntent::SetSelectedSessions {
                session_ids: self.sidebar_collection_session_ids(collection_id),
            }),
            "toggleProjects" => Some(SidebarUiIntent::ToggleAllProjects(ToggleAllProjectsInput {
                // The projects of one collection are remembered under the collection's own key,
                // so collapsing every project of a machine and collapsing one collection do not
                // overwrite each other's memory.
                machine_id: storage_id,
                group_ids: self.sidebar_collection_group_ids(collection_id),
            })),
            _ => None,
        }
    }

    fn selection_intent(&mut self, command: &Value) -> Option<SidebarUiIntent> {
        let mode = command.get("mode").and_then(Value::as_str)?;
        if matches!(mode, "clear" | "focus") {
            // A row click clears the multi-selection; the focus itself is the store's own intent
            // and is applied where the selection is made (gx_store/local_focus.rs).
            return Some(SidebarUiIntent::SetSelectedSessions {
                session_ids: Vec::new(),
            });
        }
        let clicked = command.get("sessionId").and_then(Value::as_str)?;
        let visible = self.sidebar_rendered_session_ids();
        if !visible.iter().any(|session_id| session_id == clicked) && mode == "range" {
            return Some(SidebarUiIntent::SetSelectedSessions {
                session_ids: Vec::new(),
            });
        }
        let session_ids = match mode {
            // Cmd-click adds exactly the clicked visible row to the set, dropping anything the
            // list no longer draws. The active session is never seeded in implicitly.
            "additive" => {
                let mut next: Vec<String> = self
                    .gx_store
                    .sidebar_ui
                    .state()
                    .selected_session_ids
                    .iter()
                    .filter(|session_id| visible.iter().any(|drawn| drawn == *session_id))
                    .cloned()
                    .collect();
                if visible.iter().any(|drawn| drawn == clicked)
                    && !next.iter().any(|held| held == clicked)
                {
                    next.push(clicked.to_string());
                }
                next
            }
            // Shift-click is anchored on the focused row and takes the inclusive range in rendered
            // order, so collapsed projects, filters and sorting define it exactly.
            "range" => {
                let clicked_index = visible.iter().position(|id| id == clicked)?;
                match self.sidebar_focused_row_id() {
                    Some(active) => match visible.iter().position(|id| *id == active) {
                        Some(active_index) => {
                            let start = active_index.min(clicked_index);
                            let end = active_index.max(clicked_index);
                            visible[start..=end].to_vec()
                        }
                        None => vec![clicked.to_string()],
                    },
                    None => vec![clicked.to_string()],
                }
            }
            _ => return None,
        };
        Some(SidebarUiIntent::SetSelectedSessions { session_ids })
    }
}

/// The rows and ids the intents above are resolved against: the list that is on screen.
impl GhostexGpuiApp {
    /// Every project group the machine draws, the Chats collection left out.
    fn sidebar_drawn_project_group_ids(&self) -> Vec<String> {
        self.gx_store
            .sidebar_list
            .view()
            .groups
            .iter()
            .filter(|group| group.core.group_id != ghostex_gx_core::CHATS_GROUP_ID)
            .map(|group| group.core.group_id.clone())
            .collect()
    }

    /// The `<section key>:<collection id>` a collection's own state is stored under.
    ///
    /// The key is built from the section, exactly as `runNativeCollectionAction` builds it, rather
    /// than looked up in the drawn list: a collection the user hid is in no drawn list, and
    /// resolving it there would make unhiding it a silent no-op. The collection still has to be
    /// one the sidebar knows, so an id from a stale menu does nothing, which is what the
    /// TypeScript's `if (!collection) return` does.
    ///
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// "One the sidebar knows" is asked of the store's own list, which since M4d part 2 step 6 is
    /// the only list there is. It used to be asked of the old page's projection, and that
    /// projection is about to stop being published: reading it would make every collection answer
    /// `None` here and collapse, hide, select and Collapse Projects would silently stop on every
    /// one of them.
    fn sidebar_collection_storage_id(&self, collection_id: &str) -> Option<String> {
        let state = self.gx_store.sidebar_ui.state();
        let storage_id = format!("{}:{}", state.section_key(), collection_id);
        let drawn = self.sidebar_drawn_collection(collection_id).is_some();
        let hidden = state
            .hidden_items
            .collection_keys
            .iter()
            .any(|key| *key == storage_id);
        (drawn || hidden).then_some(storage_id)
    }

    /// The groups of a collection, from the list the menu that sent the command was built from.
    fn sidebar_collection_group_ids(&self, collection_id: &str) -> Vec<String> {
        self.sidebar_drawn_collection(collection_id)
            .unwrap_or_default()
    }

    /// The drawn collection's group ids, or `None` when no drawn collection carries that id.
    fn sidebar_drawn_collection(&self, collection_id: &str) -> Option<Vec<String>> {
        self.gx_store
            .sidebar_list
            .view()
            .collections
            .iter()
            .find(|collection| collection.collection_id == collection_id)
            .map(|collection| collection.group_ids.clone())
    }

    fn sidebar_collection_session_ids(&self, collection_id: &str) -> Vec<String> {
        let group_ids = self.sidebar_collection_group_ids(collection_id);
        self.gx_store
            .sidebar_list
            .view()
            .groups
            .iter()
            .filter(|group| group_ids.iter().any(|id| *id == group.core.group_id))
            .flat_map(|group| group.core.sessions.iter())
            .map(|session| session.row.sidebar_session_id.clone())
            .collect()
    }

    /// The rows the list actually draws, in order: what a shift-click range is measured in.
    fn sidebar_rendered_session_ids(&self) -> Vec<String> {
        let view = self.gx_store.sidebar_list.view();
        view.order
            .iter()
            .flat_map(|item| match item.kind {
                ghostex_gx_core::OrderKind::Project => vec![item.id.clone()],
                ghostex_gx_core::OrderKind::Collection => view
                    .collections
                    .iter()
                    .find(|collection| collection.collection_id == item.id && !collection.collapsed)
                    .map(|collection| collection.group_ids.clone())
                    .unwrap_or_default(),
            })
            .filter_map(|group_id| view.group(&group_id))
            .filter(|group| !group.core.collapsed)
            .flat_map(|group| {
                group
                    .core
                    .sections
                    .iter()
                    .filter(|section| !section.collapsed)
                    .flat_map(|section| section.session_ids.iter().cloned())
            })
            .collect()
    }

    fn sidebar_focused_row_id(&self) -> Option<String> {
        self.gx_store
            .core
            .focus()
            .focused_session
            .as_ref()
            .map(ghostex_gx_core::SessionKey::to_sidebar_session_id)
    }

    fn sidebar_group_hidden(&self, group_id: &str) -> bool {
        self.gx_store
            .sidebar_ui
            .state()
            .hidden_items
            .group_ids
            .iter()
            .any(|held| held == group_id)
    }

    fn sidebar_collection_hidden(&self, storage_id: &str) -> bool {
        self.gx_store
            .sidebar_ui
            .state()
            .hidden_items
            .collection_keys
            .iter()
            .any(|held| held == storage_id)
    }
}

fn section_id(value: &str) -> Option<SectionId> {
    Some(match value {
        "browser" => SectionId::Browser,
        "pinned" => SectionId::Pinned,
        "drafts" => SectionId::Drafts,
        "sessions" => SectionId::Sessions,
        "parked" => SectionId::Parked,
        "snoozed" => SectionId::Snoozed,
        _ => return None,
    })
}

impl GhostexGpuiApp {
    /// Puts a row on screen: the Space that shows it is selected, the group, its collection and
    /// its heading are opened, Show Hidden and the tag filters are lifted where they hide it, and
    /// the full list is shown when the compact one would still leave it out. The old projection
    /// does the same to its own copy, from the same request, so the two stay in step.
    ///
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// A reveal that arrives before the list is ready is HELD, not answered. The whole plan is read
    /// off `inputs.ui`, and until the client-storage read lands that is the empty default: every
    /// group open, nothing hidden, no tag filter, so the plan has nothing to lift and makes no
    /// intent, and with no presentation snapshot yet even the scratch build cannot locate the row.
    /// Answering it then took the request id all the same, so the user's real collapsed project
    /// came back from storage a moment later with the row still folded inside it, and the session a
    /// notification or a project activation asked for was scrolled to but never opened.
    /// `gx_store_replay_held_sidebar_reveal` answers it once the list is ready, which is the one
    /// place the sidebar's own state and the HUD are both known to have landed. This supersedes the
    /// 2026-09-20 note that the id had to be taken before any gate, which guarded against the
    /// deleted page's publish re-carrying a minutes-old request on every frame: `newest_reveal` is
    /// now set only by a reveal that really happened, and the ready gate opens once per launch.
    pub(crate) fn gx_store_note_sidebar_reveal(
        &mut self,
        sidebar_session_id: &str,
        request_id: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.runtime_facts.counters.reveals_held += 1;
            return;
        }
        let fresh = self.gx_store.sidebar_ui.take_reveal_request(request_id);
        if !fresh {
            return;
        }
        self.gx_store_apply_sidebar_reveal(sidebar_session_id, cx);
    }

    /// The newest reveal the launch window held, answered the first time the list is ready. Costs
    /// one `Option<u64>` comparison on every other update.
    pub(super) fn gx_store_replay_held_sidebar_reveal(&mut self, cx: &mut gpui::Context<Self>) {
        let held = self
            .gx_store
            .runtime_facts
            .newest_reveal
            .as_ref()
            .filter(|reveal| !self.gx_store.sidebar_ui.reveal_handled(reveal.request_id))
            .cloned();
        let Some(held) = held else {
            return;
        };
        self.gx_store.runtime_facts.counters.reveals_replayed += 1;
        self.gx_store_note_sidebar_reveal(&held.session_id, held.request_id, cx);
    }

    /// The reveal itself, for a request a publish carried and for the slot hotkey's jump, which
    /// asks for it in the key's own frame. Returns how many changes to the sidebar's own state it
    /// made, or `None` when there is no plan (the row is nowhere, or on a machine the store holds
    /// no rows for).
    ///
    /// The plan and the changes it makes are gx-core's (`reveal_plan`, `SidebarRevealPlan::intents`),
    /// applied in one batch so a reveal that opens a group, a heading and the full list rebuilds
    /// the list once rather than three times. The count excludes the Space memory.
    pub(crate) fn gx_store_apply_sidebar_reveal(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<usize> {
        let now_ms = super::host::now_ms();
        let plan = {
            let store = &self.gx_store;
            ghostex_gx_core::reveal_plan(
                &store.core,
                &store.sidebar_list.last_inputs,
                store.sidebar_list.view(),
                sidebar_session_id,
                now_ms,
            )
        }?;
        let mut changed = 0;
        for intent in plan.intents(self.gx_store.sidebar_ui.state()) {
            if self.gx_store_apply_sidebar_ui_intent_unbuilt(intent, cx) {
                changed += 1;
            }
        }
        // `rememberNativeSidebarFocus` runs FIRST inside `applyNativeSidebarReveal` and remembers
        // the row under the Space it belongs to, whatever the follow setting says. The plan carries
        // that Space, resolved from the same group the rest of the plan was built from. It is in
        // the same batch, so a reveal with Spaces on does not rebuild the list a second time for
        // a memory the list does not draw.
        let mut remembered = false;
        if let Some(resolved) = plan.remember_space {
            remembered = self.gx_store_apply_sidebar_ui_intent_unbuilt(
                SidebarUiIntent::RememberSpaceSession {
                    section_key: resolved.section_key,
                    space_id: resolved.space_id,
                    sidebar_session_id: sidebar_session_id.to_string(),
                },
                cx,
            );
            if remembered {
                self.gx_store.sidebar_ui.counters.space_memory_writes += 1;
            }
        }
        if changed > 0 || remembered {
            self.gx_store_sidebar_state_changed(cx);
        }
        Some(changed)
    }
}

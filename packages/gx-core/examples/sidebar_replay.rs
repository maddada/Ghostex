//! Replays recorded `/api/events` frames through the core and derives the sidebar list from every
//! one of them, incrementally and from scratch, and compares the two.
//!
//! Usage: `cargo run --release --example sidebar_replay -- <frames.jsonl> [projects-rpc.json]`
//!
//! The second file is the body of a `listProjects` response, which carries the domain rows the
//! project overlays (chat projects, icons, worktrees) are built from.
//!
//! The replay does not only feed frames: it drives focus, churns the sidebar's own UI state and
//! settings on a rotation, and walks the clock a few seconds per frame so the time-based rules
//! (a new session leading the list, a snooze ending) fire. Every frame is compared both ways, so
//! anything the incremental path forgets to invalidate shows up here.
//!
//! Recordings contain private data: keep them outside the repository. This is tooling, not a test
//! suite: it reports what the view model makes of real traffic.

use std::collections::BTreeSet;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use ghostex_gx_core::protocol::{
    CustomSessionTag, CustomSessionTagsState, ServerEvent, GXSERVER_PROTOCOL_VERSION,
};
use ghostex_gx_core::{
    collapse_into_storage, collapse_state_from_storage, reveal_plan, BrowserTabInput,
    ChangeSummary, Core, Event, ExternalFocusUpdate, Intent, MachineId, MachineTabInput, SectionId,
    SessionKey, SessionSortMode, SidebarCollapseDiff, SidebarInputs, SidebarUiIntent,
    SidebarUiStore, SidebarView, SidebarViewModel, ToggleAllProjectsInput,
};
use serde_json::Value;

/// Where the replay's clock starts. The frames carry no timestamps of their own.
const START_MS: u64 = 1_790_000_000_000;
/// How far the clock moves per frame, so a recording of a few thousand frames covers hours and
/// the ten-minute new-session window and any snooze in the rows expire while it runs.
const CLOCK_STEP_MS: u64 = 3_000;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: sidebar_replay <frames.jsonl> [projects-rpc.json]");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            return ExitCode::from(2);
        }
    };
    let domain_projects = std::env::args().nth(2).and_then(read_domain_projects);
    if let Some(projects) = &domain_projects {
        println!("domain project rows: {}", projects.len());
    }

    let churned = run_pass("churned inputs", &text, domain_projects.clone(), true);
    let steady = run_pass("steady inputs", &text, domain_projects.clone(), false);
    let last_seen = last_seen_transition(&text, &path);
    let focus_moves = focus_without_frames(&text, domain_projects);

    if churned.mismatches == 0 && steady.mismatches == 0 && last_seen && focus_moves {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn read_domain_projects(path: String) -> Option<Vec<Value>> {
    let text = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    Some(
        value
            .pointer("/result/projects")
            .or_else(|| value.get("projects"))?
            .as_array()?
            .clone(),
    )
}

struct PassResult {
    mismatches: u64,
}

/// The sidebar's own state, its writes, and what they are measured against.
struct UiState {
    store: SidebarUiStore,
    stored: String,
    base: ghostex_gx_core::SidebarCollapseState,
    writes: u64,
    persist_failures: u64,
    reveals: u64,
    reveal_failures: u64,
}

impl UiState {
    fn new() -> Self {
        let store = SidebarUiStore::new();
        let stored = collapse_into_storage(&store.state().collapse, None);
        let base = store.state().collapse.clone();
        Self {
            store,
            stored,
            base,
            writes: 0,
            persist_failures: 0,
            reveals: 0,
            reveal_failures: 0,
        }
    }

    /// Applies one intent the way the desktop host does: the state moves, and what it changed is
    /// written to storage and read back, so the round trip is exercised against real ids.
    fn apply(&mut self, intent: SidebarUiIntent) {
        if !self.store.apply(intent).changed {
            return;
        }
        if !self.store.take_pending().collapse {
            return;
        }
        let diff = SidebarCollapseDiff::between(&self.base, &self.store.state().collapse);
        self.stored = diff.apply(Some(&self.stored), &self.store.state().collapse);
        self.base = self.store.state().collapse.clone();
        self.writes += 1;
        let read_back = collapse_state_from_storage(Some(&self.stored), None, None);
        if persisted_only(read_back) != persisted_only(self.base.clone()) {
            self.persist_failures += 1;
        }
    }
}

/// The collapse state as a restart would see it: only Pinned and Sessions are stored.
fn persisted_only(
    mut state: ghostex_gx_core::SidebarCollapseState,
) -> ghostex_gx_core::SidebarCollapseState {
    for sections in state.section_collapse.values_mut() {
        *sections = ghostex_gx_core::SectionCollapse {
            pinned: sections.pinned,
            sessions: sections.sessions,
            ..ghostex_gx_core::SectionCollapse::default()
        };
    }
    state
}

fn run_pass(
    label: &str,
    frames: &str,
    domain_projects: Option<Vec<Value>>,
    churn: bool,
) -> PassResult {
    let machine = MachineId::Local;
    let mut core = Core::new();
    let mut model = SidebarViewModel::new();
    let mut inputs = SidebarInputs::default();
    inputs.settings.sort_mode = SessionSortMode::LastActivity;
    let mut ui = UiState::new();
    if let Some(projects) = domain_projects {
        core.handle(
            Event::DomainProjectsRead {
                machine: machine.clone(),
                projects,
            },
            START_MS,
        );
    }

    let mut cold_build: Option<Duration> = None;
    let mut store_changed: Vec<Duration> = Vec::new();
    let mut catalog_changed: Vec<Duration> = Vec::new();
    let mut inputs_changed: Vec<Duration> = Vec::new();
    let mut idle: Vec<Duration> = Vec::new();
    let mut scratch: Vec<Duration> = Vec::new();
    let mut handled = 0u64;
    let mut focus_intents = 0u64;
    let mut churn_steps = 0u64;
    let mut catalog_swaps = 0u64;
    let mut captured_tags: Option<CustomSessionTagsState> = None;
    let mut mismatches = 0u64;
    let mut first_mismatch: Option<String> = None;
    let started = Instant::now();

    for (index, line) in frames.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(frame) = ServerEvent::parse(line) else {
            continue;
        };
        handled += 1;
        let now_ms = START_MS + index as u64 * CLOCK_STEP_MS;
        let output = core.handle(
            Event::Frame {
                machine: machine.clone(),
                frame: Box::new(frame),
            },
            now_ms,
        );
        let mut changes = output.changes.clone();
        // Drive focus the way a user would, so the focus-dependent parts of the list are
        // exercised against real rows.
        if let Some(session) = output.changes.sessions_changed.first() {
            if index % 7 == 0 {
                let focus_output = core.handle(
                    Event::Intent(Intent::FocusSession {
                        session: session.clone(),
                        visible: Some(vec![session.clone()]),
                    }),
                    now_ms,
                );
                changes.merge(focus_output.changes);
                focus_intents += 1;
            }
        }
        // The tag catalog, the one store input that invalidates every row of the machine without
        // any session changing. Swapped on a rotation between a catalog of the replay's own and
        // the recording's, so rows lose their custom tag labels and get them back.
        let mut catalog_swapped = false;
        if churn && index % 29 == 28 {
            if captured_tags.is_none() {
                captured_tags = core
                    .presentation()
                    .machine(&machine)
                    .and_then(|entry| entry.side_state().custom_session_tags.clone());
            }
            let catalog = if (index / 29) % 2 == 0 {
                replay_tag_catalog()
            } else {
                captured_tags.clone().unwrap_or_default()
            };
            if let Some(frame) = tag_catalog_frame(&catalog) {
                let swapped = core.handle(
                    Event::Frame {
                        machine: machine.clone(),
                        frame: Box::new(frame),
                    },
                    now_ms,
                );
                changes.merge(swapped.changes);
                catalog_swaps += 1;
                catalog_swapped = true;
            }
        }
        let store_moved = !changes.is_empty();
        let before = inputs.clone();
        if churn {
            churn_inputs(&mut inputs, &mut ui, model.view(), index);
            inputs.ui = ui.store.state().clone();
            if inputs != before {
                churn_steps += 1;
            }
        }
        let inputs_moved = inputs != before;

        let empty_before = model.view().groups.is_empty();
        let update_started = Instant::now();
        model.update(&core, &inputs, &changes, now_ms);
        let elapsed = update_started.elapsed();
        // The first update that produces a list is the cold build: the store's first snapshot.
        // Everything before it runs against an empty store and says nothing about the cost.
        if cold_build.is_none() && empty_before && !model.view().groups.is_empty() {
            cold_build = Some(elapsed);
        } else if catalog_swapped {
            catalog_changed.push(elapsed);
        } else if store_moved {
            store_changed.push(elapsed);
        } else if inputs_moved {
            inputs_changed.push(elapsed);
        } else {
            idle.push(elapsed);
        }

        // Every so often, ask what it would take to put one row on screen and check that doing it
        // actually draws the row. The plan is worked out against the same store and inputs, so a
        // rule it gets wrong shows up as a row that is still not drawn.
        if churn && index % 31 == 30 {
            check_reveal(&core, &inputs, model.view(), index, now_ms, &mut ui);
        }
        let scratch_started = Instant::now();
        let fresh = SidebarViewModel::build_from_scratch(&core, &inputs, now_ms);
        scratch.push(scratch_started.elapsed());
        if fresh != *model.view() {
            mismatches += 1;
            if first_mismatch.is_none() {
                first_mismatch = Some(describe_difference(&fresh, model.view(), index + 1));
            }
        }
    }
    let wall = started.elapsed();
    let view = model.view();

    println!("\n== {label} ==");
    println!(
        "{handled} frames in {} ms, {focus_intents} focus intents, {churn_steps} input changes, {catalog_swaps} tag catalog swaps, clock walked {} minutes",
        wall.as_millis(),
        handled * CLOCK_STEP_MS / 60_000
    );
    if let Some(loaded) = core.presentation().loaded(&machine) {
        println!(
            "store: {} projects, {} sessions",
            loaded.projects().len(),
            loaded.session_count()
        );
    }
    println!(
        "list: {} groups, {} rows, {} collections, {} Spaces, {} top-level rows",
        view.groups.len(),
        view.groups
            .iter()
            .map(|group| group.core.sessions.len())
            .sum::<usize>(),
        view.collections.len(),
        view.spaces.len(),
        view.order.len()
    );
    println!(
        "machine counts: {} working, {} needing attention; empty state copy: {:?}",
        view.machine.working_count, view.machine.attention_count, view.empty_state.copy
    );
    if let Some(cold) = cold_build {
        println!("{:<38} {:>9}", "cold build (first snapshot)", micros(cold));
    }
    report("incremental update (store changed)", &mut store_changed);
    report("incremental update (inputs churned)", &mut inputs_changed);
    report("incremental update (tags swapped)", &mut catalog_changed);
    report("update with nothing changed", &mut idle);
    report("build from scratch", &mut scratch);
    println!(
        "incremental against from scratch: {} comparisons, {mismatches} differences",
        scratch.len()
    );
    println!(
        "sidebar state: {} collapse writes, {} that did not survive a read, {} reveals, {} that did not draw the row",
        ui.writes, ui.persist_failures, ui.reveals, ui.reveal_failures
    );
    if let Some(first) = &first_mismatch {
        println!("{first}");
    }

    PassResult {
        mismatches: mismatches + ui.persist_failures + ui.reveal_failures,
    }
}

/// The last-seen transition: a remote machine seeded from a stored snapshot, then the live stream
/// arriving with a LOWER revision.
///
/// This is the case the design is most afraid of and the one no recording holds. A last-seen
/// snapshot carries the PREVIOUS run's revision; the daemon on the other machine may have
/// restarted and reset its counter, so its live first frames are LOWER. Every revision rule reads
/// "lower or equal" as "already contained in what I hold", so without the `last_seen` flag the
/// live stream would be silently dropped and the user would keep looking at yesterday's faded rows
/// while the machine is connected.
///
/// Four questions, and each of them fails if the flag is removed:
/// 1. The seed draws rows at all.
/// 2. A DELTA at a lower revision is refused while the rows are last-seen (it must not ride on
///    them, because they are not a revision this client was ever told).
/// 3. A `presentationSnapshotCurrent` at that revision does NOT confirm them as live.
/// 4. The live SNAPSHOT replaces them whole even though its revision is lower, and the list built
///    incrementally across the transition equals one built from scratch afterwards, which is the
///    cache question three bugs of this shape have already been found on.
///
/// It also carries the WRITE half's only checks on real rows: what the store would store for this
/// machine, against the rows it was given (`stored_round_trip`) and against what `JSON.stringify`
/// makes of the same rows (`stringify_round_trip`).
fn last_seen_transition(frames: &str, frames_path: &str) -> bool {
    let Some(stored) = first_snapshot(frames) else {
        println!(
            "\n== last-seen transition ==\nno snapshot frame in the recording, nothing measured"
        );
        return false;
    };
    let Some(victim) = first_row(&stored) else {
        println!("\n== last-seen transition ==\nthe recorded snapshot has no session rows");
        return false;
    };
    let machine = MachineId::Remote("remote-ab12".to_string());
    let mut core = Core::new();
    let mut model = SidebarViewModel::new();
    let mut inputs = SidebarInputs::default();
    inputs.ui.selected_machine_id = "remote-ab12".to_string();
    let now_ms = START_MS;

    // The stored copy, at a HIGH revision: the previous run got far.
    let seeded = core.seed_last_seen_presentation(&machine, snapshot_at(&stored, 9_000));
    model.update(&core, &inputs, &seeded.changes, now_ms);
    let seeded_rows: usize = model
        .view()
        .groups
        .iter()
        .map(|group| group.core.sessions.len())
        .sum();
    let seeded_faded = model.view().groups.iter().all(|group| group.core.is_stale);
    // The WRITE half's only gate that runs on real rows: the copy this app would store for a
    // machine has to be the copy it read, byte for byte, or the two writers of this key flip the
    // row between them on every publish and a field the wire type drops is stripped from the copy
    // the old sidebar still reads.
    let seeded_round_trip = stored_round_trip(&core, &machine, &stored, 9_000);
    let seeded_stringify = stringify_round_trip(&core, &machine, frames_path, 9_000);

    // A live delta and a "current" reply, both at a revision LOWER than the stored one, which is
    // what a daemon that restarted sends. Neither may be believed.
    let delta_changes = core
        .handle_raw_frame(machine.clone(), &delta_frame(12, &victim), now_ms)
        .map(|output| output.changes)
        .unwrap_or_default();
    // And one ABOVE the stored revision, which is the delta that would really be applied: a
    // daemon that did NOT restart is simply further along than the copy, and without the flag its
    // delta would edit yesterday's rows as though this client had been following all along.
    let ahead_changes = core
        .handle_raw_frame(machine.clone(), &delta_frame(9_500, &victim), now_ms)
        .map(|output| output.changes)
        .unwrap_or_default();
    let current_changes = core
        .handle_raw_frame(machine.clone(), &current_frame(12), now_ms)
        .map(|output| output.changes)
        .unwrap_or_default();
    let still_last_seen = core
        .presentation()
        .loaded(&machine)
        .is_some_and(|loaded| loaded.last_seen);

    // The live snapshot, also lower. This one MUST replace the rows.
    let live = core
        .handle_raw_frame(machine.clone(), &snapshot_frame(&stored, 12), now_ms)
        .expect("the live snapshot parses");
    model.update(&core, &inputs, &live.changes, now_ms);
    let now_live = core
        .presentation()
        .loaded(&machine)
        .is_some_and(|loaded| !loaded.last_seen && loaded.revision == 12);
    let live_round_trip = stored_round_trip(&core, &machine, &stored, 12);
    let live_stringify = stringify_round_trip(&core, &machine, frames_path, 12);
    let fresh = SidebarViewModel::build_from_scratch(&core, &inputs, now_ms);
    let same = fresh == *model.view();

    let checks = [
        ("the seed draws rows", seeded_rows > 0),
        ("the seeded copy writes back unchanged", seeded_round_trip),
        ("the live copy writes back unchanged", live_round_trip),
        (
            "the seeded copy is what JSON.stringify writes",
            seeded_stringify,
        ),
        (
            "the live copy is what JSON.stringify writes",
            live_stringify,
        ),
        (
            "every seeded group is faded",
            seeded_faded && seeded_rows > 0,
        ),
        ("a lower delta is refused", delta_changes.is_empty()),
        ("a HIGHER delta is refused too", ahead_changes.is_empty()),
        (
            "a lower current does not confirm",
            current_changes.is_empty(),
        ),
        ("the rows are still last-seen", still_last_seen),
        ("the live snapshot takes over", now_live),
        ("incremental equals from scratch", same),
    ];
    println!("\n== last-seen transition ==");
    println!("seeded {seeded_rows} rows at revision 9000, live stream resumed at 12");
    let mut passed = true;
    for (label, ok) in checks {
        println!("{label:<40} {}", if ok { "ok" } else { "FAILED" });
        passed &= ok;
    }
    passed
}

/// Focus that moves with no frame behind it: a tab click, a held key, the old runtime's focus
/// payload mirrored into the store. The live app's five `scratchMismatch` records of 2026-09-21
/// were this, and nothing else in this tool moves focus without a frame.
///
/// Each step moves focus and then asks two questions. The STALE probe compares the list as it stood
/// before the move with a fresh build, which is what the desktop host's scratch check saw: the host
/// brought the list up to date only after a pump or a publish, so a selection or a mirrored payload
/// landing between a publish and the check left the list built for the previous focus. The probe
/// must differ on every step (a probe that cannot fail proves nothing) and together the steps must
/// name exactly the fields the live records named. The HOST probe then does what the host does
/// since the fix (`gx_store_select_local_session`, `gx_store_admit_old_runtime_focus_state` and
/// the delayed shadow settle all call `gx_store_update_sidebar_list`): an update with an EMPTY
/// change summary, because a focus move is not in any summary the host collects, and the list must
/// then equal a fresh build. That half also shows the view model itself compares focus by value.
///
/// `SIDEBAR_REPLAY_MUTATION=no-update-after-focus` runs the host probe in the old order and must
/// fail.
fn focus_without_frames(frames: &str, domain_projects: Option<Vec<Value>>) -> bool {
    println!("\n== focus moves without a frame ==");
    let skip_update = std::env::var("SIDEBAR_REPLAY_MUTATION")
        .is_ok_and(|mutation| mutation == "no-update-after-focus");
    let machine = MachineId::Local;
    let mut core = Core::new();
    if let Some(projects) = domain_projects {
        core.handle(
            Event::DomainProjectsRead {
                machine: machine.clone(),
                projects,
            },
            START_MS,
        );
    }
    let mut now_ms = START_MS;
    for (index, line) in frames.lines().enumerate() {
        let Ok(frame) = ServerEvent::parse(line) else {
            continue;
        };
        now_ms = START_MS + index as u64 * CLOCK_STEP_MS;
        core.handle(
            Event::Frame {
                machine: machine.clone(),
                frame: Box::new(frame),
            },
            now_ms,
        );
    }
    let mut inputs = SidebarInputs::default();
    inputs.settings.sort_mode = SessionSortMode::LastActivity;
    let mut model = SidebarViewModel::new();
    model.update(&core, &inputs, &ChangeSummary::default(), now_ms);

    // Two projects that are not worktrees: the first drawn one with two session rows, and one with
    // a row outside the first one's collection. A Space claims a project through its collection
    // when it has one, so the claim below names whichever of the two the first project goes by.
    type ProjectRows = (String, Option<String>, Vec<SessionKey>);
    let project_rows = |view: &SidebarView| -> Vec<ProjectRows> {
        view.groups
            .iter()
            .filter_map(|group| {
                let context = group.core.project_context.as_ref()?;
                if context.worktree.is_some() {
                    return None;
                }
                let keys: Vec<SessionKey> = group
                    .core
                    .sessions
                    .iter()
                    .filter(|session| !session.row.is_browser)
                    .filter_map(|session| session.row.key.clone())
                    .collect();
                Some((
                    context.project_id.clone(),
                    group.collection_id.clone(),
                    keys,
                ))
            })
            .collect()
    };
    let projects = project_rows(model.view());
    let Some((first_project, first_collection, first_rows)) =
        projects.iter().find(|(_, _, rows)| rows.len() >= 2)
    else {
        println!("no project with two session rows, nothing measured");
        return false;
    };
    let Some((_, _, other_rows)) = projects.iter().find(|(project_id, collection, rows)| {
        project_id != first_project
            && (collection.is_none() || collection != first_collection)
            && !rows.is_empty()
    }) else {
        println!("no second project with a session row, nothing measured");
        return false;
    };
    let (row_a, row_b, row_c) = (
        first_rows[0].clone(),
        first_rows[1].clone(),
        other_rows[0].clone(),
    );

    // A Space that claims the first project, so the second one lands in Other and a move between
    // them moves the Space row that holds the focused session. The section shows that Space.
    let spaces = serde_json::json!({
        "type": "sidebarSpacesChanged",
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "sidebarSpaces": {
            "order": ["replay-focus"],
            "spaces": { "replay-focus": {
                "spaceId": "replay-focus", "name": "Replay", "color": "#7c6df2", "icon": "stack",
                "memberCollectionIds": first_collection.iter().collect::<Vec<_>>(),
                "memberProjectIds": if first_collection.is_some() { vec![] } else { vec![first_project] },
            } },
        },
    });
    let Ok(frame) = ServerEvent::parse(&spaces.to_string()) else {
        println!("the Spaces frame does not parse");
        return false;
    };
    let output = core.handle(
        Event::Frame {
            machine: machine.clone(),
            frame: Box::new(frame),
        },
        now_ms,
    );
    inputs.settings.sidebar_spaces_enabled = true;
    let focus_on = |core: &mut Core, session: &SessionKey| -> ChangeSummary {
        core.handle(
            Event::Intent(Intent::FocusSession {
                session: session.clone(),
                visible: Some(vec![session.clone()]),
            }),
            now_ms,
        )
        .changes
    };
    let mut changes = output.changes;
    changes.merge(focus_on(&mut core, &row_a));
    model.update(&core, &inputs, &changes, now_ms);
    let baseline_same =
        SidebarViewModel::build_from_scratch(&core, &inputs, now_ms) == *model.view();

    let mut stale_fields: BTreeSet<String> = BTreeSet::new();
    let mut stale_steps = 0usize;
    let mut host_differences = 0usize;
    let mut steps = 0usize;
    let mut step = |core: &mut Core,
                    model: &mut SidebarViewModel,
                    inputs: &SidebarInputs,
                    label: &str,
                    move_focus: &dyn Fn(&mut Core)| {
        steps += 1;
        move_focus(core);
        let fresh = SidebarViewModel::build_from_scratch(core, inputs, now_ms);
        let stale = differing_fields(model.view(), &fresh);
        if !stale.is_empty() {
            stale_steps += 1;
        }
        println!("{label:<40} stale list differs in {stale:?}");
        stale_fields.extend(stale);
        if !skip_update {
            model.update(core, inputs, &ChangeSummary::default(), now_ms);
        }
        let host = differing_fields(model.view(), &fresh);
        if !host.is_empty() {
            host_differences += 1;
            println!("{label:<40} UPDATED list differs in {host:?}");
        }
    };
    step(
        &mut core,
        &mut model,
        &inputs,
        "another row of the same project",
        &|core| {
            focus_on(core, &row_b);
        },
    );
    // What the live record's update was: nothing from the store, and the sidebar's own state and
    // the settings both moved (a modal opened and closed). The list must stay what a fresh build
    // gives, and it is the update BEFORE the next move, as it was live.
    inputs.settings.show_last_active_time = !inputs.settings.show_last_active_time;
    inputs
        .ui
        .collapse
        .expanded_hover_actions
        .insert("replay-focus-unrelated".to_string());
    model.update(&core, &inputs, &ChangeSummary::default(), now_ms);
    let quiet_same = SidebarViewModel::build_from_scratch(&core, &inputs, now_ms) == *model.view();
    step(
        &mut core,
        &mut model,
        &inputs,
        "a row of a project in another Space",
        &|core| {
            focus_on(core, &row_c);
        },
    );
    step(
        &mut core,
        &mut model,
        &inputs,
        "the old runtime's payload, mirrored",
        &|core| {
            let stamp = core.focus().local_stamp;
            core.handle(
                Event::Intent(Intent::ExternalFocus(
                    ExternalFocusUpdate::new(stamp)
                        .with_active_project(row_a.project_key())
                        .with_focused_session(row_a.clone())
                        .with_visible_sessions(vec![row_a.clone()]),
                )),
                now_ms,
            );
        },
    );
    let live_fields = ["spaces", "isActive", "sections", "isFocused", "isVisible"];
    let (remote_steps, remote_stale_steps, remote_host_differences) =
        remote_focus_moves(&mut core, &mut model, &inputs, &row_a, now_ms, skip_update);
    let checks = [
        ("the list starts equal to a fresh build", baseline_same),
        (
            "every stale probe differs",
            stale_steps == steps && steps == 3,
        ),
        (
            "every remote stale probe differs",
            remote_stale_steps == remote_steps && remote_steps == 4,
        ),
        (
            "both lists after a move across machines",
            remote_host_differences == 0,
        ),
        (
            "the stale probes name the live fields",
            live_fields
                .iter()
                .all(|field| stale_fields.contains(*field)),
        ),
        ("a store-quiet update stays equal", quiet_same),
        ("the host's update after a move", host_differences == 0),
    ];
    println!("stale fields over {steps} moves: {stale_fields:?}");
    let mut passed = true;
    for (label, ok) in checks {
        println!("{label:<40} {}", if ok { "ok" } else { "FAILED" });
        passed &= ok;
    }
    passed
}

/// Remote focus part 2 step 2: the store's focus takes remote rows too, so a focus move ACROSS
/// machines must reach both lists with no frame behind it: this computer's list loses its active
/// group and its row marks, and the remote machine's list gains them, and back. A second machine
/// is streamed in (the recording has one), with its own list built for its tab; each move is judged
/// on BOTH lists, stale and after the host's empty-summary update. Returns the moves, the moves
/// whose stale probe differed on at least one list, and the updates that left a list unequal to a
/// fresh build.
fn remote_focus_moves(
    core: &mut Core,
    local_model: &mut SidebarViewModel,
    local_inputs: &SidebarInputs,
    local_row: &SessionKey,
    now_ms: u64,
    skip_update: bool,
) -> (usize, usize, usize) {
    const MACHINE: &str = "remote-replay";
    let machine = MachineId::Remote(MACHINE.to_string());
    let row = |project: &str, session: &str| {
        serde_json::json!({
            "sessionId": session, "projectId": project, "groupId": format!("{project}:active"),
            "kind": "agent", "surface": "workspace", "zmxName": format!("S90-{session}"),
            "sortKey": format!("000000000000:0:1:{session}"), "visibleInSidebarByDefault": true,
            "alias": session, "activity": "idle", "lifecycleState": "running",
            "lastInteractionAt": "2026-09-15T01:18:43.055Z",
            "createdAt": "2026-09-15T01:00:00.000Z", "updatedAt": "2026-09-15T01:18:43.055Z",
        })
    };
    let project = |project: &str| {
        serde_json::json!({
            "projectId": project, "title": project, "path": format!("/tmp/{project}"),
            "pathState": "available", "groupIds": [format!("{project}:active")],
            "sortKey": format!("1:{project}"), "createdAt": "2026-06-29T13:10:42.091Z",
            "updatedAt": "2026-09-15T01:18:43.055Z",
        })
    };
    let group = |project: &str, sessions: &[&str]| {
        serde_json::json!({
            "groupId": format!("{project}:active"), "projectId": project, "title": "Active",
            "sessionIds": sessions, "sortKey": format!("1:{project}:active"),
        })
    };
    let snapshot = serde_json::json!({
        "type": "presentationSnapshot", "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": "remote-replay", "revision": 1,
        "snapshot": {
            "revision": 1, "generatedAt": "2026-09-21T00:00:00.000Z",
            "projects": [project("R1"), project("R2")],
            "groups": [group("R1", &["RS1", "RS2"]), group("R2", &["R2S1"])],
            "sessions": [row("R1", "RS1"), row("R1", "RS2"), row("R2", "R2S1")],
        },
    });
    let changes = core
        .handle_raw_frame(machine.clone(), &snapshot.to_string(), now_ms)
        .expect("the remote frame parses")
        .changes;
    let mut remote_inputs = local_inputs.clone();
    remote_inputs.ui.selected_machine_id = MACHINE.to_string();
    remote_inputs.host.machines = vec![MachineTabInput {
        machine_id: MACHINE.to_string(),
        label: "Remote".to_string(),
        state: "connected".to_string(),
        message: None,
        fed: true,
        this_computer: false,
    }];
    let mut local_inputs = local_inputs.clone();
    local_inputs.host.machines = remote_inputs.host.machines.clone();
    let mut remote_model = SidebarViewModel::new();
    remote_model.update(core, &remote_inputs, &changes, now_ms);
    local_model.update(core, &local_inputs, &changes, now_ms);

    let rs1 = SessionKey::remote(MACHINE, "R1", "RS1");
    let r2s1 = SessionKey::remote(MACHINE, "R2", "R2S1");
    let focus = |core: &mut Core, session: &SessionKey, visible: Option<Vec<SessionKey>>| {
        core.handle(
            Event::Intent(Intent::FocusSession {
                session: session.clone(),
                visible,
            }),
            now_ms,
        );
    };
    let moves: [(&str, Box<dyn Fn(&mut Core)>); 4] = [
        (
            "a remote row, from this computer's",
            Box::new(|core: &mut Core| focus(core, &rs1, None)),
        ),
        (
            "a remote row of another project",
            Box::new(|core: &mut Core| focus(core, &r2s1, None)),
        ),
        (
            "back to this computer's row",
            Box::new(|core: &mut Core| focus(core, local_row, Some(vec![local_row.clone()]))),
        ),
        (
            "the old runtime's remote payload, mirrored",
            Box::new(|core: &mut Core| {
                let stamp = core.focus().local_stamp;
                core.handle(
                    Event::Intent(Intent::ExternalFocus(
                        ExternalFocusUpdate::new(stamp)
                            .with_active_project(rs1.project_key())
                            .with_focused_session(rs1.clone())
                            .with_visible_sessions(vec![rs1.clone()]),
                    )),
                    now_ms,
                );
            }),
        ),
    ];
    let (mut steps, mut stale_steps, mut host_differences) = (0, 0, 0);
    for (label, move_focus) in moves.iter() {
        steps += 1;
        move_focus(core);
        let mut stale_any = false;
        for (list, model, inputs) in [
            ("this computer", &mut *local_model, &local_inputs),
            ("remote", &mut remote_model, &remote_inputs),
        ] {
            let fresh = SidebarViewModel::build_from_scratch(core, inputs, now_ms);
            let stale = differing_fields(model.view(), &fresh);
            stale_any |= !stale.is_empty();
            println!("{label:<40} {list:<13} stale list differs in {stale:?}");
            if !skip_update {
                model.update(core, inputs, &ChangeSummary::default(), now_ms);
            }
            let host = differing_fields(model.view(), &fresh);
            if !host.is_empty() {
                host_differences += 1;
                println!("{label:<40} {list:<13} UPDATED list differs in {host:?}");
            }
        }
        if stale_any {
            stale_steps += 1;
        }
    }
    (steps, stale_steps, host_differences)
}

/// The field names the host's scratch record uses, for the parts a focus move can reach, plus
/// `other` for anything else.
fn differing_fields(incremental: &SidebarView, scratch: &SidebarView) -> BTreeSet<String> {
    let mut fields: BTreeSet<String> = BTreeSet::new();
    let mut note = |name: &str, same: bool| {
        if !same {
            fields.insert(name.to_string());
        }
    };
    note("spaces", incremental.spaces == scratch.spaces);
    note("order", incremental.order == scratch.order);
    note(
        "collections",
        incremental.collections == scratch.collections,
    );
    note(
        "groupIds",
        incremental
            .groups
            .iter()
            .map(|group| &group.core.group_id)
            .eq(scratch.groups.iter().map(|group| &group.core.group_id)),
    );
    for group in &incremental.groups {
        let Some(fresh) = scratch.group(&group.core.group_id) else {
            continue;
        };
        note("isActive", group.core.is_active == fresh.core.is_active);
        note("sections", group.core.sections == fresh.core.sections);
        for session in &group.core.sessions {
            let Some(other) = fresh
                .core
                .sessions
                .iter()
                .find(|other| other.row.sidebar_session_id == session.row.sidebar_session_id)
            else {
                continue;
            };
            note("isFocused", session.is_focused == other.is_focused);
            note("isVisible", session.is_visible == other.is_visible);
        }
    }
    if fields.is_empty() && incremental != scratch {
        fields.insert("other".to_string());
    }
    fields
}

/// Whether what the store would store for this machine is what `JSON.stringify` makes of the same
/// rows, byte for byte.
///
/// The check above cannot see this and that is why this one exists: it puts both sides through the
/// same Rust writer, so the two writers' agreement on TEXT was never measured and a `sidebarOrder`
/// of zero was reaching the row as `0.0` where the sidebar writes `0`. The expected side here is
/// produced by `JSON.stringify` itself, in `tooling/gx-core/last-seen-stringify.ts`.
///
/// A run that cannot produce the artefact FAILS. Zero coverage is not a pass: the whole point is
/// that the previous gate looked green while measuring nothing.
fn stringify_round_trip(
    core: &Core,
    machine: &MachineId,
    frames_path: &str,
    revision: i64,
) -> bool {
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tooling/gx-core/last-seen-stringify.ts"
    );
    let produced = std::process::Command::new("bun")
        .args([script, frames_path, &revision.to_string()])
        .output();
    let expected = match produced {
        Ok(output) if output.status.success() => match String::from_utf8(output.stdout) {
            Ok(text) if !text.is_empty() => text,
            _ => {
                println!("  the JSON.stringify artefact is empty or not UTF-8");
                return false;
            }
        },
        Ok(output) => {
            println!(
                "  bun {script} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            return false;
        }
        Err(error) => {
            println!("  cannot run bun for the JSON.stringify artefact: {error}");
            return false;
        }
    };
    let Some(written) = core.presentation().machine(machine).and_then(|entry| {
        entry
            .to_snapshot()
            .map(|snapshot| ghostex_gx_core::snapshot_storage_json(&snapshot))
    }) else {
        return false;
    };
    if expected == written {
        return true;
    }
    report_difference("JSON.stringify", &expected, &written);
    false
}

/// Whether what the store would STORE for this machine is what it was given, byte for byte.
///
/// Both sides are put through `json_stringify` because the comparison is about the payload and not
/// about whichever map `serde_json` was built with; the writer sorts for the same reason. A field
/// the wire type does not carry, a `null` normalized to a default, a row left out of `Rows` and a
/// differently ordered array all fail this.
fn stored_round_trip(core: &Core, machine: &MachineId, stored: &Value, revision: i64) -> bool {
    let Some(written) = core.presentation().machine(machine).and_then(|entry| {
        entry
            .to_snapshot()
            .map(|snapshot| ghostex_gx_core::snapshot_storage_json(&snapshot))
    }) else {
        return false;
    };
    let mut expected = stored.clone();
    expected["revision"] = Value::from(revision);
    // The recording holds a snapshot in the order gxserver BUILDS it (project by project, each
    // project's rows by `sortKey`), and what the old sidebar stores is its own cache, which
    // `orderPresentationSessions` re-sorts whole after every delta and therefore within seconds of
    // any snapshot. So the copy to match is that order, and the harness derives it here rather
    // than borrowing the writer's own comparator: a writer that dropped one of the four keys still
    // fails this.
    if let Some(sessions) = expected.get_mut("sessions").and_then(Value::as_array_mut) {
        sessions.sort_by_key(|session| {
            let field = |name: &str| {
                session
                    .get(name)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            (
                field("projectId"),
                field("groupId"),
                field("sortKey"),
                field("sessionId"),
            )
        });
    }
    let expected = ghostex_gx_core::json_stringify(&expected);
    if expected == written {
        return true;
    }
    report_difference("stored copy", &expected, &written);
    false
}

/// Says WHERE, because "not byte identical" over a 400 KB payload is not a finding anyone can act
/// on. The first byte that differs plus its surroundings names the field.
fn report_difference(label: &str, expected: &str, written: &str) {
    let at = expected
        .bytes()
        .zip(written.bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| expected.len().min(written.len()));
    let window = |text: &str| {
        let mut start = at.saturating_sub(70);
        let mut end = (at + 70).min(text.len());
        while start > 0 && !text.is_char_boundary(start) {
            start -= 1;
        }
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        text[start..end].to_string()
    };
    println!("  {label} differs at byte {at}");
    println!("    read  …{}…", window(expected));
    println!("    write …{}…", window(written));
}

/// The first `presentationSnapshot` of the recording, as its raw `snapshot` object.
fn first_snapshot(frames: &str) -> Option<Value> {
    frames.lines().find_map(|line| {
        let value: Value = serde_json::from_str(line).ok()?;
        (value.get("type")?.as_str()? == "presentationSnapshot")
            .then(|| value.get("snapshot").cloned())
            .flatten()
    })
}

/// The recorded snapshot object, parsed at a chosen revision.
fn snapshot_at(stored: &Value, revision: i64) -> ghostex_gx_core::protocol::PresentationSnapshot {
    let mut snapshot = stored.clone();
    snapshot["revision"] = Value::from(revision);
    serde_json::from_value(snapshot).expect("the recorded snapshot parses")
}

fn snapshot_frame(stored: &Value, revision: i64) -> String {
    let mut snapshot = stored.clone();
    snapshot["revision"] = Value::from(revision);
    serde_json::json!({
        "type": "presentationSnapshot",
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": "live",
        "revision": revision,
        "snapshot": snapshot,
    })
    .to_string()
}

/// A delta that removes a row the seeded snapshot really holds.
///
/// The first cut named a session that does not exist, which every revision rule would have let
/// through and which still produced an empty change summary: a probe that cannot fail, the shape
/// this port has now caught fifteen times. The victim is taken from the recorded snapshot so the
/// delta has something to remove and "refused" means the rule refused it rather than the delta
/// having nothing to do.
fn delta_frame(revision: i64, victim: &(String, String)) -> String {
    serde_json::json!({
        "type": "presentationDelta",
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": "live",
        "revision": revision,
        "delta": { "type": "sessionRemoved", "projectId": victim.0, "sessionId": victim.1 },
    })
    .to_string()
}

/// The project and session ids of the first row of a recorded snapshot.
fn first_row(stored: &Value) -> Option<(String, String)> {
    let session = stored.get("sessions")?.as_array()?.first()?;
    Some((
        session.get("projectId")?.as_str()?.to_string(),
        session.get("sessionId")?.as_str()?.to_string(),
    ))
}

fn current_frame(revision: i64) -> String {
    serde_json::json!({
        "type": "presentationSnapshotCurrent",
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": "live",
        "revision": revision,
    })
    .to_string()
}

/// Takes one row of the list, asks what has to change for it to be drawn, does exactly that, and
/// checks the row is then in a heading the list draws.
fn check_reveal(
    core: &Core,
    inputs: &SidebarInputs,
    view: &SidebarView,
    index: usize,
    now_ms: u64,
    ui: &mut UiState,
) {
    let Some(session_id) = view
        .groups
        .iter()
        .flat_map(|group| group.core.sessions.iter())
        .nth(index % 23)
        .map(|session| session.row.sidebar_session_id.clone())
    else {
        return;
    };
    let Some(plan) = reveal_plan(core, inputs, view, &session_id, now_ms) else {
        return;
    };
    ui.reveals += 1;
    let mut revealed = inputs.clone();
    if plan.show_hidden {
        revealed.ui.show_hidden = true;
    }
    if plan.clear_tag_filters {
        revealed.ui.selected_tag_filters.clear();
    }
    if let Some(space_id) = &plan.select_space {
        revealed
            .ui
            .collapse
            .selected_space_by_section
            .insert(revealed.ui.section_key(), space_id.clone());
    }
    if let Some(storage_id) = &plan.collapsed_collection_storage_id {
        revealed
            .ui
            .collapse
            .collapsed_collections
            .remove(storage_id);
    }
    if plan.collapsed_group {
        revealed.ui.collapse.collapsed_groups.remove(&plan.group_id);
    }
    if let Some(section) = plan.collapsed_section {
        revealed
            .ui
            .collapse
            .section_collapse
            .entry(plan.storage_id.clone())
            .or_default()
            .set(section, false);
    }
    let after = SidebarViewModel::build_from_scratch(core, &revealed, now_ms);
    let drawn = after.group(&plan.group_id).is_some_and(|group| {
        !group.core.collapsed
            && group
                .core
                .sections
                .iter()
                .any(|section| !section.collapsed && section.session_ids.contains(&session_id))
    });
    if !drawn {
        ui.reveal_failures += 1;
    }
}

/// Moves one piece of the sidebar's own state per frame, so every input the view model reads is
/// changed and changed back while the store moves underneath it. The sidebar's own half goes
/// through its intents and its write, exactly as the desktop host applies them.
fn churn_inputs(inputs: &mut SidebarInputs, ui: &mut UiState, view: &SidebarView, index: usize) {
    let group_id = view
        .groups
        .get(index % view.groups.len().max(1))
        .map(|group| group.core.group_id.clone());
    let storage_id = view
        .groups
        .get(index % view.groups.len().max(1))
        .map(|group| group.core.storage_id.clone());
    let session_id = view
        .groups
        .iter()
        .flat_map(|group| group.core.sessions.iter())
        .nth(index % 17)
        .map(|session| session.row.sidebar_session_id.clone());
    // Browser tabs, which only the host has: the recording carries none, and a browser row is the
    // one row whose every field comes from this side rather than from the daemon.
    if inputs.host.browser_tabs.is_empty() {
        if let Some(project_id) = view
            .groups
            .iter()
            .find_map(|group| group.core.project_context.as_ref())
            .map(|context| context.project_id.clone())
        {
            inputs.host.browser_tabs = vec![
                BrowserTabInput {
                    project_id: project_id.clone(),
                    tab_id: "1".to_string(),
                    title: "Replay tab".to_string(),
                    favicon_url: Some("https://example.invalid/favicon.ico".to_string()),
                    is_active: true,
                    is_sleeping: false,
                    is_visible: true,
                },
                BrowserTabInput {
                    project_id,
                    tab_id: "2".to_string(),
                    title: "Replay tab two".to_string(),
                    favicon_url: None,
                    is_active: false,
                    is_sleeping: true,
                    is_visible: false,
                },
            ];
        }
    }
    match index % 13 {
        0 => {
            if let Some(group_id) = group_id {
                ui.apply(SidebarUiIntent::ToggleGroupCollapsed { group_id });
            }
        }
        1 | 2 => {
            if let Some(storage_id) = storage_id {
                ui.apply(SidebarUiIntent::ToggleHoverActions { storage_id });
            }
        }
        3 => {
            if let Some(storage_id) = storage_id {
                ui.apply(SidebarUiIntent::ToggleSection {
                    storage_id,
                    section: SectionId::ORDER[index % 6],
                });
            }
        }
        4 => {
            if let Some(session_id) = session_id {
                let mut selected = ui.store.state().selected_session_ids.clone();
                match selected.iter().position(|held| *held == session_id) {
                    Some(position) => {
                        selected.remove(position);
                    }
                    None => selected.push(session_id),
                }
                ui.apply(SidebarUiIntent::SetSelectedSessions {
                    session_ids: selected,
                });
            }
        }
        5 => inputs.settings.enable_session_parking = !inputs.settings.enable_session_parking,
        6 => {
            inputs.settings.group_working_sessions = !inputs.settings.group_working_sessions;
        }
        7 => {
            inputs.settings.sort_mode = match inputs.settings.sort_mode {
                SessionSortMode::LastActivity => SessionSortMode::Manual,
                SessionSortMode::Manual => SessionSortMode::LastActivity,
            };
        }
        8 => inputs.settings.sidebar_spaces_enabled = !inputs.settings.sidebar_spaces_enabled,
        9 => ui.apply(SidebarUiIntent::ToggleShowHidden),
        // One of the two inputs that invalidate every row of the machine at once, the other
        // being the tag catalog, which the pass swaps through the store.
        10 => inputs.settings.debugging_mode = !inputs.settings.debugging_mode,
        11 => ui.apply(SidebarUiIntent::ToggleAllProjects(ToggleAllProjectsInput {
            machine_id: ui.store.selected_machine_id().to_string(),
            group_ids: view
                .groups
                .iter()
                .map(|group| group.core.group_id.clone())
                .collect(),
        })),
        _ => {
            for tag in ["favorite", "untagged"] {
                ui.apply(SidebarUiIntent::ToggleTagFilter {
                    tag: tag.to_string(),
                });
            }
        }
    }
}

/// A catalog of the replay's own, so a swap changes what every tagged row shows.
fn replay_tag_catalog() -> CustomSessionTagsState {
    let tag = |tag_id: &str, name: &str, color: &str| {
        (
            tag_id.to_string(),
            CustomSessionTag {
                tag_id: tag_id.to_string(),
                name: name.to_string(),
                color: color.to_string(),
                icon: "tag".to_string(),
            },
        )
    };
    CustomSessionTagsState {
        order: vec![
            "custom-replayalpha".to_string(),
            "custom-replaybeta".to_string(),
        ],
        tags: [
            tag("custom-replayalpha", "Replay alpha", "#7c6df2"),
            tag("custom-replaybeta", "Replay beta", "#3aa675"),
        ]
        .into_iter()
        .collect(),
    }
}

/// The catalog as a frame the core accepts: no revision, so it is applied whatever the stream's
/// own revision is at this point of the recording.
fn tag_catalog_frame(catalog: &CustomSessionTagsState) -> Option<ServerEvent> {
    let frame = serde_json::json!({
        "type": "customSessionTagsChanged",
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "customSessionTags": catalog,
    });
    ServerEvent::parse(&frame.to_string()).ok()
}

fn report(label: &str, samples: &mut [Duration]) {
    if samples.is_empty() {
        println!("{label:<38} no samples");
        return;
    }
    samples.sort_unstable();
    let percentile = |fraction: f64| -> Duration {
        let index = ((samples.len() as f64 - 1.0) * fraction).round() as usize;
        samples[index]
    };
    println!(
        "{label:<38} n={:<6} median {:>9} p99 {:>9} max {:>9}",
        samples.len(),
        micros(percentile(0.5)),
        micros(percentile(0.99)),
        micros(*samples.last().expect("not empty")),
    );
}

fn micros(duration: Duration) -> String {
    format!("{:.1}us", duration.as_nanos() as f64 / 1000.0)
}

/// Names the first place two lists differ, without quoting anything a recording holds.
fn describe_difference(scratch: &SidebarView, incremental: &SidebarView, line: usize) -> String {
    if scratch.groups.len() != incremental.groups.len() {
        return format!(
            "line {line}: group count {} from scratch, {} incrementally",
            scratch.groups.len(),
            incremental.groups.len()
        );
    }
    for (left, right) in scratch.groups.iter().zip(&incremental.groups) {
        if left.core.group_id != right.core.group_id {
            return format!("line {line}: group order differs");
        }
        if left.core.sessions.len() != right.core.sessions.len() {
            return format!(
                "line {line}: group {} has {} rows from scratch, {} incrementally",
                left.core.group_id,
                left.core.sessions.len(),
                right.core.sessions.len()
            );
        }
        for (left_session, right_session) in left.core.sessions.iter().zip(&right.core.sessions) {
            if left_session != right_session {
                return format!(
                    "line {line}: row {} differs in group {}",
                    left_session.row.sidebar_session_id, left.core.group_id
                );
            }
        }
        if left != right {
            return format!("line {line}: group {} differs", left.core.group_id);
        }
    }
    if scratch.order != incremental.order {
        return format!("line {line}: the top-level order differs");
    }
    if scratch.collections != incremental.collections {
        return format!("line {line}: the collections differ");
    }
    if scratch.spaces != incremental.spaces {
        return format!("line {line}: the Spaces differ");
    }
    format!("line {line}: the lists differ outside the groups")
}

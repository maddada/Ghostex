//! One agent in Agent Sync (packages/core-ui/agents-hub-sync/sync-agent-pane.tsx (deleted 2026-10-01)).
//!
//! CDXC:AgentSync 2026-09-22 WHY:
//! Every agent shows the same three cards in the same order as the overview's coverage cards (Skills, Instructions, Hook scripts). Each card is one sentence on the current state, then what Sync will do and what it leaves alone; the full per-skill list sits behind a disclosure because it is reference material, not something to act on.
use super::super::native_modal_kit::hsla;
use super::palette::HubPalette;
use super::sync_model::*;
use super::sync_overview::detail_body;
use super::widgets::*;
use super::window::{AgentsHubModalCommand, GpuiAgentsHubModalWindow};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};

/// One line of a part card's body: a 14px glyph, the sentence, and an optional tone for the glyph.
struct PartLine {
    glyph: &'static str,
    text: String,
    tone: Option<SyncTone>,
}

fn line(glyph: &'static str, text: String, tone: Option<SyncTone>) -> PartLine {
    PartLine { glyph, text, tone }
}

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

impl GpuiAgentsHubModalWindow {
    pub(crate) fn render_sync_agent_pane(
        &self,
        hp: &HubPalette,
        report: &SyncReport,
        agent: &SyncAgentReport,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let issues = issue_count(agent);
        let skills_groups = part_fix_groups(agent, SyncPart::Skills);
        let instruction_groups = part_fix_groups(agent, SyncPart::Instructions);
        let hooks_groups = part_fix_groups(agent, SyncPart::Hooks);
        let root = expand_home_path(&agent.root, &report.home);
        let agent_id = agent.id.clone();

        let mut actions = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .child(hub_button(
                hp,
                "agents-hub-sync-agent-open",
                HubButtonVariant::Ghost,
                HubButtonSize::Small,
                Some(ICON_FOLDER_OPEN),
                Some("Open folder".into()),
                false,
                move |this: &mut Self, _window, cx| {
                    this.send(AgentsHubModalCommand::OpenPath { path: root.clone() }, cx);
                },
                cx,
            ))
            .child(hub_button(
                hp,
                "agents-hub-sync-agent-refresh",
                HubButtonVariant::Ghost,
                HubButtonSize::Small,
                Some(ICON_REFRESH),
                None,
                false,
                |this: &mut Self, _window, cx| this.refresh_sync_report(cx),
                cx,
            ));
        if agent.detected && issues > 0 {
            let scope = agent_id.clone();
            actions = actions.child(hub_button(
                hp,
                "agents-hub-sync-agent-fix",
                HubButtonVariant::Quiet,
                HubButtonSize::Default,
                None,
                Some(format!("Review and fix {issues}…").into()),
                false,
                move |this: &mut Self, _window, cx| this.open_sync_plan(scope.clone(), None, cx),
                cx,
            ));
        } else if agent.detected {
            actions = actions.child(pill(hp, PillTone::Ok, "In sync"));
        }
        let header = h_flex()
            .w_full()
            .items_center()
            .gap(px(14.0))
            .child(sync_agent_logo(
                agent.icon.as_deref(),
                &agent.display_name,
                40.0,
                hp,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .line_height(px(26.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(hp.foreground))
                            .child(SharedString::from(agent.display_name.clone())),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(super::super::native_modal_kit::MODAL_MONO_FONT)
                            .text_size(px(12.0))
                            .text_color(hsla(hp.muted))
                            .child(SharedString::from(agent.root.clone())),
                    ),
            )
            .child(actions);

        let mut body = detail_body().child(header);
        if !agent.detected {
            let dangling = agent
                .skills
                .as_ref()
                .map_or(0, |skills| skills.counts.dangling);
            let scope = agent_id.clone();
            body = body.child(
                v_flex()
                    .items_start()
                    .gap(px(10.0))
                    .px(px(14.0))
                    .py(px(12.0))
                    .text_size(px(13.0))
                    .text_color(hsla(hp.muted))
                    .child(format!(
                        "This agent has no config folder on this computer{}.",
                        if agent.skills.is_some() {
                            ", only leftover links from an earlier install"
                        } else {
                            ""
                        }
                    ))
                    .when(dangling > 0, |this| {
                        this.child(hub_button(
                            hp,
                            "agents-hub-sync-remove-dangling",
                            HubButtonVariant::Outline,
                            HubButtonSize::Small,
                            None,
                            Some(
                                format!("Remove {}", plural(dangling, "dead link", "dead links"))
                                    .into(),
                            ),
                            false,
                            move |this: &mut Self, _window, cx| {
                                this.open_sync_plan(
                                    scope.clone(),
                                    Some(vec!["removeDangling".to_string()]),
                                    cx,
                                )
                            },
                            cx,
                        ))
                    }),
            );
            return body.into_any_element();
        }

        let skills_card = self.skills_part_card(hp, agent, &skills_groups, cx);
        let instructions_card =
            self.instructions_part_card(hp, report, agent, &instruction_groups, cx);
        let hooks_card = self.hooks_part_card(hp, agent, &hooks_groups, cx);
        body.child(
            v_flex()
                .w_full()
                .gap(px(10.0))
                .child(skills_card)
                .child(instructions_card)
                .child(hooks_card),
        )
        .into_any_element()
    }

    fn skills_part_card(
        &self,
        hp: &HubPalette,
        agent: &SyncAgentReport,
        groups: &[&'static str],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut lines = Vec::new();
        let skills = agent.skills.as_ref();
        if let Some(skills) = skills {
            let counts = &skills.counts;
            if counts.dangling > 0 {
                lines.push(line(
                    ICON_LINK_OFF,
                    format!(
                        "{} to a skill that no longer exists",
                        plural(counts.dangling, "link points", "links point")
                    ),
                    Some(SyncTone::Err),
                ));
            }
            if counts.redundant_links > 0 {
                lines.push(line(
                    ICON_LINK_OFF,
                    format!(
                        "{} in this folder, but {} reads ~/.agents/skills itself; they will be removed",
                        plural(counts.redundant_links, "shared skill is linked", "shared skills are linked"),
                        agent.display_name
                    ),
                    Some(SyncTone::Warn),
                ));
            }
            if counts.backups > 0 {
                lines.push(line(
                    ICON_COPY,
                    format!(
                        "{} will be moved to Ghostex's backup folder",
                        plural(counts.backups, "old backup folder", "old backup folders")
                    ),
                    Some(SyncTone::Warn),
                ));
            }
            if skills.dir_state == "wholeFolderLink" && !agent.universal {
                lines.push(line(
                    ICON_FOLDER,
                    "The whole skills folder is one link. It becomes a real folder with one link per skill."
                        .to_string(),
                    Some(SyncTone::Warn),
                ));
            }
            if counts.copies_identical > 0 {
                lines.push(line(
                    ICON_COPY,
                    format!(
                        "{} identical to the original, and will be replaced with links",
                        plural(
                            counts.copies_identical,
                            "skill is a copy",
                            "skills are copies"
                        )
                    ),
                    Some(SyncTone::Warn),
                ));
            }
            if counts.copies_drifted > 0 {
                lines.push(line(
                    ICON_COPY,
                    format!(
                        "{} edited here and will be kept as {}",
                        plural(counts.copies_drifted, "copy was", "copies were"),
                        if counts.copies_drifted == 1 {
                            "it is"
                        } else {
                            "they are"
                        }
                    ),
                    Some(SyncTone::Warn),
                ));
            }
            if !agent.universal && counts.missing > 0 {
                lines.push(line(
                    ICON_LINK,
                    format!(
                        "{} will be linked",
                        plural(counts.missing, "shared skill", "shared skills")
                    ),
                    None,
                ));
            }
            let own = counts.only_here + counts.linked_elsewhere;
            if own > 0 {
                lines.push(line(
                    ICON_CHECK,
                    format!(
                        "{} only in {} and will stay untouched",
                        plural(own, "skill exists", "skills exist"),
                        agent.display_name
                    ),
                    Some(SyncTone::Ok),
                ));
            }
        }
        let state = match skills {
            None => format!(
                "{} reads the shared skills folder directly, so it needs no links.",
                agent.display_name
            ),
            Some(_) if agent.universal => format!(
                "{} reads the shared skills folder directly, so it needs no links. Sync removes links and dead links here.",
                agent.display_name
            ),
            Some(skills) if groups.is_empty() => format!(
                "{} linked.",
                plural(
                    skills.counts.linked + skills.counts.via_whole_folder,
                    "shared skill is",
                    "shared skills are"
                )
            ),
            Some(skills) => {
                let counts = &skills.counts;
                let copies = counts.copies_identical + counts.copies_drifted;
                let parts: Vec<String> = [
                    (copies > 0).then(|| plural(copies, "copy", "copies")),
                    (counts.dangling > 0)
                        .then(|| plural(counts.dangling, "dead link", "dead links")),
                    (counts.missing > 0).then(|| {
                        format!(
                            "{} not linked yet",
                            plural(counts.missing, "shared skill", "shared skills")
                        )
                    }),
                    (skills.dir_state == "wholeFolderLink")
                        .then(|| "the whole folder is one link".to_string()),
                ]
                .into_iter()
                .flatten()
                .collect();
                format!("{}.", capitalize_first(&parts.join(", ")))
            }
        };
        let extra = skills
            .filter(|skills| !skills.entries.is_empty())
            .map(|skills| self.skill_list(hp, skills, cx));
        let scope = agent.id.clone();
        let fix_groups: Vec<String> = groups.iter().map(|group| group.to_string()).collect();
        part_card(
            hp,
            "skills",
            ICON_BOOK,
            "Skills",
            skills_tone(agent),
            groups.len(),
            None,
            state,
            lines,
            extra,
            move |this: &mut Self, cx| {
                this.open_sync_plan(scope.clone(), Some(fix_groups.clone()), cx)
            },
            cx,
        )
    }

    fn skill_list(
        &self,
        hp: &HubPalette,
        skills: &SyncSkillsReport,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let show = self.sync.show_skills;
        let toggle = text_link(
            hp,
            "agents-hub-sync-skill-list",
            if show {
                "Hide the skill list".to_string()
            } else {
                format!("Show all {} skills", skills.entries.len())
            },
            |this: &mut Self, _window, cx| {
                this.sync.show_skills = !this.sync.show_skills;
                cx.notify();
            },
            cx,
        );
        let mut block = v_flex()
            .self_stretch()
            .items_start()
            .gap(px(8.0))
            .child(toggle);
        if show {
            let line_color = hsla(hp.line);
            let mut rows: Vec<AnyElement> = Vec::new();
            for (bucket_index, (label, states, sub)) in SKILL_BUCKETS.iter().enumerate() {
                let entries: Vec<&SyncSkillEntry> = skills
                    .entries
                    .iter()
                    .filter(|entry| states.contains(&entry.state.as_str()))
                    .collect();
                if entries.is_empty() {
                    continue;
                }
                let expanded = self.sync.expanded_buckets.contains(label);
                let visible = if expanded {
                    entries.len()
                } else {
                    entries.len().min(6)
                };
                // `.agents-hub-sync-group-label`: no hairline above the rows it labels.
                let first = rows.is_empty();
                rows.push(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap(px(6.0))
                        .when(!first, |this| this.border_t_1().border_color(line_color))
                        .px(px(14.0))
                        .pt(px(8.8))
                        .pb(px(4.0))
                        .text_size(px(12.0))
                        .text_color(hsla(hp.muted))
                        .child(*label)
                        .child(crate::app::element::count_badge_sized(
                            entries.len().to_string(),
                            hsla(hp.foreground),
                            17.0,
                            10.5,
                        ))
                        .when_some(*sub, |row, sub| {
                            row.child(div().text_color(hsla(hp.muted)).child(format!("· {sub}")))
                        })
                        .into_any_element(),
                );
                for (index, entry) in entries.iter().take(visible).enumerate() {
                    let detail = format!(
                        "{}{}",
                        skill_entry_label(&entry.state),
                        entry
                            .link_target
                            .as_ref()
                            .map(|target| format!(" → {target}"))
                            .unwrap_or_default()
                    );
                    rows.push(
                        h_flex()
                            .w_full()
                            .min_h(px(36.0))
                            .items_center()
                            .gap(px(10.0))
                            .px(px(14.0))
                            .py(px(5.6))
                            .when(index > 0, |this| this.border_t_1().border_color(line_color))
                            .child(status_dot(hp, skill_entry_tone(&entry.state)))
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .gap(px(1.0))
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .text_size(px(13.0))
                                            .text_color(hsla(hp.foreground))
                                            .child(SharedString::from(entry.name.clone()))
                                            .when(entry.ghostex_bundled, |this| {
                                                this.child(pill(
                                                    hp,
                                                    PillTone::Neutral,
                                                    "Ghostex bundled",
                                                ))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .font_family(
                                                super::super::native_modal_kit::MODAL_MONO_FONT,
                                            )
                                            .text_size(px(11.52))
                                            .line_height(px(16.1))
                                            .text_color(hsla(hp.muted))
                                            .child(detail),
                                    ),
                            )
                            .into_any_element(),
                    );
                }
                let hidden = entries.len() - visible;
                if hidden > 0 || (expanded && entries.len() > 6) {
                    let label_key: &'static str = label;
                    rows.push(
                        div()
                            .id(("agents-hub-sync-bucket-more", bucket_index))
                            .w_full()
                            .px(px(14.0))
                            .pt(px(6.0))
                            .pb(px(8.0))
                            .border_t_1()
                            .border_color(line_color)
                            .text_size(px(12.0))
                            .text_color(hsla(hp.muted))
                            .on_click(cx.listener(
                                move |this, _: &gpui::ClickEvent, _window, cx| {
                                    if !this.sync.expanded_buckets.remove(label_key) {
                                        this.sync.expanded_buckets.insert(label_key);
                                    }
                                    cx.notify();
                                },
                            ))
                            .child(if hidden > 0 {
                                format!("+ {hidden} more")
                            } else {
                                "Show fewer".to_string()
                            })
                            .into_any_element(),
                    );
                }
            }
            // `.agents-hub-sync-skill-list`: stretched back under the part's glyph column.
            block = block.child(
                v_flex()
                    .self_stretch()
                    .ml(px(-42.0))
                    .bg(hsla(hp.raised))
                    .border_1()
                    .border_color(line_color)
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .children(rows),
            );
        }
        block.into_any_element()
    }

    fn instructions_part_card(
        &self,
        hp: &HubPalette,
        report: &SyncReport,
        agent: &SyncAgentReport,
        groups: &[&'static str],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = &agent.display_name;
        let instructions = agent.instructions.as_ref();
        let main_md_ready = report.source.main_md_ready();
        let main_md_reason = if report.source.main_md_empty {
            "empty"
        } else {
            "missing"
        };
        let state = match instructions.map(|value| value.state.as_str()) {
            None => format!("{name} has no instruction file that Agent Sync manages."),
            Some("pointer") => format!("{name} reads your shared main.md."),
            Some("danglingPointer") => format!(
                "{name}'s instruction file points at ~/.agents/main.md, which is {main_md_reason}."
            ),
            Some("missing") => {
                format!("{name} has no instruction file, so it does not read your shared main.md.")
            }
            Some("legacyPointer") => {
                "The instruction file mentions your shared main.md with older wording.".to_string()
            }
            Some("otherContent") => "The instruction file has content of its own and does not mention your shared main.md.".to_string(),
            Some(_) => "The instruction file is a link or a folder, which Agent Sync leaves alone.".to_string(),
        };
        let best_seed = report
            .source
            .main_md_seed_candidates
            .iter()
            .find(|c| c.agent_id == agent.id)
            .or_else(|| report.source.main_md_seed_candidates.first())
            .cloned();
        let lines = match instructions {
            None => Vec::new(),
            Some(value) if value.state == "danglingPointer" => {
                let fix_hint = match &best_seed {
                    Some(best) => format!(
                        "Create ~/.agents/main.md from {} ({}) first, or restore the original from Ghostex's backup folder.",
                        best.display_name, best.path
                    ),
                    None => "Create ~/.agents/main.md with your shared instructions, or restore the original from Ghostex's backup folder.".to_string(),
                };
                vec![
                    line(ICON_ALERT, value.path.clone(), Some(SyncTone::Err)),
                    line(ICON_FILE_TEXT, fix_hint, None),
                ]
            }
            Some(value) if !main_md_ready => {
                let fix_hint = match &best_seed {
                    Some(best) => format!(
                        "~/.agents/main.md is {main_md_reason}. Sync will not replace {} until main.md is created (seed from {}).",
                        value.path, best.display_name
                    ),
                    None => format!(
                        "~/.agents/main.md is {main_md_reason}. Create ~/.agents/main.md before writing pointer files."
                    ),
                };
                vec![line(ICON_FILE_TEXT, fix_hint, Some(SyncTone::Warn))]
            }
            Some(value) if value.state == "missing" => vec![line(
                ICON_FILE_TEXT,
                format!("A one-line file will be created at {}", value.path),
                None,
            )],
            Some(value) if value.state == "legacyPointer" || value.state == "otherContent" => {
                vec![line(
                    ICON_FILE_TEXT,
                    format!(
                        "{} is backed up to Ghostex's backup folder, then replaced with the one-line file (or choose Prepend / Leave alone in the plan)",
                        value.path
                    ),
                    None,
                )]
            }
            Some(value) => vec![line(ICON_FILE_TEXT, value.path.clone(), None)],
        };
        let scope = agent.id.clone();
        let mut fix_groups: Vec<String> = groups.iter().map(|group| group.to_string()).collect();
        let seed_id = (!main_md_ready)
            .then(|| best_seed.map(|c| c.agent_id))
            .flatten();
        if seed_id.is_some() && !fix_groups.iter().any(|group| group == "seedMainMd") {
            fix_groups.insert(0, "seedMainMd".to_string());
        }
        part_card(
            hp,
            "instructions",
            ICON_FILE_TEXT,
            "Instructions",
            instructions_tone(agent),
            groups.len(),
            instructions.is_none().then_some("Not used"),
            state,
            lines,
            None,
            move |this: &mut Self, cx| {
                this.open_sync_plan_with_seed(
                    scope.clone(),
                    Some(fix_groups.clone()),
                    seed_id.clone(),
                    cx,
                )
            },
            cx,
        )
    }

    fn hooks_part_card(
        &self,
        hp: &HubPalette,
        agent: &SyncAgentReport,
        groups: &[&'static str],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = match agent.hooks.as_ref().map(|hooks| hooks.state.as_str()) {
            None => format!(
                "{} does not run hook scripts from a folder, so there is nothing to share.",
                agent.display_name
            ),
            Some("linked") => "The hooks folder is linked to your shared hook scripts.".to_string(),
            Some("missing") => {
                "The hooks folder is not linked to your shared hook scripts yet.".to_string()
            }
            Some("realDir") => {
                "The hooks folder is a folder of its own. It is backed up, then linked.".to_string()
            }
            Some("nothingShared") => {
                "Your shared folder has no hooks folder, so there is nothing to link.".to_string()
            }
            Some(_) => {
                "The hooks folder links somewhere else, which Agent Sync leaves alone.".to_string()
            }
        };
        let mut lines = Vec::new();
        if let Some(hooks) = agent.hooks.as_ref() {
            lines.push(line(ICON_TERMINAL, hooks.dir.clone(), None));
            if let Some(lock) = agent
                .lock
                .as_ref()
                .filter(|lock| lock.state != "nothingShared")
            {
                let linked = lock.state == "linked";
                lines.push(line(
                    if linked { ICON_CHECK } else { ICON_LINK },
                    if linked {
                        "The skills lock file is linked too".to_string()
                    } else {
                        "The skills lock file will be linked too".to_string()
                    },
                    linked.then_some(SyncTone::Ok),
                ));
            }
        }
        let scope = agent.id.clone();
        let fix_groups: Vec<String> = groups.iter().map(|group| group.to_string()).collect();
        part_card(
            hp,
            "hooks",
            ICON_TERMINAL,
            "Hook scripts",
            hooks_tone(agent),
            groups.len(),
            match agent.hooks.as_ref().map(|hooks| hooks.state.as_str()) {
                None => Some("Not used"),
                Some("nothingShared") => Some("Nothing shared"),
                Some(_) => None,
            },
            state,
            lines,
            None,
            move |this: &mut Self, cx| {
                this.open_sync_plan(scope.clone(), Some(fix_groups.clone()), cx)
            },
            cx,
        )
    }
}

/// `PartCard`: a raised card whose head names the part with its status pill, one sentence on
/// the state and a Fix… button when something is open, over a body of what Sync will do.
#[allow(clippy::too_many_arguments)]
fn part_card(
    hp: &HubPalette,
    id: &'static str,
    glyph: &'static str,
    name: &'static str,
    tone: SyncTone,
    fix_count: usize,
    neutral_pill: Option<&'static str>,
    state: String,
    lines: Vec<PartLine>,
    extra: Option<AnyElement>,
    on_fix: impl Fn(&mut GpuiAgentsHubModalWindow, &mut Context<GpuiAgentsHubModalWindow>) + 'static,
    cx: &mut Context<GpuiAgentsHubModalWindow>,
) -> AnyElement {
    let status = match neutral_pill {
        Some(label) => pill(hp, PillTone::Neutral, label),
        None if fix_count > 0 => pill(
            hp,
            if tone == SyncTone::Err {
                PillTone::Err
            } else {
                PillTone::Warn
            },
            format!("{fix_count} to fix"),
        ),
        None => pill(hp, PillTone::Ok, "In sync"),
    };
    let tile_tone = (tone != SyncTone::Off).then_some(tone);
    let head = h_flex()
        .w_full()
        .items_center()
        .gap(px(12.0))
        .px(px(16.0))
        .py(px(14.0))
        .child(icon_tile(hp, tile_tone, glyph))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(13.44))
                        .line_height(px(19.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(hp.foreground))
                        .child(name)
                        .child(status),
                )
                .child(
                    div()
                        .text_size(px(12.48))
                        .line_height(px(18.1))
                        .text_color(hsla(hp.muted))
                        .child(state),
                ),
        )
        .when(fix_count > 0, |this| {
            this.child(hub_button(
                hp,
                SharedString::from(format!("agents-hub-sync-part-fix-{id}")),
                HubButtonVariant::Outline,
                HubButtonSize::Small,
                None,
                Some("Fix…".into()),
                false,
                move |this: &mut GpuiAgentsHubModalWindow, _window, cx| on_fix(this, cx),
                cx,
            ))
        });
    let has_body = !lines.is_empty() || extra.is_some();
    v_flex()
        .w_full()
        .bg(hsla(hp.raised))
        .border_1()
        .border_color(hsla(hp.line))
        .rounded(px(12.0))
        .overflow_hidden()
        .child(head)
        .when(has_body, |this| {
            this.child(
                v_flex()
                    .w_full()
                    .items_start()
                    .gap(px(8.0))
                    .pl(px(58.0))
                    .pr(px(16.0))
                    .pt(px(12.0))
                    .pb(px(14.0))
                    .bg(hsla(hp.detail_fill()))
                    .border_t_1()
                    .border_color(hsla(hp.line))
                    .text_size(px(12.48))
                    .line_height(px(18.1))
                    .text_color(hsla(hp.muted))
                    .children(lines.into_iter().map(|part_line| {
                        let color = part_line
                            .tone
                            .map(|tone| tone_color(hp, tone))
                            .unwrap_or(hp.muted);
                        h_flex()
                            .w_full()
                            .items_start()
                            .gap(px(8.0))
                            .child(div().mt(px(2.0)).child(icon(part_line.glyph, 14.0, color)))
                            .child(div().flex_1().min_w_0().child(part_line.text))
                    }))
                    .children(extra),
            )
        })
        .into_any_element()
}

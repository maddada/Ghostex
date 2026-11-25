//! The Integrations page's computer and browser tools, one section each: Fast Computer & Browser
//! Use, SpaceO and the embedded browser. A section holds the tool's install rows, then its skills,
//! then its macOS permissions.
//!
//! CDXC:Settings 2026-10-31 DECISION:
//! User: "can we please have everything related to computer use/spaceo/browser use and their skills in 1 place together under integrations. right now the skill is not next to the install for cua and stuff pls fix". Each tool's skill row sits right after its install rows instead of in the Agent skills list, which keeps the other skills and the Refresh and Uninstall all buttons. Supersedes the "Desktop control" section of the 2026-09-09 decision on `skills_section`.
//! SEE-ALSO: integrations/remote_machine.rs (the same sections for a picked machine).

use super::super::super::fields::{RowSpec, setting_row};
use super::super::general;
use super::*;

pub(super) const COMPUTER_BROWSER_USE_TITLE: &str = "Computer & browser use";
pub(super) const HIDDEN_SCREEN_TITLE: &str = "Computer use on a hidden screen";
pub(super) const EMBEDDED_BROWSER_TITLE: &str = "Embedded browser";

/// Says where the skills that left the Agent skills list went; its deep links land on that list.
///
/// CDXC:AgentSkills 2026-11-20: the Ghostex computer use, browser and SpaceO skills are retired (Q4262), so the note says where agents look now.
pub(super) const TOOL_SKILLS_POINTER: &str = "For computer and browser use, agents read the cua-driver skill, which Fast Computer & Browser Use below installs.";

/// The section a bundled skill's row sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SkillHome {
    ComputerBrowserUse,
    HiddenScreen,
    EmbeddedBrowser,
    AgentSkills,
}

pub(super) fn skill_home(skill_id: &str) -> SkillHome {
    match skill_id {
        "computerUse" | "browserUse" => SkillHome::ComputerBrowserUse,
        "spaceo" => SkillHome::HiddenScreen,
        "embeddedBrowserUse" => SkillHome::EmbeddedBrowser,
        _ => SkillHome::AgentSkills,
    }
}

impl IntegrationsTab {
    /// The rows of the bundled skills that sit in `home`.
    fn tool_skill_rows(
        &mut self,
        p: &SettingsPalette,
        home: SkillHome,
        status: Option<&Value>,
        checking: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let cli_ready = flag(status, "installed") == Some(true);
        let driver_installed = flag(status, "cuaDriverInstalled") == Some(true);
        visible_skills()
            .iter()
            .filter(|skill| skill_home(&skill.id) == home)
            .map(|skill| {
                self.skill_row(p, skill, status, checking, cli_ready, driver_installed, cx)
            })
            .collect()
    }

    /// Search shows Trycua's install rows and every skill row through one catalog row,
    /// `bundledAgentSkills`, so both follow `show_skills`.
    pub(super) fn computer_browser_use_section(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        show_skills: bool,
        show_permissions: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows = Vec::new();
        if show_skills {
            rows.extend(self.trycua_rows(p, status, checking, cx));
            rows.extend(self.tool_skill_rows(
                p,
                SkillHome::ComputerBrowserUse,
                status,
                checking,
                cx,
            ));
        }
        if show_permissions {
            rows.extend(self.permission_rows(
                p,
                status,
                permissions::PermissionTool::CuaDriver,
                cx,
            ));
        }
        settings_section(p, COMPUTER_BROWSER_USE_TITLE, None, None, rows)
            .map(IntoElement::into_any_element)
    }

    /// A Mac that can't run SpaceO has no install or permission rows here, and keeps the skill's
    /// row with its Needs SpaceO note, as it had in the Agent skills list.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn hidden_screen_section(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        show_spaceo: bool,
        show_skills: bool,
        show_permissions: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows = Vec::new();
        if show_spaceo {
            rows.extend(self.spaceo_rows(p, status, checking, cx));
        }
        if show_skills {
            rows.extend(self.tool_skill_rows(p, SkillHome::HiddenScreen, status, checking, cx));
        }
        if show_permissions {
            rows.extend(self.permission_rows(p, status, permissions::PermissionTool::Spaceo, cx));
        }
        settings_section(p, HIDDEN_SCREEN_TITLE, None, None, rows)
            .map(IntoElement::into_any_element)
    }

    /// The Chromium runtime is what the browser panes run on, so this section kept its row until
    /// it moved to Settings > Browser (CDXC:Settings 2026-11-26 at `SettingsTabId::RAIL_ORDER` in
    /// model.rs). The runtime also serves the VS Code view, website and extension views and HTML
    /// in Files, so `show_runtime_pointer` keeps a line here that says where it went and opens it.
    pub(super) fn embedded_browser_section(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        show_runtime_pointer: bool,
        show_skills: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows = Vec::new();
        if show_runtime_pointer {
            rows.push(setting_row(
                p,
                "webRuntimeMoved",
                RowSpec::new(general::web_runtime::TITLE).description(
                    "Moved to Settings > Browser. It also runs the VS Code view, website and extension views, and HTML files in Files.",
                ),
                None,
                settings_button(
                    p,
                    "integrations-web-runtime-open-browser",
                    "Open Browser settings",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    |page: &mut Self, _window, cx| {
                        let store = page.store.clone();
                        store.update(cx, |store, cx| {
                            store.set_active_tab(SettingsTabId::Browser, cx)
                        });
                    },
                    cx,
                ),
                cx,
            ));
        }
        if show_skills {
            rows.extend(self.tool_skill_rows(p, SkillHome::EmbeddedBrowser, status, checking, cx));
        }
        settings_section(p, EMBEDDED_BROWSER_TITLE, None, None, rows)
            .map(IntoElement::into_any_element)
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::search::extra_tab_search;
    use super::*;

    /// Every skill Settings lists has one home.
    #[test]
    fn a_tool_skill_sits_with_its_tool() {
        let skills = catalog_skills(true);
        let in_home = |home: SkillHome| -> Vec<&str> {
            skills
                .iter()
                .filter(|skill| skill_home(&skill.id) == home)
                .map(|skill| skill.id.as_str())
                .collect()
        };
        // Only the `ghostex` skill is still bundled (CDXC:AgentSkills 2026-11-20, Q4262): the tool
        // sections hold no skill rows, and Agent skills holds that one.
        assert!(in_home(SkillHome::ComputerBrowserUse).is_empty());
        assert!(in_home(SkillHome::HiddenScreen).is_empty());
        assert!(in_home(SkillHome::EmbeddedBrowser).is_empty());
        assert_eq!(in_home(SkillHome::AgentSkills), ["agentGuide"]);
    }

    /// Search still reaches every moved row through the catalog row that gates it.
    #[test]
    fn search_finds_every_moved_row() {
        let cases: &[(&str, &str)] = &[
            ("Fast Computer", "bundledAgentSkills"),
            ("cua-driver", "bundledAgentSkills"),
            ("Fast Computer & Browser Use Permissions", "cuaPermissions"),
        ];
        for (query, key) in cases {
            let search = extra_tab_search(query, "integrations", false, &[]);
            assert!(
                search.section("integrations").visible_keys.contains(*key),
                "{query} should show {key}"
            );
        }
        if cfg!(target_os = "macos") {
            for (query, key) in [
                ("SpaceO", "spaceo"),
                ("SpaceO Permissions", "spaceoPermissions"),
            ] {
                let search = extra_tab_search(query, "integrations", false, &[]);
                assert!(
                    search.section("integrations").visible_keys.contains(key),
                    "{query} should show {key}"
                );
            }
        }
    }
}

//! SpaceO's install rows, laid out like Fast Computer & Browser Use's: the SpaceO row (Install, or
//! Update, Reinstall and Uninstall once installed), its job progress and its install command.
//! tool_sections.rs puts its skill and its Accessibility and Screen Recording rows (permissions.rs)
//! after them.

use super::*;

const SPACEO_COPY_BUTTON: &str = "integrations-spaceo-copy-command";

/// `GHOSTEX_SPACEO_PRODUCT_NAME`.
pub(super) fn spaceo_name() -> String {
    let name = settings_catalog().text(SKILLS_MODULE, "GHOSTEX_SPACEO_PRODUCT_NAME");
    if name.is_empty() {
        "SpaceO".to_string()
    } else {
        name
    }
}

fn spaceo_job(status: Option<&Value>) -> InstallJob {
    install_job(
        status,
        &spaceo_name(),
        "spaceoJob",
        "spaceoInstallPlan",
        None,
    )
}

impl IntegrationsTab {
    pub(super) fn spaceo_rows(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let name = spaceo_name();
        let installed = flag(status, "spaceoInstalled") == Some(true);
        let job = spaceo_job(status);
        let mut rows = Vec::new();
        let controls = if installed {
            self.spaceo_installed_actions(p, status, checking, &job, cx)
        } else {
            let disabled = checking || job.running;
            let reason = if checking {
                format!("{name} status is being checked.")
            } else {
                job.running_reason.clone().unwrap_or_default()
            };
            let button = settings_button(
                p,
                "integrations-spaceo-install",
                if job.running {
                    format!("Installing {name}…")
                } else {
                    format!("Install {name}")
                },
                Some(if job.running {
                    ICON_LOADER
                } else {
                    ICON_DOWNLOAD
                }),
                ButtonVariant::Outline,
                disabled,
                Some(reason.into()),
                |page: &mut Self, _window, cx| page.post("installSpaceo", cx),
                cx,
            );
            // The plan is the enabled button's tooltip; a disabled one shows its reason.
            vec![match job.plan.clone().filter(|_| !disabled) {
                Some(plan) => div()
                    .id("integrations-spaceo-install-plan")
                    .tooltip(tooltip_text(plan))
                    .child(button)
                    .into_any_element(),
                None => button,
            }]
        };
        let installed_prefix = if installed {
            match text(status, "spaceoVersion") {
                Some(version) => format!("Version {version} installed. "),
                None => "Installed. ".to_string(),
            }
        } else {
            String::new()
        };
        rows.push(integration_row(
            p,
            "spaceo",
            Some(if checking {
                ListItemStatus::Neutral
            } else if installed {
                ListItemStatus::Success
            } else {
                ListItemStatus::Warning
            }),
            Some(ICON_DEVICE_LAPTOP),
            RowTitle {
                link: None,
                label: name.clone(),
                description: format!(
                    "{installed_prefix}{name} gives agents their own hidden screen on this Mac: apps open on a virtual display, so agents click, type and take screenshots there while you keep your own screen, pointer and focus. Installing it also installs the Ghostex SpaceO skill. Needs an Apple Silicon Mac with macOS 14 or later."
                ),
                badge: None,
                pill: None,
            },
            controls,
        ));
        if let Some(detail) = job.detail.clone() {
            let output = job.output.trim();
            rows.push(integration_row(
                p,
                "spaceo-job",
                None,
                None,
                RowTitle {
                    link: None,
                    label: detail,
                    description: if output.is_empty() {
                        job.plan.clone().unwrap_or_default()
                    } else {
                        output
                            .lines()
                            .rev()
                            .take(12)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect::<Vec<_>>()
                            .join("\n")
                    },
                    badge: None,
                    pill: None,
                },
                Vec::new(),
            ));
        }
        if !installed && let Some(command) = text(status, "spaceoInstallCommand") {
            rows.push(self.spaceo_install_command_row(p, &name, command.to_string(), cx));
        }
        rows
    }

    /// Update (or the "up to date" check), Reinstall and Uninstall, as icon buttons like Trycua's.
    fn spaceo_installed_actions(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        job: &InstallJob,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let name = spaceo_name();
        let current = text(status, "spaceoVersion");
        let installed_suffix = current
            .map(|current| format!(" (installed v{current})"))
            .unwrap_or_default();
        let checking_reason = job
            .running_reason
            .clone()
            .unwrap_or_else(|| format!("{name} status is being checked."));
        let busy = checking || job.running;
        let running = |operations: &[&str]| {
            job.operation
                .as_deref()
                .is_some_and(|operation| operations.contains(&operation))
        };
        let update = updates::UpdateButton {
            id: "integrations-spaceo-update",
            name: name.clone(),
            prefix: "spaceo",
            update_message: "installSpaceo",
            check_message: "checkSpaceoUpdate",
            update_note: Some(format!("Agents using {name} finish first.")),
            updating: running(&["update"]),
            disabled: busy,
            disabled_reason: checking_reason.clone(),
        };
        vec![
            self.update_button(p, status, update, cx),
            ghost_icon_button(
                p,
                "integrations-spaceo-reinstall",
                if running(&["reinstall", "install"]) {
                    ICON_LOADER
                } else {
                    ICON_REFRESH
                },
                p.foreground,
                format!(
                    "Reinstall the latest {name}{installed_suffix}. {}",
                    job.plan.clone().unwrap_or_default()
                )
                .trim()
                .to_string(),
                busy,
                checking_reason.clone(),
                |page, _window, cx| page.post("reinstallSpaceo", cx),
                cx,
            ),
            ghost_icon_button(
                p,
                "integrations-spaceo-uninstall",
                if running(&["uninstall"]) {
                    ICON_LOADER
                } else {
                    ICON_TRASH
                },
                p.foreground,
                format!(
                    "Uninstall {name} (ends any agent's {name} session; keeps Accessibility and Screen Recording permissions)"
                ),
                busy,
                checking_reason,
                |page, _window, cx| page.post("uninstallSpaceo", cx),
                cx,
            ),
        ]
    }

    fn spaceo_install_command_row(
        &mut self,
        p: &SettingsPalette,
        name: &str,
        command: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let copied = self.copied == Some(SPACEO_COPY_BUTTON);
        let command_for_copy = command.clone();
        let code = div()
            .flex_shrink_1()
            .min_w_0()
            .max_w(px(416.0))
            .px(px(10.0))
            .py(px(4.8))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.surface))
            .font_family(MODAL_MONO_FONT)
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(hsla(p.muted))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(command)
            .into_any_element();
        let copy = ghost_icon_button(
            p,
            SPACEO_COPY_BUTTON,
            if copied {
                ICON_CIRCLE_CHECK_FILLED
            } else {
                ICON_COPY
            },
            p.foreground,
            if copied { "Copied" } else { "Copy command" }.to_string(),
            false,
            String::new(),
            move |page, _window, cx| {
                page.copy_command(SPACEO_COPY_BUTTON, command_for_copy.clone(), cx)
            },
            cx,
        );
        integration_row(
            p,
            "spaceo-install-command",
            None,
            None,
            RowTitle {
                link: None,
                label: "Install command".to_string(),
                description: format!(
                    "Install {name} runs this command in the background, then keeps {name} running and installs the Ghostex SpaceO skill. You can also run it yourself."
                ),
                badge: None,
                pill: None,
            },
            vec![code, copy],
        )
    }
}

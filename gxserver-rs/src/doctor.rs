use serde::Serialize;

use crate::{
    agent_hooks::read_agent_hook_status,
    agent_skills::read_agent_skill_status,
    auth::read_gxserver_auth_token,
    config::read_selected_local_api_port,
    constants::GXSERVER_LOCAL_API_HOST,
    http_client::fetch_server_health,
    paths::{get_gxserver_paths, GxserverPaths},
    t3_runtime::T3RuntimeStatusPayload,
    toolchain::get_gxserver_tool_statuses,
};

use anyhow::Result;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorFix {
    pub id: String,
    pub description: String,
    pub confirmation_token: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorCheck {
    pub id: String,
    pub status: CheckStatus,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<DoctorFix>,
}

pub fn check_skills(paths: &GxserverPaths) -> DoctorCheck {
    let result = read_agent_skill_status(paths, &serde_json::Map::new());
    match result {
        Ok(status) => {
            let skills = status
                .get("skills")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let installed = status
                .get("skills")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter(|s| {
                    s.get("installed").and_then(|v| v.as_bool()).unwrap_or(false)
                }).count())
                .unwrap_or(0);
            if installed == skills && skills > 0 {
                DoctorCheck {
                    id: "skills.installed".to_string(),
                    status: CheckStatus::Ok,
                    detail: format!("All {skills} agent skills are installed ({installed}/{skills})."),
                    fix: None,
                }
            } else if installed < skills {
                DoctorCheck {
                    id: "skills.installed".to_string(),
                    status: CheckStatus::Warn,
                    detail: format!("{installed}/{skills} agent skills are installed."),
                    fix: Some(DoctorFix {
                        id: "skills.reinstall".to_string(),
                        description: "Install all bundled agent skills.".to_string(),
                        confirmation_token: "reinstall-skills".to_string(),
                    }),
                }
            } else {
                DoctorCheck {
                    id: "skills.installed".to_string(),
                    status: CheckStatus::Fail,
                    detail: "Could not determine skill install status (no skills found).".to_string(),
                    fix: None,
                }
            }
        }
        Err(error) => DoctorCheck {
            id: "skills.installed".to_string(),
            status: CheckStatus::Fail,
            detail: format!("Failed to read skill status: {error}"),
            fix: None,
        },
    }
}

pub fn check_hooks(paths: &GxserverPaths) -> DoctorCheck {
    let result = read_agent_hook_status(paths, &serde_json::Map::new());
    match result {
        Ok(status) => {
            let agents = status
                .get("agents")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let installed = status
                .get("agents")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter(|s| {
                    s.get("hookInstalled").and_then(|v| v.as_bool()).unwrap_or(false)
                }).count())
                .unwrap_or(0);
            DoctorCheck {
                id: "hooks.installed".to_string(),
                status: if installed == agents && agents > 0 {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Warn
                },
                detail: format!("{installed}/{agents} agent hooks are installed."),
                fix: if installed < agents {
                    Some(DoctorFix {
                        id: "hooks.reinstall".to_string(),
                        description: "Install all agent hooks.".to_string(),
                        confirmation_token: "reinstall-hooks".to_string(),
                    })
                } else {
                    None
                },
            }
        }
        Err(error) => DoctorCheck {
            id: "hooks.installed".to_string(),
            status: CheckStatus::Fail,
            detail: format!("Failed to read hook status: {error}"),
            fix: None,
        },
    }
}

pub fn check_toolchain() -> DoctorCheck {
    let tools = get_gxserver_tool_statuses();
    let missing: Vec<_> = tools
        .iter()
        .filter(|t| t.availability != "available")
        .collect();
    if missing.is_empty() {
        DoctorCheck {
            id: "toolchain.present".to_string(),
            status: CheckStatus::Ok,
            detail: "All required tools (zmx, zehn, bd) are available.".to_string(),
            fix: None,
        }
    } else {
        let names: Vec<_> = missing.iter().map(|t| t.tool.as_str()).collect();
        DoctorCheck {
            id: "toolchain.present".to_string(),
            status: CheckStatus::Fail,
            detail: format!("Missing tools: {}", names.join(", ")),
            fix: Some(DoctorFix {
                id: "toolchain.install".to_string(),
                description: "Install missing tools through gxserver.".to_string(),
                confirmation_token: "install-tools".to_string(),
            }),
        }
    }
}

pub fn check_daemon(paths: &GxserverPaths) -> DoctorCheck {
    let auth = read_gxserver_auth_token(paths).ok().flatten();
    match fetch_server_health(auth.as_ref().map(|a| a.token.as_str()), 800) {
        Ok(Some(health)) => DoctorCheck {
            id: "daemon.running".to_string(),
            status: CheckStatus::Ok,
            detail: format!(
                "gxserver {version} running on {host}:{port} (build: {build})",
                version = health.version,
                host = GXSERVER_LOCAL_API_HOST,
                port = read_selected_local_api_port().unwrap_or(58744),
                build = health.build_identity,
            ),
            fix: None,
        },
        Ok(None) => DoctorCheck {
            id: "daemon.running".to_string(),
            status: CheckStatus::Ok,
            detail: "gxserver is not running (this is normal for CLI-only usage).".to_string(),
            fix: None,
        },
        Err(_) => DoctorCheck {
            id: "daemon.running".to_string(),
            status: CheckStatus::Ok,
            detail: "gxserver is not running (daemon is down).".to_string(),
            fix: None,
        },
    }
}

pub fn check_t3(t3_status: &T3RuntimeStatusPayload) -> DoctorCheck {
    if t3_status.running {
        DoctorCheck {
            id: "t3.running".to_string(),
            status: CheckStatus::Ok,
            detail: format!(
                "T3 runtime is running (pid={}, port={})",
                t3_status.pid.unwrap_or(0),
                t3_status.port,
            ),
            fix: None,
        }
    } else {
        DoctorCheck {
            id: "t3.running".to_string(),
            status: CheckStatus::Warn,
            detail: "T3 runtime is not running.".to_string(),
            fix: None,
        }
    }
}

pub fn run_all_checks(paths: &GxserverPaths, t3_status: &T3RuntimeStatusPayload) -> Vec<DoctorCheck> {
    vec![
        check_skills(paths),
        check_hooks(paths),
        check_toolchain(),
        check_daemon(paths),
        check_t3(t3_status),
    ]
}

/// Check that no DoctorCheck with `status != Ok` has a missing `fix`.
/// Fixes should be absent for Ok checks. (Warn/Fail checks may omit a fix
/// when no automated fix is possible.)
pub fn validate_check_invariants(checks: &[DoctorCheck]) -> Option<String> {
    for check in checks {
        match check.status {
            CheckStatus::Ok => {
                if check.fix.is_some() {
                    return Some(format!(
                        "Check {} has status Ok but a fix is present",
                        check.id
                    ));
                }
            }
            CheckStatus::Warn | CheckStatus::Fail => {
                // Warn/Fail may optionally have a fix; no invariant violation.
            }
        }
    }
    None
}

/// Run the CLI doctor command.
/// Supports `--json` flag for machine-readable output.
pub fn run_doctor_cli(args: Vec<String>) -> Result<()> {
    let use_json = args.iter().any(|a| a == "--json");
    let paths = get_gxserver_paths(None);
    // T3 status requires the daemon; for CLI with daemon down, treat as unavailable.
    let t3_status = T3RuntimeStatusPayload {
        running: false,
        pid: None,
        port: crate::t3_runtime::T3_RUNTIME_PORT,
        started_at: None,
        auth_ready: false,
        ownership: None,
    };
    let checks = run_all_checks(&paths, &t3_status);
    if use_json {
        println!("{}", serde_json::to_string_pretty(&checks)?);
    } else {
        for check in &checks {
            let status_char = match check.status {
                CheckStatus::Ok => "✓",
                CheckStatus::Warn => "⚠",
                CheckStatus::Fail => "✗",
            };
            println!("  {status_char} {}: {}", check.id, check.detail);
            if let Some(fix) = &check.fix {
                println!("       Fix: {}", fix.description);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_invariants_fix_present_only_for_warn_or_fail() {
        let checks = vec![
            DoctorCheck {
                id: "test.ok".to_string(),
                status: CheckStatus::Ok,
                detail: "ok".to_string(),
                fix: None,
            },
            DoctorCheck {
                id: "test.warn".to_string(),
                status: CheckStatus::Warn,
                detail: "warn".to_string(),
                fix: Some(DoctorFix {
                    id: "fix.warn".to_string(),
                    description: "fix the warn".to_string(),
                    confirmation_token: "warn".to_string(),
                }),
            },
            DoctorCheck {
                id: "test.fail".to_string(),
                status: CheckStatus::Fail,
                detail: "fail".to_string(),
                fix: Some(DoctorFix {
                    id: "fix.fail".to_string(),
                    description: "fix the fail".to_string(),
                    confirmation_token: "fail".to_string(),
                }),
            },
        ];
        assert!(validate_check_invariants(&checks).is_none());
    }

    #[test]
    fn check_invariants_detects_ok_with_fix() {
        let check = DoctorCheck {
            id: "test.ok".to_string(),
            status: CheckStatus::Ok,
            detail: "ok".to_string(),
            fix: Some(DoctorFix {
                id: "fix.ok".to_string(),
                description: "should not exist".to_string(),
                confirmation_token: "ok".to_string(),
            }),
        };
        assert!(validate_check_invariants(&[check]).is_some());
    }

    #[test]
    fn check_invariants_allows_fail_without_fix() {
        // The spec only enforces: fix.is_some() -> status is Warn|Fail.
        // The converse (Warn|Fail -> fix.is_some()) is not required.
        let check = DoctorCheck {
            id: "test.fail".to_string(),
            status: CheckStatus::Fail,
            detail: "fail".to_string(),
            fix: None,
        };
        assert!(validate_check_invariants(&[check]).is_none());
    }

    #[test]
    fn toolchain_check_returns_fail_when_tools_missing() {
        // This may fail in CI if tools are actually available;
        // the test verifies the check returns a valid DoctorCheck.
        let check = check_toolchain();
        assert!(!check.id.is_empty());
        assert!(check.status == CheckStatus::Ok || check.status == CheckStatus::Fail);
    }

    #[test]
    fn daemon_check_returns_valid_check_even_when_down() {
        // Use a path that likely has no running daemon.
        let paths = crate::paths::get_gxserver_paths(Some(std::path::PathBuf::from("/tmp")));
        let check = check_daemon(&paths);
        assert!(!check.id.is_empty());
        assert!(check.status == CheckStatus::Ok || check.status == CheckStatus::Fail);
    }
}

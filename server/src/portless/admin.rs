use std::collections::{BTreeMap, HashMap};

use anyhow::{anyhow, bail, ensure, Context, Result};
use rusqlite::Connection;

use crate::toolchain::require_bundled_zmx;

use super::listener_discovery::*;
use super::repository::*;
use super::sync::*;
use super::types::*;

pub(crate) fn is_portless_installed_setup_ownership(ownership: PortlessSetupOwnership) -> bool {
    matches!(
        ownership,
        PortlessSetupOwnership::Ghostex | PortlessSetupOwnership::Standalone
    )
}

pub(crate) fn portless_admin_action_set(state: &PortlessState) -> PortlessAdminActionSet {
    let recommended = recommended_portless_admin_action(state);
    PortlessAdminActionSet {
        install: portless_admin_action_availability(
            recommended == Some(PortlessAdminActionKind::Install),
        ),
        reconfigure: portless_admin_action_availability(
            recommended == Some(PortlessAdminActionKind::Reconfigure),
        ),
        remove: portless_admin_action_availability(false),
        retry: portless_admin_action_availability(
            recommended == Some(PortlessAdminActionKind::Retry),
        ),
    }
}

fn portless_admin_action_availability(recommended: bool) -> PortlessAdminActionAvailability {
    PortlessAdminActionAvailability {
        available: false,
        local_mac_only: true,
        recommended,
        unavailable_reason: Some(if recommended {
            PortlessAdminActionUnavailableReason::NativeAdminBridgeRequired
        } else {
            PortlessAdminActionUnavailableReason::NotRecommended
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortlessAdminActionKind {
    Install,
    Reconfigure,
    Retry,
}

pub(crate) fn recommended_portless_admin_action(
    state: &PortlessState,
) -> Option<PortlessAdminActionKind> {
    if !state.enabled || state.setup_status == PortlessSetupStatus::Disabled {
        return None;
    }
    match (state.setup_ownership, state.setup_status) {
        (PortlessSetupOwnership::Missing, PortlessSetupStatus::Needed) => {
            Some(PortlessAdminActionKind::Install)
        }
        (PortlessSetupOwnership::Ghostex, PortlessSetupStatus::Needed) => {
            Some(PortlessAdminActionKind::Reconfigure)
        }
        (PortlessSetupOwnership::Ghostex, PortlessSetupStatus::Failed) => {
            Some(PortlessAdminActionKind::Retry)
        }
        _ => None,
    }
}

pub(crate) fn portless_route_previews_for_desired_routes(
    protocol: PortlessProtocol,
    listeners: &[PortlessOwnedListener],
    routes: &[PortlessRoute],
) -> Vec<PortlessRoutePreview> {
    let mut listeners_by_target = HashMap::<(u16, u32), Vec<&PortlessOwnedListener>>::new();
    for listener in listeners {
        listeners_by_target
            .entry((listener.port, listener.pid))
            .or_default()
            .push(listener);
    }

    let mut route_previews = Vec::new();
    for route in routes {
        let Some(listener) = listeners_by_target
            .get_mut(&(route.port, route.pid))
            .and_then(|candidates| candidates.pop())
        else {
            continue;
        };
        route_previews.push(PortlessRoutePreview {
            hostname: route.hostname.clone(),
            kind: portless_route_preview_kind(route),
            port: route.port,
            project_id: listener.project_id.clone(),
            protocol,
            session_id: listener.session_id.clone(),
        });
    }
    route_previews
}

fn portless_route_preview_kind(route: &PortlessRoute) -> PortlessRoutePreviewKind {
    if route.hostname.starts_with(&format!("p{}.", route.port)) {
        PortlessRoutePreviewKind::Additional
    } else {
        PortlessRoutePreviewKind::Primary
    }
}

pub fn compute_live_portless_owned_listeners(
    db: &Connection,
) -> Result<Vec<PortlessOwnedListener>> {
    let sessions = list_portless_listener_candidate_sessions(db)?;
    if sessions.is_empty() {
        return Ok(Vec::new());
    }

    let zmx = require_bundled_zmx().map_err(|_| {
        anyhow!("Ghostex bundled zmx is unavailable for Portless listener detection.")
    })?;
    let output = run_portless_listener_snapshot_command(
        &build_portless_listener_snapshot_command(&zmx.executable_path),
    )?;
    if output.stdout_truncated {
        bail!("Portless listener snapshot output exceeded the safety limit.");
    }
    if output.exit_code != 0 {
        return Ok(Vec::new());
    }
    let snapshot = parse_portless_listener_snapshot_sections(&output.stdout);
    Ok(compute_portless_owned_listeners_for_sessions(
        &sessions,
        &snapshot.zmx_list_output,
        &snapshot.ps_output,
        &snapshot.listener_output,
    ))
}

pub fn compute_portless_owned_listeners_from_snapshot(
    db: &Connection,
    zmx_list_output: &str,
    ps_output: &str,
    listener_output: &str,
) -> Result<Vec<PortlessOwnedListener>> {
    let sessions = list_portless_listener_candidate_sessions(db)?;
    Ok(compute_portless_owned_listeners_for_sessions(
        &sessions,
        zmx_list_output,
        ps_output,
        listener_output,
    ))
}

pub fn compute_desired_portless_routes(
    db: &Connection,
    listeners: &[PortlessOwnedListener],
) -> Result<Vec<PortlessRoute>> {
    let repository = PortlessRepository::new(db);
    let mut groups = BTreeMap::<String, Vec<PortlessRouteTarget>>::new();

    for listener in listeners {
        ensure!(
            listener.pid > 0,
            "Portless desired routes must preserve a nonzero live listener pid."
        );
        let base_domain = portless_base_domain_for_listener(&repository, listener)?;
        groups
            .entry(base_domain)
            .or_default()
            .push(PortlessRouteTarget {
                port: listener.port,
                pid: listener.pid,
            });
    }

    let mut routes = Vec::new();
    for (base_domain, mut targets) in groups {
        targets.sort_by(|left, right| {
            left.port
                .cmp(&right.port)
                .then_with(|| left.pid.cmp(&right.pid))
        });
        let primary_index = primary_portless_route_target_index(&targets)
            .with_context(|| "Portless route group must contain at least one listener")?;
        let primary = targets.remove(primary_index);
        routes.push(PortlessRoute {
            hostname: base_domain.clone(),
            port: primary.port,
            pid: primary.pid,
        });
        for target in targets {
            routes.push(PortlessRoute {
                hostname: format!("p{}.{}", target.port, base_domain),
                port: target.port,
                pid: target.pid,
            });
        }
    }

    validate_portless_routes(&routes)?;
    Ok(routes)
}

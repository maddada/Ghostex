use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::{protocol::StatusResponse, runtime::is_process_running};

pub(super) fn health_timed_out(error: &anyhow::Error) -> bool {
    error.downcast_ref::<std::io::Error>().is_some_and(|error| {
        matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        )
    })
}

/// CDXC:ServerDaemon 2026-10-06 WHY:
/// An unanswered health probe does not mean a recorded live daemon has exited. Give it bounded time to respond before startup chooses reuse or replacement; starting another foreground server while it lives can collide with its listener.
pub(super) async fn wait_for_live_unreachable(
    mut status: StatusResponse,
    patience: Duration,
    mut probe: impl FnMut(u64) -> Result<StatusResponse>,
) -> Result<StatusResponse> {
    let deadline = Instant::now() + patience;
    while status.state == "unreachable" {
        let Some(pid) = status
            .metadata
            .as_ref()
            .map(|metadata| metadata.pid)
            .filter(|pid| is_process_running(*pid))
        else {
            break;
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(anyhow!(
                "gxserver pid {pid} is running but did not answer health checks within {} ms. Wait for it to respond or explicitly stop its control plane before retrying.",
                patience.as_millis()
            ));
        }
        status = probe(remaining.as_millis().max(1) as u64)?;
        if status.state == "unreachable" {
            tokio::time::sleep(
                Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())),
            )
            .await;
        }
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{constants::GXSERVER_PRODUCT, protocol::RuntimeMetadata};

    fn live_unreachable() -> StatusResponse {
        StatusResponse {
            health: None,
            metadata: Some(RuntimeMetadata {
                build_identity: "test".into(),
                pid: std::process::id(),
                port: 1,
                protocol_version: 1,
                server_id: "test".into(),
                started_at: "test".into(),
                version: "test".into(),
            }),
            message: "Unreachable health".into(),
            ok: false,
            product: GXSERVER_PRODUCT.into(),
            state: "unreachable".into(),
        }
    }

    #[tokio::test]
    async fn live_unreachable_pid_blocks_start_after_bounded_probes() {
        let status = live_unreachable();
        let mut probes = 0;
        let result =
            wait_for_live_unreachable(status.clone(), Duration::from_millis(20), |timeout| {
                assert!((1..=20).contains(&timeout));
                probes += 1;
                Ok(status.clone())
            })
            .await;
        assert!(probes > 0);
        let error = result.unwrap_err().to_string();
        assert!(error.contains(&format!("pid {} is running", std::process::id())));
        assert!(error.contains("within 20 ms"));
    }

    #[tokio::test]
    async fn recovered_health_returns_for_normal_reuse() {
        let mut running = live_unreachable();
        running.state = "running".into();
        running.ok = true;
        let result =
            wait_for_live_unreachable(live_unreachable(), Duration::from_secs(5), |timeout| {
                assert!(timeout > 800);
                Ok(running.clone())
            })
            .await
            .unwrap();
        assert!(result.ok);
        assert_eq!(result.state, "running");
    }

    #[test]
    fn only_socket_timeouts_are_retryable() {
        for kind in [std::io::ErrorKind::TimedOut, std::io::ErrorKind::WouldBlock] {
            assert!(health_timed_out(&std::io::Error::from(kind).into()));
        }
        assert!(!health_timed_out(&anyhow!("gxserver protocol mismatch")));
        assert!(!health_timed_out(
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()
        ));
    }
}

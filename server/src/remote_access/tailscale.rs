use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/*
CDXC:RemotePairing 2026-09-03:
Tailscale is the second phone path (SSH over the tailnet). gxserver only READS
the local client's state through `tailscale status --json`; it never installs,
signs in, or changes Tailscale settings. A missing binary is a reported status
the UI turns into "Install Tailscale", not an error.
*/
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStatus {
    pub installed: bool,
    pub running: bool,
    pub account: Option<String>,
    pub magic_dns_name: Option<String>,
    pub ip: Option<String>,
    pub ssh_enabled: Option<bool>,
}

impl TailscaleStatus {
    fn not_installed() -> Self {
        Self {
            installed: false,
            running: false,
            account: None,
            magic_dns_name: None,
            ip: None,
            ssh_enabled: None,
        }
    }

    fn installed_not_running() -> Self {
        Self {
            installed: true,
            ..Self::not_installed()
        }
    }

    /// True when a phone can reach this computer over the tailnet by name or IP.
    pub fn reachable_address(&self) -> Option<(Option<String>, Option<String>)> {
        if !self.running || (self.magic_dns_name.is_none() && self.ip.is_none()) {
            return None;
        }
        Some((self.magic_dns_name.clone(), self.ip.clone()))
    }
}

pub fn read_tailscale_status() -> TailscaleStatus {
    let Some(binary) = resolve_tailscale_binary() else {
        return TailscaleStatus::not_installed();
    };
    let output = Command::new(&binary)
        .args(["status", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return TailscaleStatus::installed_not_running();
    };
    let Ok(status) = serde_json::from_slice::<Value>(&output.stdout) else {
        return TailscaleStatus::installed_not_running();
    };
    tailscale_status_from_json(&status)
}

fn tailscale_status_from_json(status: &Value) -> TailscaleStatus {
    let running = status.get("BackendState").and_then(Value::as_str) == Some("Running");
    if !running {
        return TailscaleStatus::installed_not_running();
    }
    let node = status.get("Self");
    let magic_dns_name = node
        .and_then(|node| node.get("DNSName"))
        .and_then(Value::as_str)
        .map(|name| name.trim_end_matches('.').to_string())
        .filter(|name| !name.is_empty());
    let ip = node
        .and_then(|node| node.get("TailscaleIPs"))
        .and_then(Value::as_array)
        .and_then(|ips| ips.first())
        .and_then(Value::as_str)
        .map(str::to_string);
    let ssh_enabled = node
        .and_then(|node| node.get("SSH_HostKeys"))
        .and_then(Value::as_array)
        .map(|keys| !keys.is_empty())
        .unwrap_or(false);
    let login_name = node
        .and_then(|node| node.get("UserID"))
        .and_then(Value::as_i64)
        .and_then(|user_id| {
            status
                .get("User")
                .and_then(|users| users.get(user_id.to_string()))
                .and_then(|user| user.get("LoginName"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);
    let tailnet_name = status
        .get("CurrentTailnet")
        .and_then(|tailnet| tailnet.get("Name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    TailscaleStatus {
        installed: true,
        running: true,
        account: login_name.or(tailnet_name),
        magic_dns_name,
        ip,
        ssh_enabled: Some(ssh_enabled),
    }
}

pub fn resolve_tailscale_binary() -> Option<PathBuf> {
    tailscale_binary_candidates()
        .into_iter()
        .find(|candidate| is_executable_file(candidate))
}

fn tailscale_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("PATH") {
        candidates
            .extend(env::split_paths(&path).map(|directory| directory.join(executable_name())));
    }
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ));
    }
    if cfg!(windows) {
        candidates.push(PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe"));
    } else {
        candidates.push(PathBuf::from("/usr/bin/tailscale"));
    }
    candidates
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "tailscale.exe"
    } else {
        "tailscale"
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

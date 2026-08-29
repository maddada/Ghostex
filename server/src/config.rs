use std::{env, fs, io, path::PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    constants::{GXSERVER_DEV_LOCAL_API_PORT_ENV, GXSERVER_LOCAL_API_PORT, GXSERVER_PRODUCT},
    paths::GxserverPaths,
    protocol::{ListenerConfig, ListenersConfig},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dist_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GxserverConfig {
    pub cors: CorsConfig,
    pub created_at: String,
    pub listeners: ListenersConfig,
    pub product: String,
    pub web: WebConfig,
}

const DEFAULT_CORS_ALLOWED_ORIGINS: &[&str] = &[
    "null",
    "http://127.0.0.1:4173",
    "http://127.0.0.1:5173",
    "http://127.0.0.1:6006",
    "http://localhost:4173",
    "http://localhost:5173",
    "http://localhost:6006",
];

/*
CDXC:GxserverApi 2026-06-14-20:37:
The local Rust listener defaults to 127.0.0.1:58744 and the remote listener remains disabled by default. This preserves the TypeScript CLI/native/sidebar assumptions while the port runs side by side.

CDXC:GxserverRustPort 2026-06-14-21:58:
Compatibility runs may explicitly select a different loopback port with GHOSTEX_GXSERVER_DEV_PORT while the packaged daemon owns 58744. The alternate port is a dev/compat opt-in for the selected process, not a config fallback or a product default change.

CDXC:GxserverApi 2026-06-22-04:38:
Config files may not move the local gxserver API. Reject local host or port overrides before startup writes runtime metadata so Rust reports the same fixed-listener config error as TypeScript instead of silently starting on the default port.
*/
pub fn create_default_gxserver_config() -> Result<GxserverConfig> {
    let local_port = read_selected_local_api_port()?;
    Ok(GxserverConfig {
        cors: CorsConfig {
            allowed_origins: DEFAULT_CORS_ALLOWED_ORIGINS
                .iter()
                .map(|origin| (*origin).to_string())
                .collect(),
        },
        created_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        listeners: ListenersConfig {
            local: ListenerConfig::local_with_port(local_port),
            remote: ListenerConfig::remote_default(),
        },
        product: GXSERVER_PRODUCT.to_string(),
        web: WebConfig::default(),
    })
}

pub fn read_gxserver_config(paths: &GxserverPaths) -> Result<GxserverConfig> {
    match fs::read_to_string(&paths.config_file) {
        Ok(text) => {
            let parsed: serde_json::Value =
                serde_json::from_str(&text).with_context(|| "parse gxserver config")?;
            merge_gxserver_config(parsed)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => create_default_gxserver_config(),
        Err(error) => Err(error).with_context(|| "read gxserver config"),
    }
}

pub fn write_default_config_if_missing(paths: &GxserverPaths) -> Result<()> {
    let config = create_default_gxserver_config()?;
    let text = format!("{}\n", serde_json::to_string_pretty(&config)?);
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.config_file)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(text.as_bytes())?;
            set_file_mode_0600(&paths.config_file)?;
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error).with_context(|| "write gxserver default config"),
    }
}

pub fn read_selected_local_api_port() -> Result<u16> {
    let Some(raw_port) = env::var(GXSERVER_DEV_LOCAL_API_PORT_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(GXSERVER_LOCAL_API_PORT);
    };
    let port = raw_port.parse::<u16>().map_err(|_| {
        anyhow!("{GXSERVER_DEV_LOCAL_API_PORT_ENV} must be an integer from 1 to 65535.")
    })?;
    if port == 0 {
        return Err(anyhow!(
            "{GXSERVER_DEV_LOCAL_API_PORT_ENV} must be an integer from 1 to 65535."
        ));
    }
    Ok(port)
}

fn merge_gxserver_config(value: serde_json::Value) -> Result<GxserverConfig> {
    let defaults = create_default_gxserver_config()?;
    validate_local_listener_config(&value, &defaults.listeners.local)?;
    let allowed_origins = value
        .get("cors")
        .and_then(|cors| cors.get("allowedOrigins"))
        .and_then(|origins| origins.as_array())
        .map(|origins| {
            let mut merged = defaults.cors.allowed_origins.clone();
            for origin in origins {
                if let Some(origin) = origin.as_str() {
                    let trimmed = origin.trim();
                    if !trimmed.is_empty() && !merged.iter().any(|existing| existing == trimmed) {
                        merged.push(trimmed.to_string());
                    }
                }
            }
            merged.sort();
            merged
        })
        .unwrap_or(defaults.cors.allowed_origins);

    Ok(GxserverConfig {
        cors: CorsConfig { allowed_origins },
        created_at: value
            .get("createdAt")
            .and_then(|created_at| created_at.as_str())
            .unwrap_or(&defaults.created_at)
            .to_string(),
        listeners: defaults.listeners,
        product: GXSERVER_PRODUCT.to_string(),
        web: WebConfig {
            dist_dir: value
                .get("web")
                .and_then(|web| web.get("distDir"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
        },
    })
}

fn validate_local_listener_config(value: &Value, defaults: &ListenerConfig) -> Result<()> {
    let Some(local) = value
        .get("listeners")
        .and_then(|listeners| listeners.get("local"))
    else {
        return Ok(());
    };
    let Some(local) = local.as_object() else {
        return Ok(());
    };
    if let Some(host) = local.get("host") {
        if host.as_str() != Some(defaults.host.as_str()) {
            bail!(
                "Local gxserver listener host is fixed at {}; remove listeners.local.host from config.json.",
                defaults.host
            );
        }
    }
    if let Some(port) = local.get("port") {
        if json_port_equals(port, defaults.port) != Some(true) {
            bail!(
                "Local gxserver listener port is fixed at {}; remove listeners.local.port from config.json.",
                defaults.port
            );
        }
    }
    Ok(())
}

fn json_port_equals(value: &Value, expected: u16) -> Option<bool> {
    value
        .as_u64()
        .map(|port| port == u64::from(expected))
        .or_else(|| {
            let port = value.as_f64()?;
            Some(port.fract() == 0.0 && port == f64::from(expected))
        })
}

#[cfg(unix)]
fn set_file_mode_0600(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_mode_0600(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;

    #[test]
    fn config_rejects_local_listener_host_and_port_overrides() {
        let default_port = read_selected_local_api_port().expect("port");
        let different_port = if default_port == u16::MAX {
            default_port - 1
        } else {
            default_port + 1
        };
        for (name, config, expected_message) in [
            (
                "host",
                serde_json::json!({ "listeners": { "local": { "host": "0.0.0.0" } } }),
                format!(
                    "Local gxserver listener host is fixed at {}; remove listeners.local.host from config.json.",
                    crate::constants::GXSERVER_LOCAL_API_HOST
                ),
            ),
            (
                "port",
                serde_json::json!({ "listeners": { "local": { "port": different_port } } }),
                format!(
                    "Local gxserver listener port is fixed at {default_port}; remove listeners.local.port from config.json."
                ),
            ),
        ] {
            let temp = tempfile::tempdir().expect(name);
            let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
            fs::create_dir_all(
                paths
                    .config_file
                    .parent()
                    .expect("config file parent directory"),
            )
            .expect("config directory");
            fs::write(&paths.config_file, format!("{config}\n")).expect("config");

            let error = read_gxserver_config(&paths).expect_err("config override should fail");
            assert_eq!(error.to_string(), expected_message);
        }
    }

    #[test]
    fn config_keeps_local_listener_fixed_when_exact_values_are_present() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let default_port = read_selected_local_api_port().expect("port");
        fs::create_dir_all(
            paths
                .config_file
                .parent()
                .expect("config file parent directory"),
        )
        .expect("config directory");
        fs::write(
            &paths.config_file,
            format!(
                "{}\n",
                serde_json::json!({
                    "listeners": {
                        "local": {
                            "enabled": false,
                            "host": crate::constants::GXSERVER_LOCAL_API_HOST,
                            "kind": "remote",
                            "port": default_port
                        }
                    }
                })
            ),
        )
        .expect("config");

        let config = read_gxserver_config(&paths).expect("config");

        assert!(config.listeners.local.enabled);
        assert_eq!(
            config.listeners.local.host,
            crate::constants::GXSERVER_LOCAL_API_HOST
        );
        assert_eq!(config.listeners.local.kind, "local");
        assert_eq!(config.listeners.local.port, default_port);
    }
}

//! What the Remote page reads from the daemon and the pure rules around it: the readers and
//! formatters of packages/core-ui/settings-modal/tabs/remote-easy-connect-model.ts (deleted 2026-10-01), the pairing-code
//! reader of packages/shared/ghostex-remote-pairing.ts (`readPairingCode`), the per-OS SSH steps of
//! ssh-access-instructions.ts (deleted 2026-10-01), and the saved-machine draft of remote-machine-fields.tsx (deleted 2026-10-01).
//!
//! CDXC:RemotePairing 2026-09-03:
//! Everything the daemon returns passes through a reader here so a malformed reply surfaces as one error line on the page instead of a card rendering nothing. Identifiers still say "tailcat" where they name the gxserver endpoint or its payload; user-facing copy says "Easy Connect".
use serde_json::{Value, json};

/// Easy Connect status + pairing code poll (the code rotates after a device pairs).
pub(super) const REMOTE_FAST_REFRESH_MS: u64 = 4000;
/// SSH probe, Tailscale status, and paired devices poll.
pub(super) const REMOTE_SLOW_REFRESH_MS: u64 = 10_000;
/// A paired device counts as "connected now" while its last check-in is this recent.
const PAIRED_DEVICE_CONNECTED_WINDOW_MS: i64 = 3 * 60 * 1000;
pub(super) const EASY_CONNECT_MIN_PORT: u32 = 1;
pub(super) const EASY_CONNECT_MAX_PORT: u32 = 65535;
/// `REMOTE_GXSERVER_INSTALL_PROBE_DEBOUNCE_MS`.
pub(super) const REMOTE_GXSERVER_INSTALL_PROBE_DEBOUNCE_MS: u64 = 600;
/// `GXSERVER_LOCAL_API_PORT`.
pub(super) const GXSERVER_LOCAL_API_PORT: u32 = 58744;

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn non_empty(value: &Value, key: &str) -> Option<String> {
    text(value, key).filter(|text| !text.is_empty())
}

/// `GxserverTailcatStatus`, with the reply kept whole for Advanced's raw status.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TailcatStatus {
    pub(super) installing: bool,
    pub(super) install_progress: Option<String>,
    pub(super) install_error: Option<String>,
    pub(super) enabled: bool,
    pub(super) running: bool,
    pub(super) binary_found: bool,
    pub(super) binary_path: Option<String>,
    pub(super) binary_version: Option<String>,
    pub(super) token: Option<String>,
    pub(super) ports: Vec<u32>,
    pub(super) allowed_client_keys: Vec<String>,
    pub(super) last_error: Option<String>,
    /// The second address's relay (`TailcatRelayStatus` in server/src/tailcat/types.rs).
    pub(super) relay: TailcatRelay,
    pub(super) raw: Value,
}

/// `TailcatRelayStatus`: the relay choice and the second helper's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TailcatRelay {
    /// `ghostex`, `public` or `custom`.
    pub(super) mode: String,
    pub(super) custom_host: String,
    pub(super) host: Option<String>,
    pub(super) running: bool,
    pub(super) token: Option<String>,
    pub(super) reachable: Option<bool>,
    pub(super) last_error: Option<String>,
}

fn read_tailcat_relay(value: Option<&Value>) -> TailcatRelay {
    let Some(relay) = value.filter(|relay| relay.is_object()) else {
        // A gxserver from before the second address has only the public relays.
        return TailcatRelay {
            mode: "public".to_string(),
            ..TailcatRelay::default()
        };
    };
    TailcatRelay {
        mode: non_empty(relay, "mode").unwrap_or_else(|| "ghostex".to_string()),
        custom_host: text(relay, "customHost").unwrap_or_default(),
        host: non_empty(relay, "host"),
        running: relay.get("running").and_then(Value::as_bool) == Some(true),
        token: non_empty(relay, "token"),
        reachable: relay.get("reachable").and_then(Value::as_bool),
        last_error: non_empty(relay, "lastError"),
    }
}

/// `readTailcatStatusResult`.
pub(super) fn read_tailcat_status_result(value: &Value) -> Result<TailcatStatus, String> {
    let status = value.get("status").filter(|status| status.is_object());
    let Some(status) = status.filter(|status| status.get("enabled").is_some_and(Value::is_boolean))
    else {
        return Err("gxserver returned an unreadable Easy Connect status.".to_string());
    };
    Ok(TailcatStatus {
        installing: status.get("installing").and_then(Value::as_bool) == Some(true),
        install_progress: text(status, "installProgress"),
        install_error: text(status, "installError"),
        enabled: status.get("enabled").and_then(Value::as_bool) == Some(true),
        running: status.get("running").and_then(Value::as_bool) == Some(true),
        binary_found: status.get("binaryFound").and_then(Value::as_bool) == Some(true),
        binary_path: text(status, "binaryPath"),
        binary_version: text(status, "binaryVersion"),
        token: text(status, "token"),
        ports: status
            .get("ports")
            .and_then(Value::as_array)
            .map(|ports| {
                ports
                    .iter()
                    .filter_map(Value::as_f64)
                    .map(|port| port as u32)
                    .collect()
            })
            .unwrap_or_default(),
        allowed_client_keys: status
            .get("allowedClientKeys")
            .and_then(Value::as_array)
            .map(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        last_error: text(status, "lastError"),
        relay: read_tailcat_relay(status.get("relay")),
        raw: status.clone(),
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SshStatus {
    pub(super) enabled: bool,
}

impl SshStatus {
    fn read(value: &Value) -> Self {
        Self {
            enabled: value.get("enabled").and_then(Value::as_bool) == Some(true),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TailscaleStatus {
    pub(super) installed: bool,
    pub(super) running: bool,
    pub(super) account: Option<String>,
    pub(super) magic_dns_name: Option<String>,
    pub(super) ip: Option<String>,
}

/// `GxserverRemoteAccessStatus`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AccessStatus {
    pub(super) computer_name: String,
    pub(super) username: String,
    /// `macos`, `windows` or `linux` (`readSshAccessPlatform`).
    pub(super) platform: Option<&'static str>,
    pub(super) ssh: SshStatus,
    pub(super) tailscale: Option<TailscaleStatus>,
}

/// `readSshAccessPlatform`.
pub(super) fn read_ssh_access_platform(value: Option<&str>) -> Option<&'static str> {
    match value {
        Some("macos") => Some("macos"),
        Some("windows") => Some("windows"),
        Some("linux") => Some("linux"),
        _ => None,
    }
}

/// `readRemoteAccessStatusResult`.
pub(super) fn read_remote_access_status_result(value: &Value) -> Result<AccessStatus, String> {
    let readable = value.is_object()
        && value.get("computerName").is_some_and(Value::is_string)
        && value.get("username").is_some_and(Value::is_string)
        && value
            .get("ssh")
            .and_then(|ssh| ssh.get("enabled"))
            .is_some_and(Value::is_boolean)
        && value
            .get("tailscale")
            .is_some_and(|tailscale| !tailscale.is_null());
    if !readable {
        return Err("gxserver returned an unreadable remote access status.".to_string());
    }
    let tailscale = &value["tailscale"];
    Ok(AccessStatus {
        computer_name: text(value, "computerName").unwrap_or_default(),
        username: text(value, "username").unwrap_or_default(),
        platform: read_ssh_access_platform(value.get("platform").and_then(Value::as_str)),
        ssh: SshStatus::read(&value["ssh"]),
        tailscale: Some(TailscaleStatus {
            installed: tailscale.get("installed").and_then(Value::as_bool) == Some(true),
            running: tailscale.get("running").and_then(Value::as_bool) == Some(true),
            account: non_empty(tailscale, "account"),
            magic_dns_name: non_empty(tailscale, "magicDnsName"),
            ip: non_empty(tailscale, "ip"),
        }),
    })
}

/// The Easy Connect half of `/api/remotePairingCode`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct EasyConnectPairing {
    /// What the QR carries: the code in its ghostex.dev link.
    pub(super) payload: String,
    /// What Copy as text gives: the bare `ghostex-ec1:` code, which every phone and computer reads.
    pub(super) text: String,
    pub(super) name: String,
    pub(super) user: String,
    pub(super) port: Option<u32>,
}

/// The Tailscale half of `/api/remotePairingCode`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TailscalePairing {
    pub(super) payload: String,
    pub(super) text: String,
    pub(super) host: Option<String>,
    pub(super) ip: Option<String>,
    pub(super) user: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PairingCodes {
    pub(super) easy_connect: Option<EasyConnectPairing>,
    pub(super) tailscale: Option<TailscalePairing>,
}

/// `readRemotePairingCodeResult`.
pub(super) fn read_remote_pairing_code_result(value: &Value) -> Result<PairingCodes, String> {
    if !value.is_object() {
        return Err("gxserver returned an unreadable pairing code.".to_string());
    }
    let easy_connect = value
        .get("easyConnect")
        .filter(|code| code.get("payload").is_some_and(Value::is_string))
        .map(|entry| {
            let code = &entry["code"];
            EasyConnectPairing {
                payload: text(entry, "payload").unwrap_or_default(),
                text: text(entry, "text").unwrap_or_default(),
                name: text(code, "name").unwrap_or_default(),
                user: text(code, "user").unwrap_or_default(),
                port: code
                    .get("port")
                    .and_then(Value::as_f64)
                    .map(|port| port as u32),
            }
        });
    let tailscale = value
        .get("tailscale")
        .filter(|code| code.get("payload").is_some_and(Value::is_string))
        .map(|entry| {
            let code = &entry["code"];
            TailscalePairing {
                payload: text(entry, "payload").unwrap_or_default(),
                text: text(entry, "text").unwrap_or_default(),
                host: text(code, "host"),
                ip: text(code, "ip"),
                user: text(code, "user"),
            }
        });
    Ok(PairingCodes {
        easy_connect,
        tailscale,
    })
}

/// `GxserverPairedDevice`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PairedDevice {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) platform: String,
    pub(super) paired_at: String,
    pub(super) last_seen_at: Option<String>,
    /// The phone's Easy Connect client key; `None` for a phone paired by an older Ghostex app.
    pub(super) tailcat_client_key: Option<String>,
    /// Paired before 11.0.1 (`status: "legacy"`): it has no access left and must pair again.
    pub(super) legacy: bool,
    /// Paired through its own SSH login (Tailscale, the local network), GitHub #495.
    pub(super) over_ssh: bool,
}

/// `GxserverPendingDevice`: a phone that scanned the QR and waits for the owner's approval
/// (pairing v2, GitHub #495).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingDevice {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) platform: String,
    /// The three words the phone must show too.
    pub(super) words: Vec<String>,
    /// Its `pairingApproveDevice` approval request.
    pub(super) approval_id: String,
    /// It asked over its own SSH login (`/api/pairOverSsh`), not with the Show QR code.
    pub(super) over_ssh: bool,
}

/// The `pending` half of `/api/pairedDevices` (and of `/api/removePairedDevice`'s answer); empty
/// from a gxserver older than pairing v2.
pub(super) fn read_pending_devices(value: &Value) -> Vec<PendingDevice> {
    value
        .get("pending")
        .and_then(Value::as_array)
        .map(|pending| {
            pending
                .iter()
                .filter_map(|device| {
                    let words: Vec<String> = device
                        .get("words")?
                        .as_array()?
                        .iter()
                        .filter_map(|word| word.as_str().map(str::to_string))
                        .collect();
                    Some(PendingDevice {
                        id: text(device, "id")?,
                        name: text(device, "name").unwrap_or_default(),
                        platform: text(device, "platform").unwrap_or_default(),
                        words,
                        approval_id: text(device, "approvalId").unwrap_or_default(),
                        over_ssh: device.get("overSsh").and_then(Value::as_bool) == Some(true),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `readPairedDevicesResult`.
pub(super) fn read_paired_devices_result(value: &Value) -> Result<Vec<PairedDevice>, String> {
    let Some(devices) = value.get("devices").and_then(Value::as_array) else {
        return Err("gxserver returned an unreadable paired device list.".to_string());
    };
    Ok(devices
        .iter()
        .filter(|device| {
            device.get("id").is_some_and(Value::is_string)
                && device.get("name").is_some_and(Value::is_string)
        })
        .map(|device| PairedDevice {
            id: text(device, "id").unwrap_or_default(),
            name: text(device, "name").unwrap_or_default(),
            platform: text(device, "platform").unwrap_or_default(),
            paired_at: text(device, "pairedAt").unwrap_or_default(),
            last_seen_at: non_empty(device, "lastSeenAt"),
            tailcat_client_key: non_empty(device, "tailcatClientKey"),
            legacy: text(device, "status").as_deref() == Some("legacy"),
            over_ssh: device.get("overSsh").and_then(Value::as_bool) == Some(true),
        })
        .collect())
}

/// The outcome of `/api/enableSshAccess` when SSH stayed off (`SshEnableAttempt`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SshEnableAttempt {
    /// `enabled`, `cancelled` or `failed`.
    pub(super) outcome: String,
    pub(super) message: Option<String>,
}

/// `parseEasyConnectPortsInput`.
pub(super) fn parse_easy_connect_ports_input(value: &str) -> Option<Vec<u32>> {
    let mut ports: Vec<u32> = Vec::new();
    for entry in value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        if entry.is_empty() || entry.len() > 5 || !entry.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        let port: u32 = entry.parse().ok()?;
        if !(EASY_CONNECT_MIN_PORT..=EASY_CONNECT_MAX_PORT).contains(&port) {
            return None;
        }
        if !ports.contains(&port) {
            ports.push(port);
        }
    }
    Some(ports)
}

/// `formatEasyConnectPorts`.
pub(super) fn format_easy_connect_ports(ports: &[u32]) -> String {
    ports
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// `parseEasyConnectAllowedClientKeys`.
pub(super) fn parse_easy_connect_allowed_client_keys(value: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for line in value.split('\n') {
        let key = line.trim_end_matches('\r').trim();
        if !key.is_empty() && !keys.iter().any(|existing| existing == key) {
            keys.push(key.to_string());
        }
    }
    keys
}

/// The tone of a status badge (`data-status`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BadgeTone {
    Active,
    Disabled,
    Failed,
    NeedsSetup,
    Unknown,
    /// `data-status='plain'`: no tone colour.
    Plain,
}

/// `getEasyConnectStatusBadge`.
pub(super) fn easy_connect_status_badge(
    status: Option<&TailcatStatus>,
) -> (&'static str, BadgeTone) {
    let Some(status) = status else {
        return ("Unknown", BadgeTone::Unknown);
    };
    if status.installing {
        return ("Installing", BadgeTone::Unknown);
    }
    if !status.binary_found {
        return ("Not installed", BadgeTone::NeedsSetup);
    }
    if !status.enabled {
        return ("Off", BadgeTone::Disabled);
    }
    if status.running {
        return ("Running", BadgeTone::Active);
    }
    if status.last_error.is_some() {
        ("Failed", BadgeTone::Failed)
    } else {
        ("Starting", BadgeTone::Unknown)
    }
}

pub(super) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn parse_iso(iso: &str) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|time| time.with_timezone(&chrono::Local))
}

/// `isPairedDeviceConnectedNow`.
pub(super) fn is_paired_device_connected_now(device: &PairedDevice, now: i64) -> bool {
    device
        .last_seen_at
        .as_deref()
        .and_then(parse_iso)
        .is_some_and(|seen| now - seen.timestamp_millis() < PAIRED_DEVICE_CONNECTED_WINDOW_MS)
}

/// `formatPairedDeviceDay`: "today", "yesterday", or a short calendar date ("Aug 28", "Aug 28,
/// 2025" when not this year).
pub(super) fn format_paired_device_day(iso: &str, now: i64) -> String {
    use chrono::{Datelike as _, TimeZone as _};
    let Some(time) = parse_iso(iso) else {
        return "unknown".to_string();
    };
    let Some(now) = chrono::Local.timestamp_millis_opt(now).single() else {
        return "unknown".to_string();
    };
    let day_delta = (now.date_naive() - time.date_naive()).num_days();
    if day_delta <= 0 {
        return "today".to_string();
    }
    if day_delta == 1 {
        return "yesterday".to_string();
    }
    let month = time.format("%b");
    if time.year() == now.year() {
        format!("{month} {}", time.day())
    } else {
        format!("{month} {}, {}", time.day(), time.year())
    }
}

/// `formatPairedDeviceDetail`.
pub(super) fn format_paired_device_detail(device: &PairedDevice, now: i64) -> String {
    if device.legacy {
        return "Paired before Ghostex 11.1.0, so it no longer connects. Pair it again from the phone, or remove it.".to_string();
    }
    let paired = format!(
        "{} {}",
        if device.over_ssh {
            "Signs in over SSH · paired"
        } else {
            "Paired"
        },
        format_paired_device_day(&device.paired_at, now)
    );
    if is_paired_device_connected_now(device, now) {
        return format!("{paired} · connected now");
    }
    if let Some(seen) = device.last_seen_at.as_deref() {
        return format!(
            "{paired} · last seen {}",
            format_paired_device_day(seen, now)
        );
    }
    format!("{paired} · not connected yet")
}

/// `isPhonePlatform`.
pub(super) fn is_phone_platform(platform: &str) -> bool {
    matches!(
        platform.trim().to_lowercase().as_str(),
        "android" | "ios" | "iphone" | "ipad" | "mobile" | "phone"
    )
}

/// One OS's "turn on SSH access by hand" steps (`SSH_ACCESS_INSTRUCTIONS`).
pub(super) struct SshAccessInstructions {
    pub(super) title: &'static str,
    pub(super) steps: &'static [&'static str],
    pub(super) note: &'static str,
}

/// `SSH_ACCESS_PLATFORMS` with their labels (`SSH_ACCESS_PLATFORM_LABELS`).
pub(super) const SSH_ACCESS_PLATFORMS: [(&str, &str); 3] = [
    ("macos", "macOS"),
    ("windows", "Windows"),
    ("linux", "Linux"),
];

/// CDXC:RemotePairing 2026-09-03:
/// Per-OS "turn on SSH access by hand" steps, the same text as ssh-access-instructions.ts (deleted 2026-10-01) and the mobile app's help sheet. Product copy says "SSH access"; the OS's own feature name appears only inside the path the user has to find on that OS.
pub(super) fn ssh_access_instructions(platform: &str) -> SshAccessInstructions {
    match platform {
        "windows" => SshAccessInstructions {
            title: "Turn on SSH access on Windows",
            steps: &[
                "Open Settings → System → Optional features → Add a feature.",
                "Install OpenSSH Server.",
                "Open Services, start OpenSSH SSH Server, and set its startup type to Automatic.",
            ],
            note: "Or let Ghostex do it: Turn on SSH access above. Windows asks for admin approval once.",
        },
        "linux" => SshAccessInstructions {
            title: "Turn on SSH access on Linux",
            steps: &[
                "Install the OpenSSH server: sudo apt install openssh-server (Debian, Ubuntu) or sudo dnf install openssh-server (Fedora).",
                "Start it and keep it on: sudo systemctl enable --now ssh (or sshd on Fedora).",
            ],
            note: "Or let Ghostex do it: Turn on SSH access above. It asks for your password once.",
        },
        _ => SshAccessInstructions {
            title: "Turn on SSH access on macOS",
            steps: &[
                "Open System Settings → General → Sharing.",
                "Turn on Remote Login.",
                "Under Remote Login, make sure your user is allowed access.",
            ],
            note: "Or let Ghostex do it: Turn on SSH access above. macOS asks for an admin password once.",
        },
    }
}

// ---- pairing codes (packages/shared/ghostex-remote-pairing.ts) ------------------------------

const EASY_CONNECT_CODE_PREFIX: &str = "ghostex-ec1:";
/// `EASY_CONNECT_COMPACT_CODE_PREFIX`, read in either case.
const EASY_CONNECT_COMPACT_CODE_PREFIX: &str = "GHOSTEX-EC2:";
const TAILSCALE_CODE_PREFIX: &str = "ghostex-ts1:";
const LEGACY_EASY_CONNECT_ADDRESS_PREFIX: &str = "tc";
/// `PAIRING_LINK_PREFIX`: gxserver wraps each code in this link so a phone's system camera opens
/// the app (server/src/remote_access/pairing_code.rs); the code field reads it and the bare code.
const PAIRING_LINK_HOST: &str = "ghostex.dev";
const PAIRING_LINK_PATH: &str = "/pair";

/// A decoded Easy Connect code (`EasyConnectCode`), the fields the add form uses.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct EasyConnectCode {
    pub(super) address: String,
    pub(super) name: String,
    pub(super) user: String,
    pub(super) ssh_port: u32,
    /// The other computer's gxserver API port, which pairing reaches through the tunnel.
    pub(super) port: u32,
    /// The one-time pairing secret: only a code from the other computer's Show QR window has one.
    pub(super) secret: Option<String>,
}

/// What pairing this computer with the one behind an Easy Connect code needs (`/api/pairComputer`),
/// or `None` when the code has no one-time secret (CDXC:RemotePairing 2026-11-08 in
/// server/src/remote_access/computer_pairing.rs).
pub(super) fn computer_pairing_params(input: &str) -> Option<Value> {
    match read_pairing_code(input)? {
        PairingCode::EasyConnect(EasyConnectCode {
            address,
            name,
            port,
            secret: Some(secret),
            ..
        }) => Some(serde_json::json!({
            "address": address,
            "port": port,
            "secret": secret,
            "name": name,
        })),
        _ => None,
    }
}

/// `ReadPairingCodeResult`.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum PairingCode {
    EasyConnect(EasyConnectCode),
    Tailscale,
    LegacyAddress(String),
}

/// base64url over UTF-8 JSON, with the JavaScript decoder's rules: no padding, trailing bits
/// ignored, a length of 1 mod 4 refused.
fn decode_base64_url_json(text: &str) -> Option<Value> {
    use base64::Engine as _;
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    if text.is_empty() || text.len() % 4 == 1 {
        return None;
    }
    let engine = GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::RequireNone)
            .with_decode_allow_trailing_bits(true),
    );
    let bytes = engine.decode(text).ok()?;
    let json = String::from_utf8(bytes).ok()?;
    serde_json::from_str(&json).ok()
}

fn read_port(value: Option<&Value>) -> Option<u32> {
    let port = value.and_then(Value::as_f64)?;
    (port.fract() == 0.0 && port > 0.0 && port <= 65535.0).then_some(port as u32)
}

fn read_easy_connect_code(value: &Value) -> Option<EasyConnectCode> {
    if !value.is_object() || value.get("v").and_then(Value::as_f64) != Some(1.0) {
        return None;
    }
    let address = non_empty(value, "address")?;
    let name = non_empty(value, "name")?;
    let user = non_empty(value, "user")?;
    let port = read_port(value.get("port"))?;
    let ssh_port = read_port(value.get("sshPort"))?;
    Some(EasyConnectCode {
        address,
        name,
        user,
        ssh_port,
        port,
        secret: non_empty(value, "secret"),
    })
}

fn strip_compact_prefix(text: &str) -> Option<&str> {
    let prefix = text.get(..EASY_CONNECT_COMPACT_CODE_PREFIX.len())?;
    prefix
        .eq_ignore_ascii_case(EASY_CONNECT_COMPACT_CODE_PREFIX)
        .then(|| &text[EASY_CONNECT_COMPACT_CODE_PREFIX.len()..])
}

/// `readEasyConnectCompactCode`: big-endian u16 lengths for the address bytes, the secret bytes,
/// then the two ports, the user and the name; bytes after the name are ignored.
fn read_easy_connect_compact_code(body: &str) -> Option<EasyConnectCode> {
    use base64::Engine as _;
    let bytes = decode_base32(body)?;
    fn read_u16(rest: &mut &[u8]) -> Option<u16> {
        let (head, tail) = rest.split_first_chunk::<2>()?;
        *rest = tail;
        Some(u16::from_be_bytes(*head))
    }
    fn read_field(rest: &mut &[u8]) -> Option<Vec<u8>> {
        let length = read_u16(rest)? as usize;
        let field = rest.get(..length)?.to_vec();
        *rest = &rest[length..];
        Some(field)
    }
    let mut rest = bytes.as_slice();
    let address = read_field(&mut rest)?;
    let secret = read_field(&mut rest)?;
    let port = read_u16(&mut rest)?;
    let ssh_port = read_u16(&mut rest)?;
    let user = String::from_utf8(read_field(&mut rest)?).ok()?;
    let name = String::from_utf8(read_field(&mut rest)?).ok()?;
    if port == 0 || ssh_port == 0 || user.is_empty() || name.is_empty() || address.is_empty() {
        return None;
    }
    Some(EasyConnectCode {
        address: format!(
            "{LEGACY_EASY_CONNECT_ADDRESS_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(address)
        ),
        name,
        user,
        ssh_port: u32::from(ssh_port),
        port: u32::from(port),
        secret: (!secret.is_empty())
            .then(|| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret)),
    })
}

/// RFC 4648 base32 without padding, either case; leftover bits are dropped.
fn decode_base32(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    if text.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() * 5 / 8);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for char in text.bytes() {
        let index = ALPHABET
            .iter()
            .position(|&letter| letter == char.to_ascii_uppercase())?;
        buffer = ((buffer << 5) | index as u32) & 0xffff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
        }
    }
    Some(bytes)
}

fn is_tailscale_code(value: &Value) -> bool {
    value.is_object()
        && value.get("v").and_then(Value::as_f64) == Some(1.0)
        && non_empty(value, "name").is_some()
        && non_empty(value, "user").is_some()
        && read_port(value.get("port")).is_some()
        && (non_empty(value, "host").is_some() || non_empty(value, "ip").is_some())
}

/// `readPairingLinkFragment(text)`: the fragment of a ghostex.dev pairing link (`Some(None)` when
/// it does not decode), or `None` when `text` is not one. Accepts `http`, `www.`, any host case, a
/// trailing slash and a query, like the TypeScript reader.
fn read_pairing_link_fragment(text: &str) -> Option<Option<String>> {
    let lower = text.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let rest = rest.strip_prefix(PAIRING_LINK_HOST)?;
    let rest = rest.strip_prefix(PAIRING_LINK_PATH)?;
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let hash = rest.find('#')?;
    if !rest[..hash].is_empty() && !rest.starts_with('?') {
        return None;
    }
    // Lowercasing kept every byte offset, so the fragment comes from the original text.
    let fragment = &text[text.len() - (rest.len() - hash - 1)..];
    Some(
        percent_encoding::percent_decode_str(fragment)
            .decode_utf8()
            .ok()
            .map(|decoded| decoded.into_owned()),
    )
}

/// `readPairingCode(payload)`: a pairing link or a bare code.
pub(super) fn read_pairing_code(payload: &str) -> Option<PairingCode> {
    let trimmed = payload.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    if let Some(fragment) = read_pairing_link_fragment(trimmed) {
        // A link carries a whole code, never a bare address.
        let fragment = fragment?;
        if !fragment.starts_with(EASY_CONNECT_CODE_PREFIX)
            && !fragment.starts_with(TAILSCALE_CODE_PREFIX)
            && strip_compact_prefix(&fragment).is_none()
        {
            return None;
        }
        return read_pairing_code(&fragment);
    }
    if let Some(rest) = strip_compact_prefix(trimmed) {
        return read_easy_connect_compact_code(rest).map(PairingCode::EasyConnect);
    }
    if let Some(rest) = trimmed.strip_prefix(EASY_CONNECT_CODE_PREFIX) {
        return read_easy_connect_code(&decode_base64_url_json(rest)?)
            .map(PairingCode::EasyConnect);
    }
    if let Some(rest) = trimmed.strip_prefix(TAILSCALE_CODE_PREFIX) {
        return is_tailscale_code(&decode_base64_url_json(rest)?).then_some(PairingCode::Tailscale);
    }
    if trimmed.len() > LEGACY_EASY_CONNECT_ADDRESS_PREFIX.len()
        && trimmed.starts_with(LEGACY_EASY_CONNECT_ADDRESS_PREFIX)
    {
        return Some(PairingCode::LegacyAddress(trimmed.to_string()));
    }
    None
}

/// `EasyConnectCodeReading`.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum CodeReading {
    Empty,
    Accepted {
        address: String,
        name: Option<String>,
        user: Option<String>,
        ssh_port: Option<u32>,
        summary: String,
    },
    Rejected(String),
}

/// `readEasyConnectCodeInput`.
pub(super) fn read_easy_connect_code_input(input: &str) -> CodeReading {
    if input.trim().is_empty() {
        return CodeReading::Empty;
    }
    match read_pairing_code(input) {
        None => CodeReading::Rejected(
            "That is not an Easy Connect code. Copy it from the other computer as text."
                .to_string(),
        ),
        Some(PairingCode::Tailscale) => CodeReading::Rejected(
            "That is a Tailscale code. Use SSH details with its Tailscale name or IP instead."
                .to_string(),
        ),
        Some(PairingCode::LegacyAddress(address)) => CodeReading::Accepted {
            address,
            name: None,
            user: None,
            ssh_port: None,
            summary: "Looks like a pairing address. Enter the name and user of that computer below. You still need that computer's SSH login.".to_string(),
        },
        Some(PairingCode::EasyConnect(code)) => CodeReading::Accepted {
            summary: format!(
                "Looks like a pairing code for {} on {}. You still need that computer's SSH login.",
                code.user, code.name
            ),
            address: code.address,
            name: Some(code.name),
            user: Some(code.user),
            ssh_port: Some(code.ssh_port),
        },
    }
}

// ---- saved machines (remote-machine-fields.tsx (deleted 2026-10-01)) --------------------------------------------

/// `RemoteMachineDraft`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct MachineDraft {
    pub(super) id: String,
    pub(super) name: String,
    /// The sidebar pill's short name, at most two characters (gx-core `machine_short_names`).
    pub(super) short_name: String,
    /// The icon last read on that computer (gx-core computer_icons.rs), kept through a save; the
    /// dialog's picker changes it on the computer itself, not here.
    pub(super) icon: String,
    /// `ssh` or `easyConnect`.
    pub(super) easy_connect: bool,
    pub(super) easy_connect_code: String,
    pub(super) easy_connect_address: String,
    pub(super) ssh_host: String,
    pub(super) ssh_identity_file: String,
    pub(super) ssh_password: String,
    pub(super) ssh_password_saved: bool,
    /// Ride the SSH connection the user signed in to instead of signing in again (GitHub #496).
    pub(super) reuse_ssh_connection: bool,
    pub(super) ssh_port: String,
    pub(super) ssh_user: String,
    pub(super) wsl_distribution: String,
    pub(super) disabled: bool,
    /// The machine's Agent access switch is off (it is on by default).
    pub(super) agent_access_off: bool,
}

fn base36(mut value: u128) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(std::char::from_digit((value % 36) as u32, 36).unwrap_or('0'));
        value /= 36;
        if value == 0 {
            break;
        }
    }
    digits.iter().rev().collect()
}

/// `createRemoteMachineDraft`: `remote-<Date.now() base 36>-<5 random base-36 digits>`.
pub(super) fn create_remote_machine_draft() -> MachineDraft {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let mut seed = (now.as_nanos() as u64) ^ 0x9e37_79b9_7f4a_7c15;
    let mut random = String::new();
    for _ in 0..5 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        random.push(std::char::from_digit(((seed >> 33) % 36) as u32, 36).unwrap_or('0'));
    }
    MachineDraft {
        id: format!("remote-{}-{random}", base36(now.as_millis())),
        ..MachineDraft::default()
    }
}

/// `createRemoteMachineDraftFromSettings`.
pub(super) fn machine_draft_from_settings(machine: &Value) -> MachineDraft {
    MachineDraft {
        id: text(machine, "id").unwrap_or_default(),
        name: text(machine, "name").unwrap_or_default(),
        short_name: text(machine, "shortName").unwrap_or_default(),
        icon: text(machine, "icon").unwrap_or_default(),
        easy_connect: machine.get("transport").and_then(Value::as_str) == Some("easyConnect"),
        easy_connect_code: String::new(),
        easy_connect_address: text(machine, "easyConnectAddress").unwrap_or_default(),
        ssh_host: text(machine, "sshHost").unwrap_or_default(),
        ssh_identity_file: text(machine, "sshIdentityFile").unwrap_or_default(),
        ssh_password: String::new(),
        ssh_password_saved: machine.get("sshPasswordSaved").and_then(Value::as_bool) == Some(true),
        reuse_ssh_connection: machine.get("reuseSshConnection").and_then(Value::as_bool)
            == Some(true),
        ssh_port: machine
            .get("sshPort")
            .and_then(Value::as_f64)
            .filter(|port| *port != 0.0)
            .map(|port| format!("{}", port as i64))
            .unwrap_or_default(),
        ssh_user: text(machine, "sshUser").unwrap_or_default(),
        wsl_distribution: text(machine, "wslDistribution").unwrap_or_default(),
        disabled: machine.get("disabled").and_then(Value::as_bool) == Some(true),
        agent_access_off: machine.get("agentAccess").and_then(Value::as_bool) == Some(false),
    }
}

/// `easyConnectCodeDraftPatch`: the address plus the name/user/port a pasted code carries.
pub(super) fn apply_easy_connect_code(draft: &mut MachineDraft, input: &str) {
    draft.easy_connect_code = input.to_string();
    match read_easy_connect_code_input(input) {
        CodeReading::Accepted {
            address,
            name,
            user,
            ssh_port,
            ..
        } => {
            draft.easy_connect_address = address;
            if let Some(name) = name
                && draft.name.trim().is_empty()
            {
                draft.name = name;
            }
            if let Some(user) = user {
                draft.ssh_user = user;
            }
            if let Some(port) = ssh_port {
                draft.ssh_port = port.to_string();
            }
        }
        _ => draft.easy_connect_address.clear(),
    }
}

/// The WSL distribution rule of `normalizeRemoteMachineDraft`.
fn wsl_distribution_valid(distribution: &str) -> bool {
    let mut chars = distribution.chars();
    let Some(first) = chars.next() else {
        return true;
    };
    !distribution.starts_with('-')
        && first.is_ascii_alphanumeric()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || "._+() -".contains(ch))
}

/// `normalizeRemoteMachineDraft`: the saved machine, or `None` while the draft cannot be saved.
pub(super) fn normalize_remote_machine_draft(draft: &MachineDraft) -> Option<Value> {
    let wsl_distribution = draft.wsl_distribution.trim();
    if !wsl_distribution_valid(wsl_distribution) {
        return None;
    }
    if draft.easy_connect && draft.easy_connect_address.is_empty() {
        return None;
    }
    let ssh_port = if draft.ssh_port.is_empty() {
        Value::Null
    } else {
        draft
            .ssh_port
            .parse::<u64>()
            .map(Value::from)
            .unwrap_or(Value::Null)
    };
    let candidate = json!([{
        "id": draft.id,
        "name": draft.name,
        "shortName": draft.short_name,
        "icon": draft.icon,
        "transport": if draft.easy_connect { "easyConnect" } else { "ssh" },
        "easyConnectAddress": draft.easy_connect_address,
        "sshHost": if draft.easy_connect { "" } else { draft.ssh_host.as_str() },
        "sshIdentityFile": draft.ssh_identity_file,
        "sshPasswordSaved": draft.ssh_password_saved,
        // An Easy Connect machine is dialed on this computer's own forwarder, and a Windows WSL
        // machine's gxserver listens inside that distribution where a local forward cannot land,
        // so neither has a connection of the user's to reuse.
        "reuseSshConnection": draft.reuse_ssh_connection
            && !draft.easy_connect
            && draft.wsl_distribution.trim().is_empty(),
        "sshPort": ssh_port,
        "sshUser": draft.ssh_user,
        "wslDistribution": wsl_distribution,
        "disabled": draft.disabled,
        "agentAccess": !draft.agent_access_off,
    }]);
    ghostex_gx_core::normalize_remote_machine_settings(Some(&candidate))
        .into_iter()
        .next()
}

/// `normalizeRemoteMachineSettings` of the saved list (the React draft holds it normalized).
pub(super) fn normalize_remote_machines(value: &Value) -> Vec<Value> {
    ghostex_gx_core::normalize_remote_machine_settings(Some(value))
}

/// `formatRemoteMachineSshTarget`: the tile's how-line.
pub(super) fn format_remote_machine_ssh_target(machine: &Value) -> String {
    let user = text(machine, "sshUser").filter(|user| !user.is_empty());
    if machine.get("transport").and_then(Value::as_str) == Some("easyConnect") {
        return match user {
            Some(user) => format!("Easy Connect · {user}"),
            None => "Easy Connect".to_string(),
        };
    }
    let ssh_host = text(machine, "sshHost").unwrap_or_default();
    let host = match user {
        Some(user) => format!("{user}@{ssh_host}"),
        None => ssh_host,
    };
    match machine.get("sshPort").and_then(Value::as_f64) {
        Some(port) if port != 0.0 => format!("{host}:{}", port as i64),
        _ => host,
    }
}

/// The per-target key a gxserver install probe is asked for once (`probeKey`).
pub(super) fn machine_probe_key(machine: &Value) -> Option<String> {
    let ssh_host = text(machine, "sshHost").unwrap_or_default();
    let ssh_host = ssh_host.trim();
    if ssh_host.is_empty() {
        return None;
    }
    let port = machine
        .get("sshPort")
        .and_then(Value::as_f64)
        .map(|port| format!("{}", port as i64))
        .unwrap_or_default();
    Some(
        [
            text(machine, "id").unwrap_or_default(),
            ssh_host.to_string(),
            text(machine, "sshUser")
                .unwrap_or_default()
                .trim()
                .to_string(),
            port,
            text(machine, "wslDistribution")
                .unwrap_or_default()
                .trim()
                .to_string(),
        ]
        .join("|"),
    )
}

#[cfg(test)]
mod pairing_code_tests {
    use super::*;

    const EASY_CONNECT_JSON: &str = r#"{"v":1,"address":"tc1qexampleaddress","name":"Studio","user":"madda","port":58744,"sshPort":22,"secret":"s3cr3t","expiresAt":"2026-10-24T07:15:00.000Z"}"#;

    fn easy_connect_code() -> String {
        use base64::Engine as _;
        format!(
            "ghostex-ec1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(EASY_CONNECT_JSON)
        )
    }

    fn studio() -> PairingCode {
        PairingCode::EasyConnect(EasyConnectCode {
            address: "tc1qexampleaddress".to_string(),
            name: "Studio".to_string(),
            user: "madda".to_string(),
            ssh_port: 22,
            port: 58744,
            secret: Some("s3cr3t".to_string()),
        })
    }

    #[test]
    fn reads_the_bare_code_older_computers_show() {
        assert_eq!(read_pairing_code(&easy_connect_code()), Some(studio()));
    }

    #[test]
    fn reads_the_pairing_link_newer_computers_show() {
        let code = easy_connect_code();
        for link in [
            format!("https://ghostex.dev/pair#{code}"),
            format!("  https://ghostex.dev/pair#{code}\n"),
            format!("HTTPS://Ghostex.Dev/pair/#{code}"),
            format!("https://www.ghostex.dev/pair?from=camera#{code}"),
            format!("https://ghostex.dev/pair#{}", code.replace(':', "%3A")),
        ] {
            assert_eq!(read_pairing_code(&link), Some(studio()), "{link}");
        }
    }

    /// gxserver's `COMPACT_VECTOR` (server/src/remote_access/pairing_code.rs).
    const COMPACT_VECTOR: &str = "GHOSTEX-EC2:AAUAAAICAMCAKBQHBAEQUCYMBUHA6EARCIJRIFIWC4MBSGQ3DQOR4HZAEERCGJBFEYTQAEDEMVTGO2DJNJVWY3LON5YHC4TT4V4AAFQAAVWWCZDEMEAA4U3UOVSGS37CQCMXGICNMFRQ";

    #[test]
    fn reads_the_compact_code_bare_linked_and_in_lower_case() {
        let expected = Some(PairingCode::EasyConnect(EasyConnectCode {
            address: "tcAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJw".to_string(),
            name: "Studio\u{2019}s Mac".to_string(),
            user: "madda".to_string(),
            ssh_port: 22,
            port: 58744,
            // The vector's secret bytes, as gxserver wrote them (base64url).
            secret: Some("ZGVmZ2hpamtsbW5vcHFycw".to_string()),
        }));
        for payload in [
            COMPACT_VECTOR.to_string(),
            format!("https://ghostex.dev/pair#{COMPACT_VECTOR}"),
            COMPACT_VECTOR.to_ascii_lowercase(),
        ] {
            assert_eq!(read_pairing_code(&payload), expected, "{payload}");
        }
        for broken in [
            "GHOSTEX-EC2:",
            "GHOSTEX-EC2:AAUAAAICAMCA",
            "GHOSTEX-EC2:AAUA1AAICAMCA",
        ] {
            assert_eq!(read_pairing_code(broken), None, "{broken}");
        }
    }

    #[test]
    fn copy_as_text_is_the_bare_code_beside_the_link() {
        let code = easy_connect_code();
        let codes = read_remote_pairing_code_result(&serde_json::json!({
            "easyConnect": {
                "payload": format!("https://ghostex.dev/pair#{code}"),
                "text": code,
                "code": { "name": "Studio", "user": "madda", "port": 58744 },
            },
        }))
        .unwrap();
        let easy_connect = codes.easy_connect.unwrap();
        assert_eq!(easy_connect.text, code);
        assert_eq!(read_pairing_code(&easy_connect.text), Some(studio()));
    }

    #[test]
    fn refuses_links_that_are_not_a_pairing_link() {
        let code = easy_connect_code();
        for link in [
            format!("https://ghostex.dev/pairs#{code}"),
            format!("https://ghostex.dev/pair/x#{code}"),
            format!("https://evil.example/pair#{code}"),
            format!("https://ghostex.dev.evil.example/pair#{code}"),
            "https://ghostex.dev/pair".to_string(),
            "https://ghostex.dev/pair#tc1qexampleaddress".to_string(),
            "https://ghostex.dev/pair#ghostex-ec1:not-base64!".to_string(),
        ] {
            assert_eq!(read_pairing_code(&link), None, "{link}");
        }
        assert_eq!(
            read_pairing_code("tc1qexampleaddress"),
            Some(PairingCode::LegacyAddress("tc1qexampleaddress".to_string()))
        );
    }
}

/// What a machine tile says instead of its SSH target or version.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TileStatus {
    pub(super) text: &'static str,
    /// Drawn with a red dot.
    pub(super) failed: bool,
}

/// What a machine tile says instead of its SSH target or version: that it is this computer, or
/// that its connection is failing. `None` keeps the usual line.
///
/// CDXC:RemoteMachines 2026-10-28 WHY: a failing machine's tile said nothing (the tab only counted
/// "0/1 connected"), so the reason was only visible in the sidebar (found testing on Linux; every
/// platform). A machine that is this computer says so.
///
/// CDXC:RemoteMachines 2026-11-08 DECISION: User (Q2024): the tile shows only the state's short
/// words with a red dot (`machine_state_short_word`); the reason it used to wrap over six lines
/// is the first thing in the machine's sheet (`machine_status_panel` in machines.rs).
pub(super) fn machine_tile_status(state: Option<&str>, this_computer: bool) -> Option<TileStatus> {
    if this_computer && state == Some("connected") {
        return Some(TileStatus {
            text: "This computer",
            failed: false,
        });
    }
    let state = state.filter(|state| ghostex_gx_core::machine_state_is_failure(state))?;
    Some(TileStatus {
        text: ghostex_gx_core::machine_state_short_word(state),
        failed: true,
    })
}

#[cfg(test)]
mod machine_tile_status_tests {
    use super::machine_tile_status;

    #[test]
    fn a_failing_machine_says_why_and_this_computer_says_so() {
        let text = |state, this_computer| {
            machine_tile_status(Some(state), this_computer).map(|status| status.text)
        };
        assert_eq!(text("sshFailed", false), Some("Can't reach"));
        assert_eq!(text("authFailed", false), Some("Sign-in refused"));
        assert_eq!(text("installFailed", false), Some("Setup failed"));
        assert_eq!(text("connected", true), Some("This computer"));
        assert_eq!(text("connected", false), None);
        assert_eq!(text("connecting", false), None);
        assert_eq!(machine_tile_status(None, false), None);
    }
}

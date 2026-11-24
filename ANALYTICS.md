# Ghostex Analytics and Privacy

Ghostex collects minimal, anonymous usage telemetry so we know which platforms, views, and coding agents people rely on and where reliability issues occur. Every rule in this document is enforced in code by a single closed allowlist in [`server/src/telemetry/taxonomy.rs`](server/src/telemetry/taxonomy.rs).

## Core Privacy Guarantees

- **Single egress chokepoint:** Only `gxserver` on your local computer sends analytics (to PostHog at `https://us.i.posthog.com/batch/`). The desktop app, web UI, and iOS/Android apps never contact PostHog directly.
- **Random per-install UUIDv4 only:** Events use a random UUIDv4 generated once per install and stored locally in `analytics-install-id`. Ghostex never derives an identifier from your name, email, machine hostname, hardware serial, or agent account files (such as `~/.codex/auth.json` or `~/.claude.json`).
- **Country-only GeoIP with raw IPs discarded:** PostHog performs country-only GeoIP lookup on ingest and discards raw client IP addresses. Ghostex does not send coordinates, city, or custom location fields.
- **First-run onboarding hold:** On a brand-new install, Ghostex holds all outbound analytics flushes so zero bytes leave your computer until you finish or skip first-run onboarding. If you turn analytics off during onboarding, any buffered events are dropped immediately and never sent.
- **Activity-gated daily snapshots:** Daily summary events (`heartbeat`, `session_mode.daily_rollup`, and `settings.snapshot`) are only emitted when a Ghostex client (desktop, web, or mobile) or an agent session was actively used since the previous heartbeat. An idle background daemon never phones home.
- **Remote SSH helpers stay silent:** A `gxserver` installed on a remote machine over SSH (`analytics-role: remote`) never sends analytics.
- **Closed compile-time schema:** Every event name and property key/value is validated by [`server/src/telemetry/taxonomy.rs`](server/src/telemetry/taxonomy.rs) before queueing. Free-form user strings cannot pass validation.

## What Is Never Sent

- Prompts, chat replies, terminal output, code, or diffs
- File paths, repository URLs, project names, session titles, or workspace names
- Email addresses, usernames, credentials, tokens, or third-party account IDs
- **Custom CLI/tool names:** Custom or unknown agents report strictly `"custom"` with zero binary, command, or wrapper script name
- **Custom extension names:** Local or unlisted extensions are never named (only `"custom"` or a bucketed count is sent)
- **Custom Boost names:** User-created Boost names and prompts are never sent (only a bucketed count is sent)
- Hostnames, saved remote machine addresses, theme hex colors, or custom wallpaper/video paths
- Raw IP addresses (discarded by PostHog after country-only GeoIP lookup)

## How to Disable or Inspect Telemetry

### Turn telemetry off

You can disable analytics at any time using any of the following methods:

1. **Settings:** Set `"analyticsEnabled": false` in your Ghostex settings (`native-sidebar-settings.json`, or via the Analytics switch in Settings > About once enabled in the desktop UI).
2. **Environment variables:** Start Ghostex or `gxserver` with `DO_NOT_TRACK=1` or `GHOSTEX_TELEMETRY_DISABLED=1`.
3. **Launch flag:** Start the desktop app or `gxserver` with `--no-analytics`. If a daemon is already running without the flag, the desktop app sends a local `analytics.off` signal that keeps analytics off until the daemon exits.

**Immediate opt-out wipe:** As soon as analytics are turned off, `gxserver` immediately drops its in-memory event queue, deletes the entire `analytics.state.v1` state row from SQLite (clearing `lastHeartbeatAt`, `lastActivityAt`, `cachedAccounts`, `builtInExtensionsOn`, `builtInExtensionsKnown`, `lastMobileHelloAt`, and `sessionModeSwitches`), and removes `analytics-install-id` from disk.

### Inspect exact outgoing payloads

- **Debug payload logging:** Run `gxserver` with `GHOSTEX_TELEMETRY_DEBUG=1` to print the full formatted JSON batch payload to stderr right before each batch is sent.
- **Local read-only preview API:** `POST /api/telemetryPreview` (local authenticated owner-device endpoint) returns the current gate status (`enabled`, `reason`, `onboardingPending`), `distinctId`, queued count, and live sanitized `heartbeat` and `settingsSnapshot` JSON payloads without writing state or sending anything.

## Complete Event and Property Reference

Source of truth: [`server/src/telemetry/taxonomy.rs`](server/src/telemetry/taxonomy.rs), [`server/src/telemetry/base.rs`](server/src/telemetry/base.rs), [`server/src/telemetry/profile.rs`](server/src/telemetry/profile.rs), [`server/src/telemetry/heartbeat.rs`](server/src/telemetry/heartbeat.rs), and [`server/src/telemetry/settings_snapshot.rs`](server/src/telemetry/settings_snapshot.rs).

### Properties attached to every event

**Base properties (`base.rs`):**

| Property | Type | Description |
| --- | --- | --- |
| `server_version` | version string | Ghostex release version |
| `platform` | enum | Operating system (`macos`, `linux`, `windows`, etc.) |
| `arch` | enum | CPU architecture (`aarch64`, `x86_64`, etc.) |
| `os_version` | string | Major OS version only (for example `15`, not minor patch builds) |
| `is_dev` | boolean | `true` for unstamped or debug builds |

**Profile properties (`profile.rs`):**

| Property | Type | Description |
| --- | --- | --- |
| `interface` | enum | Preferred interface (`chat` or `terminal`), omitted if settings do not exist yet |
| `default_agent` | enum | Default agent catalog ID (or `custom`) |
| `project_bucket` | enum | Coarse project count bucket (`0`, `1-2`, `3-5`, `6-10`, `10+`) |

### Events

| Event | When emitted | Properties |
| --- | --- | --- |
| `heartbeat` | At most once per 20-24h, only when actively used since the previous heartbeat | `project_count`, `session_count`, `running_session_count`, `agents_used` (catalog IDs or `custom`), `default_agent`, `preferred_interface`, `extension_count`, `built_in_extensions_on`, `store_extensions_enabled`, `custom_extensions_enabled`, `remote_machine_count`, `days_since_install`, plus `$set` person properties (`os`, `arch`, `server_version`, `interface`, `default_agent`, `project_count`, `session_count`, `extension_count`, `remote_machine_count`, `days_since_install`, `project_bucket`) |
| `settings.snapshot` | Once per active daily heartbeat (and at most one follow-up if account plans finish loading shortly after startup) | Curated allowlisted UI/agent settings (catalog select choices, booleans, bucketed numbers for blur/opacity/zoom/sidebar width, presence-only booleans for wallpaper/video/theme color roles), Linux environment (`linux_distro`, numeric `linux_distro_version` for known distros only, `linux_desktop`, `linux_session`, `linux_wsl`), `remote_machine_count`, bucketed counts (`workspace_count`, `custom_boost_count`, `edited_boost_count`, `custom_extensions_installed`, `claude_accounts`, `codex_accounts`, `cursor_accounts`, `gemini_accounts`, `copilot_accounts`), `boosts_on`, `claude_plans`, `codex_plans`, `claude_top_plan`, `codex_top_plan`, `store_extensions_installed` (`id@version`), `antigravity_mode`, `claude_mode`, `codex_mode`, plus `$set` for top-level breakdown keys (`appearance`, `transparency`, `sidebar_view`, `sidebar_layout`, `linux_distro`, `linux_desktop`, `claude_top_plan`, `codex_top_plan`, `antigravity_mode`, `claude_mode`, `codex_mode`) |
| `app.launched` | Desktop app startup | `client` (`desktop`), `app_version`, optional `linux_desktop`, optional `linux_session` |
| `client.connected` | Mobile or web client connects (throttled to 1/hr per client and OS) | `client` (`mobile` or `web`), `client_os`, optional `client_os_version`, optional `client_app_version` |
| `session.started` | New agent session created (plain terminal sessions excluded) | `agent` (known catalog agent ID, or `custom`) |
| `prompt.sent` | Prompt dispatched to an agent | `agent` (known catalog agent ID, or `custom`), `source` (`automation`, `board`, `chat`, `cli`, `queue`, `quick_action`, `terminal`) |
| `surface.opened` | View, modal, or mobile screen opened (throttled to 1/hr per client, surface, and extension) | `surface` (closed view/screen enum), optional `extension` (built-in ID, official Store ID, or `custom`), optional `client` (`desktop` or `mobile`) |
| `extension.installed` | Extension installed | `source` (`local` or `store`), optional `extension` (official Store ID or `custom`), optional `extension_version` (official Store only) |
| `extension.uninstalled` | Extension uninstalled | optional `extension` (official Store ID or `custom`) |
| `extension.enabled` | Built-in or installed extension switched on | `extension` (built-in ID, official Store ID, or `custom`), `kind` (`built_in` or `installed`) |
| `extension.disabled` | Built-in or installed extension switched off | `extension` (built-in ID, official Store ID, or `custom`), `kind` (`built_in` or `installed`) |
| `boosts.used` | Prompt sent with one or more Boosts active | `agent` (known catalog agent ID, or `custom`), `source`, `boosts` (built-in Ghostex Boost IDs only), `custom_boost_count` (bucketed count) |
| `sidebar_layout.changed` | Sidebar layout switched between Rail and Classic | `layout` (`rail` or `classic`), `source` (`settings` or `intro`) |
| `rail_intro.shown` | One-time "Meet the rail" popup shown | `layout` (`rail` or `classic`) |
| `rail_intro.answered` | "Meet the rail" popup closed | `chose` (`rail` or `classic`) |
| `onboarding.step` | First-run onboarding funnel milestone | `step` (`started`, `cli_detected`, `project_added`, `project_opened`, `first_session`, `first_prompt`, `completed`, `skipped`), optional `platform` |
| `reliability.error` | Anonymous reliability failure category (throttled to 1 per 5 min per category and agent) | `category` (`cli_not_found`, `daemon_bootstrap_failed`, `loopback_telemetry_failed`, `mobile_connect_failed`, `native_chat_init_failed`, `prompt_delivery_failed`, `provider_auth_failed`, `session_launch_failed`, `update_failed`, `wmx_start_failed`, `zmx_attach_failed`), optional `agent_id` (known catalog ID or `custom`), optional `platform` |
| `session_mode.state` | App start or session mode setting change | `trigger` (`app_start` or `changed`), `client` (`desktop`, `mobile`, `server`), `preferred_view`, `antigravity_mode`, `claude_mode`, `codex_mode`, `native_agent_count`, plus `$set` for `interface` and the three agent modes |
| `session_mode.switched` | Session switched between Native and Terminal mode | `agent` (`antigravity`, `claude`, `codex`), `direction` (`native_to_terminal`, `terminal_to_native`), `source` (`bulk_modal`, `more_menu`, `pill`, `settings`, `unknown`), `client` (`desktop`, `mobile`, `server`) |
| `session_mode.daily_rollup` | Daily alongside active `heartbeat` | `preferred_view`, `antigravity_mode`, `claude_mode`, `codex_mode`, live session counts by agent family and mode, and daily switch counts by direction and entry point |

## Auditing the Code

All telemetry code lives in [`server/src/telemetry/`](server/src/telemetry/):

- Closed event and property allowlist: [`server/src/telemetry/taxonomy.rs`](server/src/telemetry/taxonomy.rs)
- Opt-out gate and first-run onboarding hold: [`server/src/telemetry/gate.rs`](server/src/telemetry/gate.rs)
- Random UUIDv4 install ID: [`server/src/telemetry/identity.rs`](server/src/telemetry/identity.rs)
- Activity-gated daily tasks and opt-out state wipe: [`server/src/telemetry/task.rs`](server/src/telemetry/task.rs)
- Read-only local preview endpoint: [`server/src/server/telemetry_http.rs`](server/src/server/telemetry_http.rs)

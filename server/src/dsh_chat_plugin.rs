//! Ghostex's DeepSeek Harness plugin: the chat channel half that runs inside `dsh-tui`.
//!
//! CDXC:DshChatChannel 2026-11-05 DECISION:
//! User (via coordinator, Q1347): DeepSeek Harness's chat gets "the full plugin on the shared chat channel, falling back to the inject socket, then the screen", so the chat no longer has to read dsh-tui's screen to know whether it can send. dsh-tui runs one process and loads Cordis plugin rows into it; the plugin below finds dsh-tui's own channel (the object its Enter, Ctrl+C, `/model` and `/effort` call) through dsh-tui's channel registry, and long-polls gxserver's `/api/agentChatChannel` exactly as the Pi extension does (pi_chat_channel.rs), so a message sent from chat shows in the TUI as typed, the TUI's header follows a model pick, and the TUI stays a normal TUI on the same conversation.
//! WHY: the plugin is a file Ghostex owns (`<app data>/hooks/dsh-tui/ghostex-chat.mjs`), mounted by one marked row in the profile's own patch layer (`<DSH_HOME>/profiles/dsh-tui/cordis.patch.yml`). dsh's loader takes an absolute module path as a row name (dsh `packages/boot/app-boot/src/index.ts`, `HostResolvedRootInclude`), so nothing is added to the profile's package.json or node_modules, `dsh-tui update` (which only runs `dsh plugin update` on its own package) leaves it alone, and the user's own rows stay as they are. The plugin needs no grant: dsh-tui's grants gate its interception events and plugin storage, and this uses neither.
//! WHY: the channel registry and the channel's methods are dsh-tui internals (0.17.0 and 0.18.0 ship the same `lib/types/adapter/channel/host-registry.js` and the same `submit`, `cancel`, `switchModel`, `setEffort`), so every one is feature-checked and a miss answers "unsupported", which sends that action back to the screen path. Outside a Ghostex terminal (no GHOSTEX_* variables) the plugin does nothing.
//! SEE-ALSO: server/src/dsh_chat_channel.rs (gxserver's side), server/src/pi_chat_channel.rs (the shared channel).

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Bumped whenever the plugin's commands or answers change; gxserver reads it from each poll.
pub(crate) const DSH_CHAT_PLUGIN_VERSION: u32 = 1;

const ROW_BEGIN: &str = "# ghostex chat channel begin (managed by Ghostex; to opt out, replace this block with the line: # ghostex chat channel off)";
const ROW_END: &str = "# ghostex chat channel end";
const OPT_OUT: &str = "# ghostex chat channel off";

const PLUGIN_SOURCE: &str = r###"// Ghostex chat channel for DeepSeek Harness (dsh-tui). Written by Ghostex; edits are overwritten.
// The session's chat in Ghostex sends messages, stops turns and picks the model and effort through
// dsh-tui's own channel instead of typing into the terminal and reading it back. It polls gxserver
// with the address, token and session ref Ghostex puts in the terminal's environment, and does
// nothing outside a Ghostex terminal.
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { pathToFileURL } from 'node:url'

// The token goes only to a listener of this account: the ghostex CLI runs the owner check this
// process cannot (ghostex_paths::loopback_owner). Exit 3 is a refusal; a missing or older CLI
// answers something else and the request goes ahead as before.
async function ghostexOwnerCheck(baseUrl) {
  const { execFile } = await import("node:child_process");
  const outcome = await new Promise((resolve) => {
    execFile(process.env.GHOSTEX_CLI_EXECUTABLE || "ghostex", ["loopback-owner-check", baseUrl], { timeout: 5000, windowsHide: true }, (error, _stdout, stderr) => resolve({ code: error ? error.code : 0, stderr: String(stderr || "") }));
  });
  if (outcome.code === 3) throw new Error(outcome.stderr.trim() || "Another account on this computer is listening where Ghostex's service should be.");
}

export const name = 'ghostex-chat'

const VERSION = __VERSION__
const CAPABILITIES = ['state', 'send', 'abort', 'model']

class Unsupported extends Error {}

const text = (value) => (typeof value === 'string' && value.trim() !== '' ? value.trim() : undefined)
const record = (value) => (value && typeof value === 'object' ? value : {})
const sleep = (ms) => new Promise((resolve) => { const timer = setTimeout(resolve, ms); timer.unref?.() })

function chatEnv() {
  if (process.env.GHOSTEX_CHAT_CHANNEL_DISABLED === '1') return null
  const baseUrl = text(process.env.GHOSTEX_GXSERVER_BASE_URL)
  const tokenFile = text(process.env.GHOSTEX_GXSERVER_AUTH_TOKEN_FILE)
  const sessionRef = text(process.env.GHOSTEX_GLOBAL_SESSION_REF)
  if (!baseUrl || !tokenFile || !sessionRef) return null
  return { baseUrl: baseUrl.replace(/\/+$/, ''), tokenFile, sessionRef, protocol: text(process.env.GHOSTEX_GXSERVER_PROTOCOL_VERSION) ?? '1' }
}

// dsh-tui's package as this profile loaded it: from the row's base URL, then the launched entry.
function dshTuiDir(ctx) {
  const anchors = [ctx?.baseUrl, process.argv[1] && pathToFileURL(process.argv[1]).href]
  const home = text(process.env.DSH_HOME) ?? join(process.env.HOME ?? '', '.dsh')
  anchors.push(pathToFileURL(join(home, 'profiles', 'dsh-tui', 'package.json')).href)
  for (const anchor of anchors) {
    if (!anchor) continue
    try {
      return dirname(createRequire(anchor).resolve('@deepseek-harness-tui/dsh-tui/package.json'))
    } catch {}
  }
  return undefined
}

async function importFrom(dir, relative) {
  try {
    return await import(pathToFileURL(join(dir, relative)).href)
  } catch {
    return undefined
  }
}

export function apply(ctx) {
  const env = chatEnv()
  if (!env) return
  const instance = `${process.pid}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
  let running = true
  let poll = null
  let registry
  let llm

  const channel = () => {
    try {
      const registration = registry?.getTuiChannelRegistration?.(ctx)
      return registration?.channel ?? registry?.getRegisteredTuiChannel?.(ctx)
    } catch {
      return undefined
    }
  }
  const agents = () => {
    try { return ctx.get?.('agents', false) ?? ctx.agents } catch { return undefined }
  }
  const foregroundAgent = (sessionId) => {
    const id = sessionId ?? text(channel()?.agentId)
    try { return id ? agents()?.get?.(id) : undefined } catch { return undefined }
  }

  const state = () => {
    const ch = channel()
    if (!ch) return { agentSessionId: undefined, status: 'starting', dialogs: [] }
    return {
      agentSessionId: text(ch.agentId),
      status: ch.status,
      working: ch.working === true,
      model: { provider: text(ch.provider), id: text(ch.model), name: text(ch.model) },
      thinkingLevel: text(ch.reasoningEffort),
      efforts: Array.isArray(ch.effortLevels) ? ch.effortLevels : undefined,
      dialogs: [],
    }
  }

  const run = async (kind, params) => {
    const ch = channel()
    if (kind === 'state') return state()
    if (kind === 'send') {
      const message = typeof params.text === 'string' ? params.text : ''
      if (message.trim() === '') throw new Error('The message is empty.')
      // Slash commands run in dsh-tui's editor, not its channel.
      if (message.trimStart().startsWith('/')) throw new Unsupported('a TUI command')
      if (Array.isArray(params.images) && params.images.length > 0) throw new Unsupported('images')
      if (ch && typeof ch.submit === 'function') {
        if (params.interrupt === true && ch.working && typeof ch.interruptAndDeliver === 'function') ch.interruptAndDeliver([message])
        else ch.submit(message)
        return { delivered: 'message', via: 'channel', agentSessionId: text(ch.agentId) }
      }
      const agent = foregroundAgent()
      if (agent && typeof agent.followup === 'function' && llm?.createUserMessage) {
        agent.followup(llm.createUserMessage({ content: [{ type: 'text', text: message }], source: { kind: 'user' } }))
        return { delivered: 'message', via: 'agent', agentSessionId: text(agent.id) }
      }
      throw new Unsupported('no dsh-tui channel')
    }
    if (kind === 'abort') {
      if (ch && typeof ch.cancel === 'function') {
        const working = ch.working === true
        if (working) ch.cancel()
        return { stopped: working, via: 'channel' }
      }
      const agent = foregroundAgent()
      if (agent && typeof agent.cancel === 'function') {
        const working = agent.status === 'running'
        if (working) agent.cancel({ kind: 'user' }, { keepInbox: true })
        return { stopped: working, via: 'agent' }
      }
      throw new Unsupported('no dsh-tui channel')
    }
    if (kind === 'model') {
      if (!ch || typeof ch.switchModel !== 'function' || typeof ch.setEffort !== 'function') throw new Unsupported('no dsh-tui channel')
      const provider = text(params.provider)
      const model = text(params.model)
      const effort = text(params.effort)
      if (provider && model && (provider !== text(ch.provider) || model !== text(ch.model))) {
        if (!(await ch.switchModel(provider, model))) throw new Error(`dsh-tui did not switch to ${provider}/${model}`)
      }
      if (effort && effort !== text(ch.reasoningEffort)) {
        if (!(await ch.setEffort(effort))) throw new Error(`dsh-tui does not offer the ${effort} effort here`)
      }
      return state()
    }
    throw new Unsupported(`unknown command ${kind}`)
  }

  const post = async (params, timeoutMs, signal) => {
    let token = ''
    try {
      const file = env.tokenFile.startsWith('~/') ? join(process.env.HOME ?? '', env.tokenFile.slice(2)) : env.tokenFile
      token = readFileSync(file, 'utf8').trim()
    } catch {}
    if (!token) throw new Error('no gxserver token')
    await ghostexOwnerCheck(env.baseUrl)
    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort(), timeoutMs)
    const abort = () => controller.abort()
    signal?.addEventListener('abort', abort)
    try {
      const response = await fetch(`${env.baseUrl}/api/agentChatChannel`, {
        method: 'POST',
        headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-gxserver-protocol-version': env.protocol },
        body: JSON.stringify({ protocolVersion: Number(env.protocol) || 1, params: { sessionRef: env.sessionRef, ...params } }),
        signal: controller.signal,
      })
      const body = record(await response.json().catch(() => ({})))
      if (!response.ok || body.ok === false) throw new Error(text(body.message) ?? `gxserver answered ${response.status}`)
      return record(body.result)
    } finally {
      clearTimeout(timer)
      signal?.removeEventListener('abort', abort)
    }
  }

  const answer = async (command) => {
    const id = text(command.id)
    if (!id) return
    let ack
    try {
      ack = { action: 'ack', id, ok: true, result: await run(text(command.kind) ?? '', record(command.params)) }
    } catch (error) {
      ack = { action: 'ack', id, ok: false, unsupported: error instanceof Unsupported, error: error instanceof Error ? error.message : String(error) }
    }
    for (let attempt = 0; attempt < 3; attempt += 1) {
      try { await post(ack, 5000); return } catch { await sleep(300) }
    }
  }

  const loop = async () => {
    const dir = dshTuiDir(ctx)
    if (dir) {
      registry = await importFrom(dir, 'lib/types/adapter/channel/host-registry.js')
      try { llm = await import(pathToFileURL(createRequire(join(dir, 'package.json')).resolve('@deepseek-ai/dsh-llm')).href) } catch {}
    }
    // dsh-tui registers its channel after the first frame; the chat waits for it.
    while (running && !channel()) await sleep(500)
    let failures = 0
    while (running) {
      poll = new AbortController()
      try {
        const result = await post({
          action: 'poll', agent: 'dsh', pid: process.pid, instance, version: VERSION,
          capabilities: CAPABILITIES, agentSessionId: text(channel()?.agentId),
        }, 60000, poll.signal)
        failures = 0
        for (const command of Array.isArray(result.commands) ? result.commands : []) await answer(record(command))
      } catch {
        if (!running) break
        failures += 1
        // gxserver restarting or not up yet: keep trying, never more than a few seconds apart.
        await sleep(Math.min(5000, 250 * 2 ** Math.min(failures, 5)))
      }
    }
  }

  void loop().catch(() => {})
  const stop = () => {
    if (!running) return
    running = false
    poll?.abort()
    void post({ action: 'close', instance }, 2000).catch(() => {})
  }
  try {
    if (typeof ctx.effect === 'function') ctx.effect(() => stop, 'ghostex chat channel')
    else ctx.on?.('dispose', stop)
  } catch {}
}
"###;

fn plugin_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir
        .join("hooks")
        .join("dsh-tui")
        .join("ghostex-chat.mjs")
}

fn plugin_source() -> String {
    PLUGIN_SOURCE.replace("__VERSION__", &DSH_CHAT_PLUGIN_VERSION.to_string())
}

/// YAML single-quoted scalar.
fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// The row disables itself outside a Ghostex terminal and while the plugin file is missing: a row
/// whose module cannot be imported disposes dsh-tui's whole app at boot (dsh-TUI #60), and dsh's
/// loader evaluates a `!!js` `disabled` before importing anything (cordis-plugin-loader 1.0.6).
fn marked_rows(plugin: &Path) -> String {
    let path = plugin.to_string_lossy();
    let js_path = serde_json::Value::String(path.to_string()).to_string();
    format!(
        "{ROW_BEGIN}\n- insert:\n    - id: ghostex-chat\n      name: {}\n      disabled: !!js >-\n        !process.env.GHOSTEX_GLOBAL_SESSION_REF || !process.getBuiltinModule('node:fs').existsSync({js_path})\n{ROW_END}\n",
        quoted(&path)
    )
}

/// The patch layer with Ghostex's row in it, or `None` when it already has it or Ghostex must
/// leave it alone: the user opted out, or the file is not a block list Ghostex can append to.
fn patched_layer(current: &str, rows: &str) -> Option<String> {
    if current.contains(OPT_OUT) {
        return None;
    }
    if let (Some(begin), Some(end)) = (current.find(ROW_BEGIN), current.find(ROW_END)) {
        if end < begin {
            return None;
        }
        let end = end + ROW_END.len();
        let end = current[end..]
            .find('\n')
            .map_or(current.len(), |offset| end + offset + 1);
        let replaced = format!("{}{rows}{}", &current[..begin], &current[end..]);
        return (replaced != current).then_some(replaced);
    }
    // The body without comments and blank lines decides the shape: empty, the template's `[]`, or
    // a block list of `- ` entries. A flow list with entries or a mapping is left to the user.
    let body: Vec<&str> = current
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .collect();
    let is_block_list = body
        .first()
        .is_some_and(|line| line.starts_with("- ") || *line == "-");
    if body.is_empty() || is_block_list {
        let separator = if current.is_empty() || current.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        return Some(format!("{current}{separator}{rows}"));
    }
    if body.len() == 1 && body[0].trim() == "[]" {
        let replaced = current
            .lines()
            .map(|line| {
                if line.trim() == "[]" {
                    rows.trim_end()
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Some(format!("{replaced}\n"));
    }
    None
}

fn write_if_changed(path: &Path, contents: &str) -> std::io::Result<bool> {
    if fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let staging = path.with_extension(format!("ghostex-{}.tmp", std::process::id()));
    fs::write(&staging, contents)?;
    fs::rename(&staging, path)?;
    Ok(true)
}

static LAST_RESULT: Mutex<Option<String>> = Mutex::new(None);

/// Writes the plugin and mounts it in the dsh-tui profile's patch layer, once both are current.
/// Runs before every DeepSeek Harness launch; quiet when nothing changed, and never fails the
/// launch (a session whose plugin is missing keeps the inject socket and screen paths).
pub(crate) fn ensure_dsh_chat_plugin() {
    if cfg!(test) {
        return;
    }
    let paths = crate::paths::get_gxserver_paths(None);
    let outcome = install(&paths.app_data_dir, &crate::session_chat_dsh::dsh_home());
    let summary = match &outcome {
        Ok(changes) => format!("ok {changes:?}"),
        Err(error) => format!("error {error}"),
    };
    let mut last = LAST_RESULT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if last.as_deref() == Some(summary.as_str()) {
        return;
    }
    *last = Some(summary);
    let (level, details) = match outcome {
        Ok(changes) => (
            crate::logging::LogLevel::Info,
            serde_json::json!({ "pluginWritten": changes.0, "patchLayer": changes.1, "version": DSH_CHAT_PLUGIN_VERSION }),
        ),
        Err(error) => (
            crate::logging::LogLevel::Warn,
            serde_json::json!({ "error": error.to_string() }),
        ),
    };
    crate::dsh_chat_channel::log(level, "dshChatChannel.pluginInstall", details);
}

/// `(plugin written, what happened to the patch layer)`.
fn install(app_data_dir: &Path, dsh_home: &Path) -> std::io::Result<(bool, &'static str)> {
    let plugin = plugin_path(app_data_dir);
    let written = write_if_changed(&plugin, &plugin_source())?;
    let profile = dsh_home.join("profiles").join("dsh-tui");
    // dsh-tui creates its profile on first run; until then there is nothing to mount into.
    if !profile.join("package.json").is_file() {
        return Ok((written, "no dsh-tui profile yet"));
    }
    let layer = profile.join("cordis.patch.yml");
    let current = match fs::read_to_string(&layer) {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let Some(next) = patched_layer(&current, &marked_rows(&plugin)) else {
        let state = if current.contains(OPT_OUT) {
            "opted out"
        } else if current.contains(ROW_BEGIN) {
            "current"
        } else {
            "left alone (not a block list)"
        };
        return Ok((written, state));
    };
    write_if_changed(&layer, &next)?;
    Ok((written, "mounted"))
}

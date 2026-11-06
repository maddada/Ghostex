// ghostex-opencode-session-plugin-marker v4
import { spawn } from "node:child_process";

// CDXC:AgentHooks 2026-09-25 WHY:
// The v2 service is shared by unrelated terminal clients. Only the TUI owns both
// the current conversation and the Ghostex terminal environment.
export default {
  id: "ghostex-session",
  setup(ctx) {
    if (!process.env.GHOSTEX_SESSION_ID || process.env.GHOSTEX_OPENCODE_HOOKS_DISABLED === "1") return;
    const env = process.env;
    if (env.GHOSTEX_INTERNAL_PROMPT_GENERATION === "1" || env.GHOSTEX_INTERNAL_TITLE_GENERATION === "1") return;
    if (!(env.VSMUX_SESSION_STATE_FILE || env.GHOSTEX_SESSION_STATE_FILE || env.ghostex_SESSION_STATE_FILE)
      && !(env.GHOSTEX_GLOBAL_SESSION_REF && env.GHOSTEX_GXSERVER_BASE_URL && env.GHOSTEX_GXSERVER_AUTH_TOKEN_FILE)) return;
    let previous = "";
    let currentSession = "";
    let closed = false;
    // A fresh TUI has no conversation until its first prompt; create its own empty
    // conversation so the GPUI composer can submit that first prompt through the API.
    const startup = setTimeout(async () => {
      if (ctx.ui.router.current().type !== "home" || process.argv.some(arg => /^(?:--session(?:=|$)|-s|--continue(?:=|$)|-c$|--fork(?:=|$))/.test(arg))) return;
      try {
        const info = await ctx.client.session.create({ location: ctx.data.location.default() });
        if (!closed && ctx.ui.router.current().type === "home") ctx.ui.router.navigate({type:"session",sessionID:info.id});
      } catch { /* The TUI's connection and provider setup remain available. */ }
    }, 1500);
    // CDXC:SessionChat 2026-10-25 WHY:
    // User: "even when opencode is on v2+ it still shows wrong model in the chat view (doesnt show same model that's in the terminal view)".
    // The TUI keeps a model picked in its switcher (and the model a new conversation starts on) as its own
    // draft until its next submit, while the conversation itself has no model or the previous one, so
    // the chat showed another model and a prompt sent from the chat ran on OpenCode's default instead
    // (seen live: the TUI said Gemini 3.1 Pro, the chat's prompt ran on Kimi K3.1 Pro). Once the TUI's
    // pick has stood for a second while idle, it becomes the conversation's model, the way the TUI's own
    // submit commits it. A model chosen elsewhere (Ghostex's picker) is never pushed back over.
    const selectedModel = () => {
      try {
        const model = ctx.ui.model?.current?.();
        if (!model?.providerID || !model.modelID) return;
        return { providerID: model.providerID, id: model.modelID, ...(model.variant ? { variant: model.variant } : {}) };
      } catch { return; }
    };
    const modelKey = model => model?.providerID ? JSON.stringify([model.providerID, model.id, model.variant ?? "default"]) : "";
    const modelSync = { session: "", durable: "", selected: "", since: 0, skip: "", busy: false };
    const syncModel = (id, info, running) => {
      if (modelSync.busy || typeof ctx.client.session?.switchModel !== "function") return;
      const model = selectedModel();
      const selected = modelKey(model);
      const durable = modelKey(info.model);
      if (modelSync.session !== id) Object.assign(modelSync, { session: id, durable, selected, since: Date.now(), skip: "" });
      if (durable !== modelSync.durable) {
        // Changed outside this TUI while its pick stayed put: that pick is stale, not a new choice.
        if (selected === modelSync.selected) modelSync.skip = selected;
        modelSync.durable = durable;
      }
      if (selected !== modelSync.selected) Object.assign(modelSync, { selected, since: Date.now(), skip: "" });
      if (!model || selected === durable || selected === modelSync.skip || running || Date.now() - modelSync.since < 1000) return;
      modelSync.busy = true;
      Promise.resolve(ctx.client.session.switchModel({ sessionID: id, model }))
        // A model this conversation cannot use (another location's provider) is left to the TUI's own submit.
        .then(() => { modelSync.durable = selected; }, () => { modelSync.skip = selected; })
        .finally(() => { modelSync.busy = false; });
    };
    let pending = false;
    let activeChild;
    let timeout;
    let retryAt = 0;
    const failures = new Set();
    const failed = reason => {
      retryAt = Date.now() + 5000;
      if (failures.has(reason)) return;
      failures.add(reason);
      console.error(`[Ghostex] OpenCode hook failed: ${reason}`);
    };
    const report = () => {
      if (closed || pending || Date.now() < retryAt) return;
      const route = ctx.ui.router.current();
      if (route.type !== "session") return;
      const id = ctx.data.session.root(route.sessionID);
      const info = ctx.data.session.get(id);
      if (!info || info.parentID) return;
      const permissions = ctx.data.session.permission.list(id) ?? [];
      const forms = ctx.data.session.form.list(id) ?? [];
      const running = ctx.data.session.status(id) === "running";
      syncModel(id, info, running);
      const event = id !== currentSession ? "SessionStart" : permissions.length ? "PermissionRequest" : forms.length ? "AskUserQuestion" : running ? "SessionBusy" : "Stop";
      const identity = JSON.stringify([id, event, info.title, info.time.updated]);
      if (identity === previous) return;
      pending = true;
      const payload = { agent: "opencode", session_id: id, cwd: info.location.directory,
        hook_event_name: event, event, title: info.title };
      let failure;
      let finished = false;
      const finish = reason => {
        if (finished) return;
        finished = true;
        clearTimeout(timeout);
        activeChild = undefined;
        pending = false;
        if (closed) return;
        if (reason) failed(reason);
        else { previous = identity; currentSession = id; failures.clear(); retryAt = 0; }
      };
      try {
        const child = process.platform === "win32"
          ? spawn(__GXSERVER__, ["agent-hook-notify-native", __NOTIFY__, "opencode"], { windowsHide: true, stdio: ["pipe", "ignore", "ignore"] })
          : spawn(__NOTIFY__, [], { stdio: ["pipe", "ignore", "ignore"] });
        activeChild = child;
        child.stdin.on("error", error => { failure = error.code || error.name; child.kill(); });
        child.once("error", error => finish(error.code || error.name));
        child.once("close", (code, signal) => finish(failure || (code === 0 ? null : signal || `exit ${code}`)));
        timeout = setTimeout(() => { failure = "timeout"; child.kill(); }, 5000);
        child.stdin.end(JSON.stringify(payload));
      } catch (error) { activeChild?.kill(); finish(error.code || error.name || "spawn failed"); }
    };
    const timer = setInterval(report, 500);
    report();
    // CDXC:PiChatChannel 2026-11-05 WHY:
    // OpenCode 2 has no service route for the TUI's prompt text, and its Ctrl+G (the key the
    // chat typed to start the draft handoff) jumps to the first message. The chat asks this
    // plugin through the same local channel and token the Pi extension uses
    // (server/src/pi_chat_channel.rs), and it runs the TUI's own `prompt.editor` command, which
    // hands the prompt to Ghostex's editor script.
    const instance = `${process.pid}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
    let poll;
    const channelPost = async (params, ms, signal) => {
      const { readFileSync } = await import("node:fs");
      const tokenFile = env.GHOSTEX_GXSERVER_AUTH_TOKEN_FILE.replace(/^~(?=\/)/, env.HOME || "");
      const token = readFileSync(tokenFile, "utf8").trim();
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), ms);
      signal?.addEventListener("abort", () => controller.abort(), { once: true });
      try {
        const response = await fetch(`${env.GHOSTEX_GXSERVER_BASE_URL.replace(/\/+$/, "")}/api/agentChatChannel`, {
          method: "POST",
          headers: { authorization: `Bearer ${token}`, "content-type": "application/json", "x-gxserver-protocol-version": env.GHOSTEX_GXSERVER_PROTOCOL_VERSION || "1" },
          body: JSON.stringify({ protocolVersion: Number(env.GHOSTEX_GXSERVER_PROTOCOL_VERSION) || 1, params: { sessionRef: env.GHOSTEX_GLOBAL_SESSION_REF, ...params } }),
          signal: controller.signal,
        });
        const body = await response.json().catch(() => ({}));
        if (!response.ok || body.ok === false) throw new Error(body.message || `gxserver answered ${response.status}`);
        return body.result || {};
      } finally { clearTimeout(timer); }
    };
    const runCommand = command => {
      const commands = { promptEditor: "prompt.editor", promptClear: "prompt.clear" };
      if (commands[command.kind] && typeof ctx.keymap?.dispatch === "function") {
        ctx.keymap.dispatch(commands[command.kind]);
        return { dispatched: true };
      }
      const failure = new Error(`OpenCode cannot run ${command.kind} from its plugin`);
      failure.unsupported = true;
      throw failure;
    };
    if (env.GHOSTEX_GLOBAL_SESSION_REF && env.GHOSTEX_GXSERVER_BASE_URL && env.GHOSTEX_GXSERVER_AUTH_TOKEN_FILE) {
      (async () => {
        let failures = 0;
        while (!closed) {
          poll = new AbortController();
          try {
            const result = await channelPost({ action: "poll", agent: "opencode", pid: process.pid, instance, version: 1, capabilities: ["promptEditor", "promptClear"] }, 60000, poll.signal);
            failures = 0;
            for (const command of result.commands || []) {
              let ack;
              try { ack = { action: "ack", id: command.id, ok: true, result: runCommand(command) }; }
              catch (error) { ack = { action: "ack", id: command.id, ok: false, unsupported: error.unsupported === true, error: String(error.message || error) }; }
              await channelPost(ack, 5000).catch(() => {});
            }
          } catch {
            if (closed) break;
            failures += 1;
            await new Promise(resolve => setTimeout(resolve, Math.min(5000, 250 * 2 ** Math.min(failures, 5))));
          }
        }
      })();
    }
    return () => {
      closed = true; clearTimeout(startup); clearInterval(timer); clearTimeout(timeout); activeChild?.kill();
      poll?.abort();
      channelPost({ action: "close", instance }, 2000).catch(() => {});
    };
  },
};

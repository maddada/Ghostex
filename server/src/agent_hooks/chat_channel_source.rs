//! The chat channel part of Ghostex's Pi and OMP extensions; gxserver's side is
//! `pi_chat_channel.rs` (CDXC:PiChatChannel 2026-11-05 there says why).

/// Bumped whenever the channel's commands or answers change; gxserver reads it from each poll.
pub(crate) const CHAT_CHANNEL_VERSION: u32 = 2;

/// TypeScript inserted into both extensions before their `export default`. It uses the
/// extensions' own `firstString` and `isIdle` helpers, and `__CHAT_AGENT__` / `__CHAT_LABEL__`
/// name the agent.
const CHAT_CHANNEL_SOURCE: &str = r###"
// Ghostex chat channel: the session's chat sends messages and picks the model and thinking level
// through this process's own API instead of typing into the terminal and reading it back. It polls
// gxserver with the same address, token and session ref the hook reports use.
const CHAT_CHANNEL_VERSION = __CHAT_CHANNEL_VERSION__;
const CHAT_AGENT: string = "__CHAT_AGENT__";
const CHAT_CHANNEL_CAPABILITIES = ["state", "send", "model", "thinking", "abort", "name", "editor", "compact", "dialogs", "commands"];
const CHAT_THINKING_LEVELS = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const CHAT_IMAGE_TYPES: Record<string, string> = {
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
};

type ChatChannelEnv = { baseUrl: string; tokenFile: string; sessionRef: string; protocol: string };
type ChatCommand = { id?: unknown; kind?: unknown; params?: unknown };
type ChatDialog = { id: string; kind: string; title: string; detail: string | null; options: string[]; answer: (value: unknown) => void };

class ChatUnsupported extends Error {}

function chatChannelEnv(): ChatChannelEnv | null {
  if (process.env.GHOSTEX_CHAT_CHANNEL_DISABLED === "1") return null;
  const baseUrl = firstString(process.env.GHOSTEX_GXSERVER_BASE_URL);
  const tokenFile = firstString(process.env.GHOSTEX_GXSERVER_AUTH_TOKEN_FILE);
  const sessionRef = firstString(process.env.GHOSTEX_GLOBAL_SESSION_REF);
  if (!baseUrl || !tokenFile || !sessionRef) return null;
  return {
    baseUrl: baseUrl.replace(/\/+$/, ""),
    tokenFile,
    sessionRef,
    protocol: firstString(process.env.GHOSTEX_GXSERVER_PROTOCOL_VERSION) ?? "__GXSERVER_PROTOCOL_VERSION__",
  };
}

function chatSleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    (timer as { unref?: () => void }).unref?.();
  });
}

function chatRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" ? (value as Record<string, unknown>) : {};
}

function chatSafe<T>(read: () => T): T | undefined {
  try {
    return read();
  } catch (_) {
    return undefined;
  }
}

class GhostexChatChannel {
  private ctx: ExtensionContext | null = null;
  private running = false;
  private poll: AbortController | null = null;
  private readonly instance = `${process.pid}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  private inputs: Array<{ seq: number; text: string }> = [];
  private inputSeq = 0;
  private starts = 0;
  private dialogs: ChatDialog[] = [];
  private dialogSeq = 0;

  private readonly api: ExtensionAPI;

  constructor(api: ExtensionAPI) {
    this.api = api;
  }

  // Only the terminal's own top-level agent answers the chat: print/json/rpc runs and OMP
  // subagents (which load the same extension) never open the channel.
  start(ctx: ExtensionContext): void {
    const typed = ctx as unknown as { mode?: unknown; hasUI?: unknown; agent?: { kind?: unknown } };
    if (typed.mode !== undefined ? typed.mode !== "tui" : typed.hasUI === false) return;
    if (typed.agent?.kind === "sub") return;
    if (!chatChannelEnv()) return;
    this.ctx = ctx;
    this.watchDialogs(ctx);
    if (this.running) return;
    this.running = true;
    void this.loop();
  }

  stop(): void {
    if (!this.running) return;
    this.running = false;
    this.poll?.abort();
    const env = chatChannelEnv();
    if (env) void this.post(env, { action: "close", instance: this.instance }, 2000).catch(() => {});
    this.ctx = null;
  }

  // Pi asks whether to trust a folder before the session starts. A folder the user told Ghostex to
  // trust and remember (Trust and Remember) is answered here, so no trust dialog is drawn; any
  // other folder is left to Pi's own dialog.
  async trust(cwd: unknown): Promise<{ trusted: "yes" | "undecided" }> {
    const env = chatChannelEnv();
    const folder = firstString(cwd);
    if (!env || !folder) return { trusted: "undecided" };
    try {
      const result = await this.post(env, { action: "trust", cwd: folder }, 2000);
      return { trusted: result.trusted === "yes" ? "yes" : "undecided" };
    } catch (_) {
      return { trusted: "undecided" };
    }
  }

  // Dialogs (an extension's question, a confirmation) go through the session's shared ui object.
  // Each one opened is kept with its title and options so the chat can answer it; an answer from
  // the chat settles it and dismisses the TUI's copy through the dialog's own abort signal. The
  // TUI keeps answering it as before.
  private watchDialogs(ctx: ExtensionContext): void {
    const ui = chatSafe(() => ctx.ui) as unknown as Record<string, unknown> | undefined;
    if (!ui || typeof ui !== "object" || (ui as { __ghostexChatDialogs?: unknown }).__ghostexChatDialogs) return;
    for (const kind of ["select", "confirm", "input"]) {
      const original = ui[kind];
      if (typeof original !== "function") continue;
      const bound = (original as (...args: unknown[]) => unknown).bind(ui);
      ui[kind] = (...args: unknown[]) => this.trackDialog(kind, bound, args);
    }
    try {
      Object.defineProperty(ui, "__ghostexChatDialogs", { value: true });
    } catch (_) {}
  }

  private trackDialog(kind: string, original: (...args: unknown[]) => unknown, args: unknown[]): Promise<unknown> {
    const options = chatRecord(args[2]);
    const controller = new AbortController();
    const outer = options.signal as AbortSignal | undefined;
    if (outer?.aborted) controller.abort();
    else outer?.addEventListener?.("abort", () => controller.abort(), { once: true });
    const forwarded = [...args];
    while (forwarded.length < 2) forwarded.push(undefined);
    forwarded[2] = { ...options, signal: controller.signal };
    return new Promise((resolve) => {
      this.dialogSeq += 1;
      let settled = false;
      const dialog: ChatDialog = {
        id: `d${this.dialogSeq}`,
        kind,
        title: typeof args[0] === "string" ? args[0] : "",
        detail: kind === "select" ? null : typeof args[1] === "string" ? args[1] : null,
        options: kind === "select" && Array.isArray(args[1]) ? args[1].map((option) => String(option)) : [],
        answer: (value) => {
          finish(value);
          controller.abort();
        },
      };
      const finish = (value: unknown) => {
        if (settled) return;
        settled = true;
        this.dialogs = this.dialogs.filter((open) => open !== dialog);
        resolve(value);
      };
      this.dialogs.push(dialog);
      try {
        Promise.resolve(original(...forwarded)).then(finish, () => finish(undefined));
      } catch (_) {
        finish(undefined);
      }
    });
  }

  private openDialogs(): Array<Record<string, unknown>> {
    return this.dialogs.map(({ id, kind, title, detail, options }) => ({ id, kind, title, detail, options }));
  }

  // Answers the dialogs a question opens, one after another (a question tool opens the next one
  // only after the previous answer). A custom answer picks the select's free-text row and then
  // types the text into the input that row opens.
  private async answerDialogs(params: Record<string, unknown>): Promise<Record<string, unknown>> {
    const answers = Array.isArray(params.answers) ? params.answers.map(chatRecord) : [];
    let answered = 0;
    const next = async (kind?: string): Promise<ChatDialog> => {
      const found = await this.waitFor(() => this.dialogs.some((open) => !kind || open.kind === kind), answered === 0 ? 1500 : 5000);
      const dialog = this.dialogs.find((open) => !kind || open.kind === kind);
      if (!found || !dialog) {
        if (answered === 0) throw new ChatUnsupported("no __CHAT_LABEL__ dialog is open");
        throw new Error(`__CHAT_LABEL__ did not open the next dialog after ${answered} answer(s)`);
      }
      return dialog;
    };
    for (const answer of answers) {
      const dialog = await next();
      if (answer.cancel === true) {
        dialog.answer(undefined);
        answered += 1;
        break;
      }
      const label = firstString(answer.label);
      const text = typeof answer.text === "string" && answer.text.trim() ? answer.text : null;
      if (dialog.kind === "select") {
        const exact = label ? dialog.options.find((option) => option === label) : undefined;
        const loose = label ? dialog.options.find((option) => option.trim().startsWith(label.trim())) : undefined;
        const custom = dialog.options.find((option) => /custom answer|type your own|type something|^other\b/i.test(option));
        const choice = exact ?? loose ?? (text ? custom : undefined);
        if (!choice) throw new Error(`"${label ?? text ?? ""}" is not one of the choices of "${dialog.title}"`);
        dialog.answer(choice);
        answered += 1;
        if (choice === custom && text && choice !== exact) {
          const input = await next("input");
          input.answer(text);
        }
      } else if (dialog.kind === "confirm") {
        dialog.answer(/^(yes|y|true|ok|allow|confirm)/i.test(label ?? text ?? ""));
        answered += 1;
      } else {
        dialog.answer(text ?? label ?? "");
        answered += 1;
      }
    }
    return { answered, ...this.state() };
  }

  private editor(params: Record<string, unknown>): Record<string, unknown> {
    const ui = chatSafe(() => this.context().ui) as unknown as
      | { getEditorText?: () => string; setEditorText?: (text: string) => void; setStatus?: (key: string, text: string | undefined) => void }
      | undefined;
    // Pi's setEditorText changes the box without redrawing it; clearing an unused status key asks
    // the TUI for the redraw, so the change shows at once instead of on the next keypress.
    const redraw = () => chatSafe(() => ui?.setStatus?.("ghostex-chat", undefined));
    if (typeof ui?.getEditorText !== "function" || typeof ui?.setEditorText !== "function") {
      throw new ChatUnsupported("this __CHAT_LABEL__ cannot read its input box from an extension");
    }
    const current = String(ui.getEditorText() ?? "");
    switch (params.action) {
      case "read":
        return { text: current };
      case "clear": {
        // Only the text the caller saved is cleared; anything typed since stays.
        if (current !== params.expected) return { cleared: false, text: current };
        ui.setEditorText("");
        redraw();
        return { cleared: true, text: "" };
      }
      case "replace": {
        const text = typeof params.text === "string" ? params.text : "";
        ui.setEditorText(text);
        redraw();
        return { previous: current, text };
      }
      default:
        throw new ChatUnsupported(`unknown editor action ${String(params.action)}`);
    }
  }

  // The built-in commands the extension API can run itself; `null` leaves the text to the TUI.
  private async builtinCommand(ctx: ExtensionContext, text: string): Promise<Record<string, unknown> | null> {
    const match = /^\/(\S+)(?:\s+([\s\S]*))?$/.exec(text);
    if (!match) return null;
    const name = match[1];
    const arg = (match[2] ?? "").trim();
    const omp = CHAT_AGENT === "omp";
    if (name === "compact") {
      this.compact(ctx, arg || undefined);
      return { command: "compact", ...this.state() };
    }
    if ((name === (omp ? "rename" : "name")) && arg) {
      return { command: name, ...(await this.run("name", { name: arg })) };
    }
    if (!omp && name === "thinking" && arg && CHAT_THINKING_LEVELS.includes(arg)) {
      return { command: name, ...(await this.run("thinking", { level: arg })) };
    }
    if ((omp ? name === "switch" : name === "model") && /^[^\s/]+\/\S+$/.test(arg)) {
      const [spec, level] = omp ? arg.split(/:(?=[a-z]+$)/) : [arg, undefined];
      const slash = spec.indexOf("/");
      return {
        command: name,
        ...(await this.run("model", { provider: spec.slice(0, slash), model: spec.slice(slash + 1), thinking: level })),
      };
    }
    return null;
  }

  private compact(ctx: ExtensionContext, instructions: string | undefined): void {
    const typed = ctx as unknown as { compact?: (options?: unknown) => unknown };
    if (typeof typed.compact !== "function") throw new ChatUnsupported("this __CHAT_LABEL__ cannot compact from an extension");
    const omp = CHAT_AGENT === "omp";
    const result = typed.compact(omp ? instructions : instructions ? { customInstructions: instructions } : undefined);
    void Promise.resolve(result).catch(() => {});
  }

  noteInput(event: unknown): void {
    const typed = chatRecord(event);
    if (typed.source !== "extension" || typeof typed.text !== "string") return;
    this.inputSeq += 1;
    this.inputs.push({ seq: this.inputSeq, text: typed.text.trim() });
    if (this.inputs.length > 20) this.inputs.shift();
  }

  noteStart(): void {
    this.starts += 1;
  }

  private async post(env: ChatChannelEnv, params: Record<string, unknown>, timeoutMs: number, signal?: AbortSignal): Promise<Record<string, unknown>> {
    let token = "";
    try {
      const file = env.tokenFile.startsWith("~/") ? path.join(process.env.HOME || "", env.tokenFile.slice(2)) : env.tokenFile;
      token = fs.readFileSync(file, "utf8").trim();
    } catch (_) {}
    if (!token) throw new Error("no gxserver token");
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    const abort = () => controller.abort();
    signal?.addEventListener("abort", abort);
    try {
      const response = await fetch(`${env.baseUrl}/api/agentChatChannel`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${token}`,
          "content-type": "application/json",
          "x-gxserver-protocol-version": env.protocol,
        },
        body: JSON.stringify({ protocolVersion: Number(env.protocol) || 1, params: { sessionRef: env.sessionRef, ...params } }),
        signal: controller.signal,
      });
      const body = chatRecord(await response.json().catch(() => ({})));
      if (!response.ok || body.ok === false) throw new Error(firstString(body.message, body.error) ?? `gxserver answered ${response.status}`);
      return chatRecord(body.result);
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
    }
  }

  private async loop(): Promise<void> {
    let failures = 0;
    while (this.running) {
      const env = chatChannelEnv();
      const ctx = this.ctx;
      if (!env || !ctx) break;
      this.poll = new AbortController();
      try {
        const result = await this.post(
          env,
          {
            action: "poll",
            agent: CHAT_AGENT,
            pid: process.pid,
            instance: this.instance,
            version: CHAT_CHANNEL_VERSION,
            capabilities: CHAT_CHANNEL_CAPABILITIES,
            agentSessionId: chatSafe(() => ctx.sessionManager.getSessionId()) ?? undefined,
          },
          60000,
          this.poll.signal,
        );
        failures = 0;
        const commands = Array.isArray(result.commands) ? (result.commands as ChatCommand[]) : [];
        for (const command of commands) await this.answer(env, command);
      } catch (_) {
        if (!this.running) break;
        failures += 1;
        // gxserver restarting or not up yet: keep trying, never more than a few seconds apart.
        await chatSleep(Math.min(5000, 250 * 2 ** Math.min(failures, 5)));
      }
    }
    this.running = false;
  }

  private async answer(env: ChatChannelEnv, command: ChatCommand): Promise<void> {
    const id = firstString(command.id);
    if (!id) return;
    let ack: Record<string, unknown>;
    try {
      const result = await this.run(firstString(command.kind) ?? "", chatRecord(command.params));
      ack = { action: "ack", id, ok: true, result };
    } catch (error) {
      ack = {
        action: "ack",
        id,
        ok: false,
        unsupported: error instanceof ChatUnsupported,
        error: error instanceof Error ? error.message : String(error),
      };
    }
    for (let attempt = 0; attempt < 3; attempt += 1) {
      try {
        await this.post(env, ack, 5000);
        return;
      } catch (_) {
        await chatSleep(300);
      }
    }
  }

  private context(): ExtensionContext {
    if (!this.ctx) throw new Error("__CHAT_LABEL__ has no session yet");
    return this.ctx;
  }

  private state(): Record<string, unknown> {
    const ctx = this.context();
    const model = chatSafe(() => ctx.model) as { provider?: unknown; id?: unknown; name?: unknown; reasoning?: unknown } | undefined;
    const api = this.api as unknown as { getThinkingLevel?: () => unknown };
    const typed = ctx as unknown as { hasPendingMessages?: () => boolean; getContextUsage?: () => unknown };
    return {
      agentSessionId: chatSafe(() => ctx.sessionManager.getSessionId()) ?? null,
      sessionFile: chatSafe(() => ctx.sessionManager.getSessionFile()) ?? null,
      model: model ? { provider: model.provider ?? null, id: model.id ?? null, name: model.name ?? null, reasoning: model.reasoning === true } : null,
      thinkingLevel: chatSafe(() => api.getThinkingLevel?.()) ?? null,
      idle: isIdle(ctx),
      pendingMessages: chatSafe(() => typed.hasPendingMessages?.()) ?? false,
      contextUsage: chatSafe(() => typed.getContextUsage?.()) ?? null,
      sessionName: chatSafe(() => (this.api as unknown as { getSessionName?: () => unknown }).getSessionName?.()) ?? null,
      dialogs: this.openDialogs(),
    };
  }

  private async waitFor(check: () => boolean, ms: number): Promise<boolean> {
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      if (check()) return true;
      await chatSleep(25);
    }
    return check();
  }

  private findModel(provider: string | null, id: string): unknown {
    const ctx = this.context();
    const registry = chatRecord(chatSafe(() => ctx.modelRegistry)) as {
      find?: (provider: string, id: string) => unknown;
      getAvailable?: () => unknown[];
      getAll?: () => unknown[];
    };
    if (provider && typeof registry.find === "function") {
      const found = chatSafe(() => registry.find!(provider, id));
      if (found) return found;
    }
    const query = chatRecord((ctx as unknown as { models?: unknown }).models) as { resolve?: (spec: string) => unknown };
    if (typeof query.resolve === "function") {
      const found = chatSafe(() => query.resolve!(provider ? `${provider}/${id}` : id));
      if (found) return found;
    }
    const all = [...(chatSafe(() => registry.getAvailable?.()) ?? []), ...(chatSafe(() => registry.getAll?.()) ?? [])];
    return all.find((model) => {
      const typed = chatRecord(model);
      if (provider && typed.provider !== provider) return false;
      return typed.id === id || `${String(typed.provider)}/${String(typed.id)}` === id;
    });
  }

  private setThinking(level: string): void {
    if (!CHAT_THINKING_LEVELS.includes(level)) throw new Error(`__CHAT_LABEL__ has no thinking level ${level}`);
    const api = this.api as unknown as { setThinkingLevel?: (level: string) => void };
    if (typeof api.setThinkingLevel !== "function") throw new ChatUnsupported("this __CHAT_LABEL__ cannot set the thinking level from an extension");
    api.setThinkingLevel(level);
  }

  private async run(kind: string, params: Record<string, unknown>): Promise<Record<string, unknown>> {
    const ctx = this.context();
    switch (kind) {
      case "state":
        return this.state();
      case "model": {
        const id = firstString(params.model);
        if (!id) throw new Error("A model pick names its model");
        const provider = firstString(params.provider);
        const model = this.findModel(provider, id);
        if (!model) throw new Error(`__CHAT_LABEL__ has no model ${provider ? `${provider}/` : ""}${id}`);
        const api = this.api as unknown as { setModel?: (model: unknown) => Promise<boolean> };
        if (typeof api.setModel !== "function") throw new ChatUnsupported("this __CHAT_LABEL__ cannot set the model from an extension");
        if (!(await api.setModel(model))) throw new Error(`__CHAT_LABEL__ has no login for ${String(chatRecord(model).provider)}`);
        const thinking = firstString(params.thinking);
        if (thinking) this.setThinking(thinking);
        return this.state();
      }
      case "thinking": {
        const level = firstString(params.level);
        if (!level) throw new Error("A thinking pick names its level");
        this.setThinking(level);
        return this.state();
      }
      case "abort": {
        if (!isIdle(ctx)) {
          ctx.abort();
          await this.waitFor(() => isIdle(ctx), 5000);
        }
        return this.state();
      }
      case "send":
        return this.send(ctx, params);
      case "name": {
        const name = firstString(params.name);
        if (!name) throw new Error("A session name is not empty");
        const api = this.api as unknown as { setSessionName?: (name: string) => void; getSessionName?: () => unknown };
        if (typeof api.setSessionName !== "function") throw new ChatUnsupported("this __CHAT_LABEL__ cannot name the session from an extension");
        api.setSessionName(name);
        return { sessionName: chatSafe(() => api.getSessionName?.()) ?? name, ...this.state() };
      }
      case "editor":
        return this.editor(params);
      case "compact":
        this.compact(ctx, firstString(params.instructions) ?? undefined);
        return this.state();
      case "answerDialogs":
        return this.answerDialogs(params);
      default:
        throw new ChatUnsupported(`unknown chat command ${kind}`);
    }
  }

  private async send(ctx: ExtensionContext, params: Record<string, unknown>): Promise<Record<string, unknown>> {
    const text = typeof params.text === "string" ? params.text : "";
    const imagePaths = Array.isArray(params.images) ? params.images.filter((value): value is string => typeof value === "string") : [];
    if (!text.trim() && imagePaths.length === 0) throw new Error("Nothing to send");
    // Built-in commands (/model, /compact, /new...) belong to the TUI's editor; sending them as a
    // message would hand the model the literal text, so the chat types those instead.
    let command: string | null = null;
    if (text.startsWith("/") && imagePaths.length === 0) {
      const builtin = await this.builtinCommand(ctx, text.trim());
      if (builtin) return { delivered: "command", ...builtin };
    }
    if (text.startsWith("/")) {
      const name = text.slice(1).split(/\s/, 1)[0];
      const commands = chatSafe(() => (this.api as unknown as { getCommands?: () => Array<{ name?: unknown; source?: unknown }> }).getCommands?.()) ?? [];
      const known = commands.find((entry) => entry.name === name);
      if (!known) throw new ChatUnsupported(`/${name} is a __CHAT_LABEL__ terminal command`);
      command = firstString(known.source) ?? "command";
    }
    const images = imagePaths.map((file) => {
      const mimeType = CHAT_IMAGE_TYPES[path.extname(file).toLowerCase()];
      if (!mimeType) throw new Error(`${path.basename(file)} is not a picture __CHAT_LABEL__ can read`);
      return { type: "image", data: fs.readFileSync(file).toString("base64"), mimeType };
    });
    if (params.interrupt === true && !isIdle(ctx)) {
      ctx.abort();
      await this.waitFor(() => isIdle(ctx), 5000);
    }
    const idle = isIdle(ctx);
    const deliverAs = idle ? undefined : params.deliverAs === "steer" ? "steer" : "followUp";
    const expected = text.trim();
    const seqBefore = this.inputSeq;
    const startsBefore = this.starts;
    const typed = ctx as unknown as { hasPendingMessages?: () => boolean };
    const content = images.length > 0 ? [{ type: "text", text }, ...images] : text;
    (this.api as unknown as { sendUserMessage: (content: unknown, options?: unknown) => void }).sendUserMessage(content, {
      deliverAs,
      expandPromptTemplates: true,
    });
    // An extension command runs straight away and starts nothing; everything else passes `input`.
    if (command === "extension") return { delivered: "command", ...this.state() };
    const taken = await this.waitFor(
      () =>
        this.inputs.some((input) => input.seq > seqBefore && input.text === expected) ||
        this.starts > startsBefore ||
        (!idle && chatSafe(() => typed.hasPendingMessages?.()) === true),
      5000,
    );
    if (!taken) throw new Error("__CHAT_LABEL__ did not take the message");
    if (idle) {
      const started = await this.waitFor(() => this.starts > startsBefore || !isIdle(ctx), 5000);
      if (!started) throw new Error("__CHAT_LABEL__ took the message but did not start (is a model and its login set up?)");
      return { delivered: "started", ...this.state() };
    }
    return { delivered: deliverAs, ...this.state() };
  }
}
"###;

pub(crate) fn chat_channel_source(agent: &str, label: &str) -> String {
    CHAT_CHANNEL_SOURCE
        .replace(
            "__CHAT_CHANNEL_VERSION__",
            &CHAT_CHANNEL_VERSION.to_string(),
        )
        .replace(
            "__GXSERVER_PROTOCOL_VERSION__",
            &crate::constants::GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .replace("__CHAT_AGENT__", agent)
        .replace("__CHAT_LABEL__", label)
}

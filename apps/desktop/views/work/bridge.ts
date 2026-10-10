import type { CurrentSession, WorkItemRef } from "./types";

type GhostexGpuiWorkApi = {
  postProjectBoardRequest?: (payload: string) => boolean;
};

type WorkWindow = Window & { ghostexGpui?: GhostexGpuiWorkApi };

type WorkResponse = {
  requestId: string;
  ok: boolean;
  payload?: unknown;
  error?: string;
};

const RESPONSE_EVENT = "ghostex-work-response";
const OPEN_EVENT = "ghostex-work-open";
const REFRESH_EVENT = "ghostex-work-refresh";
const CURRENT_SESSION_EVENT = "ghostex-work-current-session";
const BRIDGE_RETRY_MS = 25;
/** CEF installs the bridge right after the page's first load; past this the page is not in the app. */
const BRIDGE_WAIT_MS = 10_000;

const pending = new Map<
  string,
  { resolve: (value: unknown) => void; reject: (error: Error) => void }
>();
let nextRequestId = 1;
let fixtureAnswer:
  | ((action: string, params: Record<string, unknown>) => Promise<unknown>)
  | null = null;

window.addEventListener(RESPONSE_EVENT, (event) => {
  const detail = (event as CustomEvent<WorkResponse>).detail;
  const request = detail && pending.get(detail.requestId);
  if (!request) return;
  pending.delete(detail.requestId);
  if (detail.ok) request.resolve(detail.payload);
  else request.reject(new Error(detail.error || "The request failed."));
});

/** The page's own dev mode: `?fixtures=1` answers every request from sample data, with no app. */
export function isFixtureMode(): boolean {
  return new URLSearchParams(location.search).get("fixtures") === "1";
}

export function installFixtureAnswers(
  answer: (action: string, params: Record<string, unknown>) => Promise<unknown>,
): void {
  fixtureAnswer = answer;
}

/**
 * CDXC:WorkMode 2026-10-09 WHY:
 * The Work page talks to the app the way the Files embed page does: through the fixed project
 * workarea bridge function CEF installs on first-party workarea pages, never to gxserver directly,
 * so the page holds no token and the app decides what each request may do
 * (apps/desktop/src/app/work_view/bridge.rs).
 */
export function workRequest<T>(
  action: string,
  params: Record<string, unknown> = {},
  timeoutMs = 120_000,
): Promise<T> {
  if (fixtureAnswer) return fixtureAnswer(action, params) as Promise<T>;
  const requestId = `work-${Date.now()}-${nextRequestId++}`;
  const payload = JSON.stringify({ ...params, action, requestId });
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      if (pending.delete(requestId))
        reject(new Error("Ghostex did not answer in time."));
    }, timeoutMs);
    pending.set(requestId, {
      resolve: (value) => {
        window.clearTimeout(timer);
        resolve(value as T);
      },
      reject: (error) => {
        window.clearTimeout(timer);
        reject(error);
      },
    });
    const startedAt = Date.now();
    const send = () => {
      const post = (window as WorkWindow).ghostexGpui?.postProjectBoardRequest;
      if (typeof post === "function" && post(payload)) return;
      if (Date.now() - startedAt > BRIDGE_WAIT_MS) {
        if (pending.delete(requestId)) {
          window.clearTimeout(timer);
          reject(new Error("The Work page only runs inside the Ghostex app."));
        }
        return;
      }
      window.setTimeout(send, BRIDGE_RETRY_MS);
    };
    send();
  });
}

/** The app made a ticket (its Create Linear Ticket dialog): the list should read itself again. */
export function onWorkRefresh(listener: () => void): () => void {
  window.addEventListener(REFRESH_EVENT, listener);
  return () => window.removeEventListener(REFRESH_EVENT, listener);
}

/** A session card's chip asked the app to open one ticket while the page is showing. */
export function onWorkOpen(listener: (ref: WorkItemRef) => void): () => void {
  const handler = (event: Event) => {
    const detail = (event as CustomEvent<WorkItemRef>).detail;
    if (detail && typeof detail === "object") listener(detail);
  };
  window.addEventListener(OPEN_EVENT, handler);
  return () => window.removeEventListener(OPEN_EVENT, handler);
}

/** The window selected another session, or the selected one's title or links changed. */
export function onCurrentSession(
  listener: (session: CurrentSession | null) => void,
): () => void {
  const handler = (event: Event) => {
    const detail = (event as CustomEvent<CurrentSession | null>).detail;
    listener(detail && typeof detail === "object" ? detail : null);
  };
  window.addEventListener(CURRENT_SESSION_EVENT, handler);
  return () => window.removeEventListener(CURRENT_SESSION_EVENT, handler);
}

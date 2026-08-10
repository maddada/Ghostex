export const PROJECT_DOCS_GXSERVER_ENDPOINT = "/api/runProjectDocsAction";
export const PROJECT_DOCS_RESOURCE_ACTION = "readResource" as const;

export type ProjectDocsFileEntry = {
  depth: number;
  kind: "directory" | "file";
  modifiedAt?: string;
  name: string;
  path: string;
  size?: number;
};

export type ProjectDocsGitBaselineReason =
  | "binary"
  | "error"
  | "git-unavailable"
  | "ignored"
  | "not-file"
  | "not-repo"
  | "too-large";

export type ProjectDocsGitBaseline = {
  available: boolean;
  baseText?: string | null;
  headOid?: string | null;
  maxBytesExceeded?: boolean;
  reason?: ProjectDocsGitBaselineReason;
  tracked: boolean;
};

export type ProjectDocsFilePreview = {
  content?: string;
  /**
   * CDXC:DocsRootAdditive 2026-08-09:
   * `path` is the routing address a request must send back; `displayPath` is
   * the same file named the way the Docs tree names it, so a file under a
   * mounted Docs directory reads as `<mount name>/...` instead of the reserved
   * routing segment. Absent for hosts that predate it — fall back to `path`.
   */
  displayPath?: string;
  error?: string;
  gitBaseline?: ProjectDocsGitBaseline;
  kind: "text" | "unsupported";
  modifiedAt?: string;
  name: string;
  path: string;
  size?: number;
};

export type ProjectDocsRequest = {
  action:
    | "addToSessionContext"
    | "copyFullPath"
    | "list"
    | "read"
    | "stat"
    | "save"
    | "rename"
    | "delete"
    | "duplicate"
    | "createFolder"
    | "move"
    | "revealInFinder"
    | "openDocsFoldersSettings";
  content?: string;
  newPath?: string;
  path?: string;
  projectEditorId: string;
  projectId: string;
  requestId: string;
};

export type ProjectDocsResponse = {
  action: ProjectDocsRequest["action"];
  entries?: ProjectDocsFileEntry[];
  error?: string;
  file?: ProjectDocsFilePreview;
  requestId: string;
  rootName?: string;
};

export type ProjectDocsResourceRequest = {
  action: typeof PROJECT_DOCS_RESOURCE_ACTION;
  additionalDocsFolders: string;
  path: string;
  projectId: string;
  requestId: string;
};

export type ProjectDocsResourceResponse = {
  action: typeof PROJECT_DOCS_RESOURCE_ACTION;
  dataBase64?: string;
  error?: string;
  requestId: string;
};

export type ProjectDocsResourceTransport = (
  endpoint: typeof PROJECT_DOCS_GXSERVER_ENDPOINT,
  request: ProjectDocsResourceRequest,
) => Promise<unknown>;

export type ProjectDocsHostTransport = {
  eventName: string;
  eventTarget: EventTarget;
  postMessage: (request: ProjectDocsRequest) => void;
  timeoutMs: number;
};

export function createProjectDocsRequestId(prefix = "docs"): string {
  return `${prefix}-${globalThis.crypto.randomUUID()}`;
}

function isProjectDocsResponseRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function decodeProjectDocsResourceResponse(
  value: unknown,
  requestId: string,
): Uint8Array {
  if (
    !isProjectDocsResponseRecord(value) ||
    value.action !== PROJECT_DOCS_RESOURCE_ACTION ||
    value.requestId !== requestId
  ) {
    throw new Error("The Docs service returned an invalid resource response.");
  }
  if (typeof value.error === "string" && value.error.length > 0) {
    throw new Error(value.error);
  }
  if (typeof value.dataBase64 !== "string") {
    throw new Error("The Docs service returned an invalid resource response.");
  }
  const decoded = globalThis.atob(value.dataBase64);
  return Uint8Array.from(decoded, (character) => character.charCodeAt(0));
}

export async function readProjectDocsResource(
  request: Omit<ProjectDocsResourceRequest, "action" | "requestId">,
  transport: ProjectDocsResourceTransport,
): Promise<Uint8Array> {
  const requestId = createProjectDocsRequestId("docs-resource");
  const response = await transport(PROJECT_DOCS_GXSERVER_ENDPOINT, {
    ...request,
    action: PROJECT_DOCS_RESOURCE_ACTION,
    requestId,
  });
  return decodeProjectDocsResourceResponse(response, requestId);
}

/*
 * CDXC:RemoteProjectDocs 2026-08-06:
 * The Docs page owns one request/response contract independent of its host.
 * GPUI supplies the CEF event bridge today; a web client can use the exported
 * gxserver endpoint and the same request/response types without importing the
 * native Manage page or duplicating its timeout/correlation behavior.
 */
export function requestProjectDocsFromHost(
  request: Omit<ProjectDocsRequest, "requestId">,
  transport: ProjectDocsHostTransport,
): Promise<ProjectDocsResponse> {
  const requestId = createProjectDocsRequestId();
  const message: ProjectDocsRequest = { ...request, requestId };
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      transport.eventTarget.removeEventListener(transport.eventName, handleResponse);
      reject(new Error("Docs request timed out."));
    }, transport.timeoutMs);
    function handleResponse(event: Event) {
      const response = (event as CustomEvent<ProjectDocsResponse>).detail;
      if (response?.requestId !== requestId || response.action !== message.action) {
        return;
      }
      window.clearTimeout(timeout);
      transport.eventTarget.removeEventListener(transport.eventName, handleResponse);
      resolve(response);
    }
    transport.eventTarget.addEventListener(transport.eventName, handleResponse);
    transport.postMessage(message);
  });
}

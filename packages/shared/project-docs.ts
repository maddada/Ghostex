export const PROJECT_DOCS_GXSERVER_ENDPOINT = '/api/runProjectDocsAction';
export const PROJECT_DOCS_RESOURCE_ACTION = 'readResource' as const;

export type ProjectDocsFileEntry = {
  depth: number;
  /**
   * CDXC:DocsRootAdditive 2026-08-10:
   * The same entry named the way the Docs tree names it, so anything that puts
   * a path in front of a human — Copy Path, feedback pasted into a terminal —
   * reads as `<mount name>/...` instead of the reserved routing segment. Only
   * mounted entries carry it; the project's own files route by display name
   * already. Absent for hosts that predate it — fall back to `path`.
   */
  displayPath?: string;
  kind: 'directory' | 'file';
  modifiedAt?: string;
  name: string;
  path: string;
  size?: number;
};

export type ProjectDocsGitBaselineReason =
  'binary' | 'error' | 'git-unavailable' | 'ignored' | 'not-file' | 'not-repo' | 'too-large';

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
  kind: 'text' | 'unsupported';
  modifiedAt?: string;
  name: string;
  path: string;
  size?: number;
};

export type ProjectDocsRequest = {
  action:
    | 'addToSessionContext'
    | 'copyFullPath'
    | 'list'
    | 'read'
    | 'stat'
    | 'save'
    | 'rename'
    | 'delete'
    | 'duplicate'
    | 'createFolder'
    | 'move'
    | 'revealInFinder'
    | 'openDocsFoldersSettings';
  content?: string;
  newPath?: string;
  path?: string;
  projectEditorId: string;
  projectId: string;
  requestId: string;
};

export type ProjectDocsResponse = {
  action: ProjectDocsRequest['action'];
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
  request: ProjectDocsResourceRequest
) => Promise<unknown>;

export type ProjectMarkdownSaveTransport = (
  endpoint: typeof PROJECT_DOCS_GXSERVER_ENDPOINT,
  request: Record<string, unknown>
) => Promise<unknown>;

export type SaveProjectMarkdownDocumentParams = {
  content: string;
  path: string;
  projectId: string;
};

export type ProjectDocsHostTransport = {
  eventName: string;
  eventTarget: EventTarget;
  postMessage: (request: ProjectDocsRequest) => void;
  timeoutMs: number;
};

export function createProjectDocsRequestId(prefix = 'docs'): string {
  return `${prefix}-${globalThis.crypto.randomUUID()}`;
}

function isProjectDocsResponseRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function decodeProjectDocsResourceResponse(value: unknown, requestId: string): Uint8Array {
  if (
    !isProjectDocsResponseRecord(value) ||
    value.action !== PROJECT_DOCS_RESOURCE_ACTION ||
    value.requestId !== requestId
  ) {
    throw new Error('The Docs service returned an invalid resource response.');
  }
  if (typeof value.error === 'string' && value.error.length > 0) {
    throw new Error(value.error);
  }
  if (typeof value.dataBase64 !== 'string') {
    throw new Error('The Docs service returned an invalid resource response.');
  }
  const decoded = globalThis.atob(value.dataBase64);
  return Uint8Array.from(decoded, (character) => character.charCodeAt(0));
}

export async function readProjectDocsResource(
  request: Omit<ProjectDocsResourceRequest, 'action' | 'requestId'>,
  transport: ProjectDocsResourceTransport
): Promise<Uint8Array> {
  const requestId = createProjectDocsRequestId('docs-resource');
  const response = await transport(PROJECT_DOCS_GXSERVER_ENDPOINT, {
    ...request,
    action: PROJECT_DOCS_RESOURCE_ACTION,
    requestId,
  });
  return decodeProjectDocsResourceResponse(response, requestId);
}

function checkedProjectDocsResponse(value: unknown, requestId: string): Record<string, unknown> {
  if (!isProjectDocsResponseRecord(value) || value.requestId !== requestId) {
    throw new Error('The Docs service returned an invalid response.');
  }
  if (typeof value.error === 'string' && value.error.length > 0) {
    throw new Error(value.error);
  }
  return value;
}

export async function listProjectMarkdownDocumentPaths(
  projectId: string,
  transport: ProjectMarkdownSaveTransport
): Promise<readonly string[]> {
  const requestId = createProjectDocsRequestId('list-message-markdown');
  const response = checkedProjectDocsResponse(
    await transport(PROJECT_DOCS_GXSERVER_ENDPOINT, {
      action: 'list',
      projectId,
      requestId,
    }),
    requestId
  );
  if (!Array.isArray(response.entries)) {
    throw new Error('Docs did not return the project file list.');
  }
  return response.entries.flatMap((entry) => {
    if (!isProjectDocsResponseRecord(entry) || entry.kind !== 'file' || typeof entry.path !== 'string') {
      return [];
    }
    return entry.path.toLowerCase().endsWith('.md') ? [entry.path] : [];
  });
}

/**
 * Saves one Markdown document through the project's existing Docs boundary,
 * then asks the same owning gxserver for its absolute machine path. The client
 * never derives an absolute path from project metadata, which keeps remote and
 * local sessions on the same filesystem authority.
 */
export async function saveProjectMarkdownDocument(
  params: SaveProjectMarkdownDocumentParams,
  transport: ProjectMarkdownSaveTransport
): Promise<{ path: string }> {
  const saveRequestId = createProjectDocsRequestId('save-message-markdown');
  const saved = checkedProjectDocsResponse(
    await transport(PROJECT_DOCS_GXSERVER_ENDPOINT, {
      action: 'save',
      content: params.content,
      path: params.path,
      projectId: params.projectId,
      requestId: saveRequestId,
    }),
    saveRequestId
  );
  if (!isProjectDocsResponseRecord(saved.file)) {
    throw new Error('Docs did not return the saved Markdown file.');
  }

  const pathRequestId = createProjectDocsRequestId('saved-message-path');
  const resolved = checkedProjectDocsResponse(
    await transport(PROJECT_DOCS_GXSERVER_ENDPOINT, {
      action: 'copyFullPath',
      path: params.path,
      projectId: params.projectId,
      requestId: pathRequestId,
    }),
    pathRequestId
  );
  if (typeof resolved.fullPath !== 'string' || resolved.fullPath.length === 0) {
    throw new Error('Docs did not return the saved Markdown path.');
  }
  return { path: resolved.fullPath };
}

/*
 * CDXC:RemoteProjectDocs 2026-08-06:
 * The Docs page owns one request/response contract independent of its host.
 * GPUI supplies the CEF event bridge today; a web client can use the exported
 * gxserver endpoint and the same request/response types without importing the
 * native Manage page or duplicating its timeout/correlation behavior.
 */
export function requestProjectDocsFromHost(
  request: Omit<ProjectDocsRequest, 'requestId'>,
  transport: ProjectDocsHostTransport
): Promise<ProjectDocsResponse> {
  const requestId = createProjectDocsRequestId();
  const message: ProjectDocsRequest = { ...request, requestId };
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      transport.eventTarget.removeEventListener(transport.eventName, handleResponse);
      reject(new Error('Docs request timed out.'));
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

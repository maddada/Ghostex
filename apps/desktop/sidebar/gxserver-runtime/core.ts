import { nativePost } from '@/packages/shared/native-runtime/bridge';
/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { asGpuiSidebarCommand } from './sidebar-command-entry';
import { GpuiGxserverClient } from './client';
import { GPUI_SIDEBAR_REMOTE_EVENT_NAME } from './constants';
import { createEmptyGpuiAppUserData } from './helpers/bootstrap';
import { readStoredGpuiRemoteRecentProjects } from './helpers/recent-projects';
import { normalizeGpuiSidebarRemoteEvent } from './helpers/remote-presentation';
import type { GpuiSidebarRuntimePresentationStreamMethods } from './presentation-stream';
import { gpuiSidebarRuntimePresentationStreamMethods } from './presentation-stream';
import type { GpuiSidebarRuntimeRemoteMachineMethods } from './remote-machines';
import { gpuiSidebarRuntimeRemoteMachineMethods } from './remote-machines';
import type { GpuiSidebarRuntimeSidebarGroupMethods } from './sidebar-groups';
import { gpuiSidebarRuntimeSidebarGroupMethods } from './sidebar-groups';
import type {
  GpuiPendingRemoteGxserverRequest,
  GpuiPresentationSubscription,
  GpuiValidatedGxserverBootstrap,
} from './types-and-protocol';
import type { GpuiSidebarRuntimeWorkspaceGroupMethods } from './workspace-groups-sync';
import { gpuiSidebarRuntimeWorkspaceGroupMethods } from './workspace-groups-sync';
import type { WebviewApi } from '@/packages/core-ui/webview-api';
import type {
  GxserverAppUserData,
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
} from '@/packages/shared/gxserver-protocol';
import type {
  ExtensionToSidebarMessage,
  SidebarGroupsChangedMessage,
  SidebarHudChangedMessage,
  SidebarHydrateMessage,
  SidebarOrderSyncResultMessage,
  SidebarPreviousSessionsResultMessage,
  SidebarToExtensionMessage,
} from '@/packages/shared/session-grid-contract';

/*
CDXC:StateSync 2026-06-24-11:00:
The production GPUI sidebar must mount the shared SidebarApp and hydrate it from gxserver presentation, never Storybook fixtures. Keep the renderer contract narrow: Rust/CEF installs baseUrl, authToken, protocolVersion, and optional active/focus ids on window.ghostexGpui.gxserverBootstrap; this adapter owns HTTP/WebSocket presentation flow, shared reducer/projection, active-project posting, and explicit unsupported handling for sidebar commands outside this slice.

CDXC:Settings 2026-06-24-11:59:
Settings project/worktree metadata in the GPUI SidebarApp still comes from real gxserver project domain rows, but read-side agent/action chrome now comes from `/api/readSidebarHud` so the renderer does not duplicate custom launcher/action normalization. Keep Beads/worktree metadata on project rows and never invent project paths when gxserver omits them.

CDXC:Projects 2026-06-24-14:18:
Reused SidebarApp project path actions in GPUI may send only fixed action names plus trusted gxserver project ids to the sidebar-native bridge. The renderer must never send paths from DOM text, group labels, project titles, or cached project domain rows; Rust resolves ids through gxserver immediately before clipboard/Finder side effects.

CDXC:Projects 2026-06-24-13:49:
Reused SidebarApp IDE-open messages in GPUI use the same pathless native project action bridge. The renderer maps group IDE opens to a Settings-owned fixed action and active workspace IDE opens to fixed VS Code/Zed action names plus gxserver project ids only; targetApp, editor commands, app names, paths, labels, URLs, and shell snippets stay out of the bridge payload so Rust owns editor selection and launch.

CDXC:ServerDaemon 2026-06-24-13:30:
Pinned Prompts in the reused GPUI SidebarApp must hydrate and save through
gxserver app-user-data, matching the app-modal host. Keep prompt bodies inside
authenticated RPC payloads only; do not log them or persist them in a
GPUI-only JSON file.

CDXC:RemoteMachines 2026-06-24-19:06:
Remote terminal focus and copy-attach commands may leave React only as fixed native action names plus machine-scoped remote presentation session ids. Rust owns saved-machine SSH details, gxserver attach/resume metadata, GPUI terminal launch payloads, and clipboard command construction so renderer state never carries tokens, hostnames, paths, or command text.

CDXC:RemoteMachines 2026-08-14:
Remote Recent Projects opens a real project-scoped terminal through the fixed `openRemoteProjectTerminal` selector. React sends only the machine-scoped project id; Rust restores the parked project, creates the remote gxserver terminal, and owns all SSH attach metadata and terminal launch payloads.

CDXC:RemoteMachines 2026-06-24-20:26:
Remote IDE project and changed-file opens are allowed only through Rust-owned fixed editor openers. React may request a fixed action for a machine-scoped project id, but it must never send remote paths, URI strings, SSH host/user/port/identity details, Settings custom commands, or editor command text.

CDXC:RemoteMachines 2026-06-24-21:33:
Zed remote opens are allowed through Rust-owned documented `zed ssh://[user@]host[:port]/path` argv only. React still sends only fixed action names and machine-scoped project ids; Cursor, Windsurf, VSCodium, Sublime, and custom remote editor commands remain unsupported without an equally reviewed native opener contract.

CDXC:FocusRouting 2026-06-24-21:07:
Focused and visible session bootstrap state may use only gxserver presentation session ids the GPUI runtime already owns from create/focus/fork/restore results or machine-scoped remote presentation ids. Local ids stay raw gxserver session ids; remote ids use the existing `remote:<machine>:session:<project>:<session>` convention so React, Rust, and the CEF bootstrap never infer focus from labels, paths, terminal text, project names, or shell placeholder ids.

CDXC:Projects 2026-06-24-22:18:
GPUI must mirror the macOS sidebar projection rules for gxserver project domain metadata and canonical chat-folder paths. Legacy `isChat`/`isQuick`, `launchSettings.isChat`, `launchSettings.isQuick`, and projects under the Ghostex chats roots feed the synthetic Chats group instead of normal Project groups, `isRecentProject` rows stay out of active presentation groups, and automatic fallback focus must choose a visible non-chat project while explicit chat-session focus keeps the Chats group active.

CDXC:Projects 2026-06-24-22:51:
Generated Chat folders must not render as individual GPUI project groups, and clicking a chat session must not publish that chat folder as the active project to Rust. Treat host Ghostex-home chat roots, including dev `.active/chats` homes, as projectless Chats containers before building active-project context, Settings project rows, or Git HUD state.
*/
export function createGpuiSidebarRuntime(): {
  messageSource: GpuiSidebarLocalMessageSource;
  start: () => void;
  vscode: WebviewApi;
} {
  const runtime = new GpuiSidebarRuntime();
  return {
    messageSource: runtime.messageSource,
    start: () => runtime.start(),
    vscode: runtime.vscode,
  };
}

export class GpuiSidebarLocalMessageSource {
  private readonly eventTarget = new EventTarget();

  addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: AddEventListenerOptions | boolean
  ): void {
    this.eventTarget.addEventListener(type, listener, options);
  }

  removeEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: EventListenerOptions | boolean
  ): void {
    this.eventTarget.removeEventListener(type, listener, options);
  }

  postMessage(
    message:
      | ExtensionToSidebarMessage
      | SidebarHydrateMessage
      | SidebarGroupsChangedMessage
      | SidebarHudChangedMessage
      | SidebarOrderSyncResultMessage
      | SidebarPreviousSessionsResultMessage
  ): void {
    this.eventTarget.dispatchEvent(
      new MessageEvent('message', {
        data: message,
      })
    );
  }
}

export class GpuiSidebarRuntime {
  readonly messageSource = new GpuiSidebarLocalMessageSource();
  readonly vscode: WebviewApi = {
    postMessage: (message) => {
      void this.handleSidebarMessage(message);
    },
  };

  appUserData: GxserverAppUserData = createEmptyGpuiAppUserData();
  /**
   * Escalating presentation-stream recovery state. `AcknowledgedAt` is when the
   * daemon last answered a `subscribePresentation` (with a snapshot or with
   * "you are already current"); `Attempt` indexes
   * `GPUI_PRESENTATION_STREAM_RECOVERY_DELAYS_MS`; `TimeoutId` both holds the
   * pending retry and coalesces the `onClose` + `onError` pair a single socket
   * failure produces.
   */
  presentationStreamAcknowledgedAt: number | undefined;
  presentationStreamRecoveryAttempt = 0;
  presentationStreamRecoveryTimeoutId: number | undefined;
  client: GpuiGxserverClient | undefined;
  domainProjects: GxserverProjectDomainState[] = [];
  gxserverBootstrap: GpuiValidatedGxserverBootstrap | undefined;
  hasHydrated = false;
  pendingRemoteGxserverRequests = new Map<string, GpuiPendingRemoteGxserverRequest>();
  presentation: GxserverPresentationSnapshot | undefined;
  recentProjects: GxserverRecentProjectDomainState[] = [];
  remoteGxserverRequestSequence = 0;
  /**
   * Nothing fills this any more: the remote machines' presentations are the Rust store's
   * (apps/desktop/src/app/gx_store/remote_clients.rs). It is read only by the remote Project Group
   * and Space edits below, which wait on the user (CDXC:RemoteMachines 2026-09-21 DECISION in
   * apps/desktop/src/app/gx_store/remote_project_docs.rs) and go with them.
   */
  remotePresentations = new Map<string, GxserverPresentationSnapshot>();
  remoteRecentProjectsByMachineId = new Map<string, GxserverRecentProjectDomainState[]>();
  revision = 0;
  subscription: GpuiPresentationSubscription | undefined;
  lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId = new Map<string, string>();
  lastForwardedRemoteSidebarSpacesJsonByMachineId = new Map<string, string>();

  start(): void {
    this.installGpuiBridgeCallbacks();
    this.remoteRecentProjectsByMachineId = readStoredGpuiRemoteRecentProjects();
    window.addEventListener(GPUI_SIDEBAR_REMOTE_EVENT_NAME, this.handleGpuiSidebarRemoteEvent);
    this.publishUnavailable('bootstrap-pending');
    // `service.ts` installs the bootstrap from the start config before `start()` runs (an empty
    // object when there is none), so there is nothing to poll for.
    const bootstrap = window.ghostexGpui?.gxserverBootstrap;
    if (bootstrap) {
      this.startFromBootstrap(bootstrap);
    }
  }

  installGpuiBridgeCallbacks(): void {
    const gpuiBridge = (window.ghostexGpui = window.ghostexGpui ?? {});
    /*
    CDXC:Sidebar 2026-09-21 WHY:
    The desktop sidebar is the Rust store's, and the page that used to receive its commands and
    forward them here is deleted. What the store cannot perform itself, because this runtime still
    owns it (a remote machine's project collections and Spaces), arrives on this one entry instead
    and goes straight to the same handler the page's forward ended in.
    */
    gpuiBridge.onSidebarCommand = (payload) => {
      const message = asGpuiSidebarCommand(payload);
      if (message) void this.handleSidebarMessage(message);
    };
    const pendingSidebarCommands = Array.isArray(gpuiBridge.pendingSidebarCommands)
      ? gpuiBridge.pendingSidebarCommands.splice(0)
      : [];
    for (const payload of pendingSidebarCommands) {
      const message = asGpuiSidebarCommand(payload);
      if (message) void this.handleSidebarMessage(message);
    }
    gpuiBridge.onGxserverBootstrapChanged = (bootstrap) => {
      this.applyGxserverBootstrapChanged(bootstrap);
    };
  }

  /**
   * CDXC:RemoteMachines 2026-09-25 WHY:
   * Only the answers to this runtime's own remote requests arrive here now. The machines' status
   * and presentations are the Rust store's (apps/desktop/src/app/gx_store/remote_clients.rs), so
   * Rust no longer sends them to this runtime.
   */
  readonly handleGpuiSidebarRemoteEvent = (event: Event): void => {
    const remoteEvent = normalizeGpuiSidebarRemoteEvent((event as CustomEvent<unknown>).detail);
    if (remoteEvent?.type === 'remoteGxserverResponse') {
      this.resolveRemoteGxserverRequest(remoteEvent);
    }
  };

  async handleSidebarMessage(message: SidebarToExtensionMessage): Promise<void> {
    /*
     * CDXC:Diagnostics 2026-09-25 WHY:
     * The app runtime port's meter counts every handler call here, whichever of the four doors
     * delivered it (onSidebarCommand, onSidebarHostMessage, the Git modal commands, Quick Access).
     * The service drops this message unless the `native.runtime.trace` scenario armed it.
     */
    nativePost({ kind: 'traceEntry', name: `handleSidebarMessage:${message.type}` });
    switch (message.type) {
      /*
      CDXC:Projects 2026-09-21 WHY:
      REMOTE only. This computer's copies of both documents are written and pushed by Rust
      (apps/desktop/src/app/gx_store/project_docs.rs), which is why the local arms and the
      debounced write-through behind them were deleted on 2026-09-21: nothing has posted either
      message without a `remoteMachineId` since M4d part 2 blocker 3, and a queue nothing fills
      would be a second writer waiting to happen.
      */
      case 'updateSidebarProjectCollections':
        if (message.remoteMachineId) {
          await this.updateRemoteSidebarProjectCollections(message.remoteMachineId, message.state);
        }
        return;
      case 'updateSidebarSpaces':
        if (message.remoteMachineId) {
          await this.updateRemoteSidebarSpaces(message.remoteMachineId, message.state);
        }
        return;
      default:
        this.handleUnsupportedSidebarMessage(message);
        return;
    }
  }

  handleUnsupportedSidebarMessage(_message: SidebarToExtensionMessage): void {
    /*
    CDXC:StateSync 2026-06-24-11:00:
    GPUI command parity is intentionally incremental. Unsupported SidebarApp messages must be explicit no-ops in this adapter instead of mutating fixture state, inventing host behavior, logging user content, or pretending native-only Browser/Git/settings/chrome actions succeeded.
    */
  }
}

/*
CDXC:RepoStructure 2026-08-22:
`GpuiSidebarRuntime` is one object with one lifetime; the split only moved its
method bodies into per-responsibility modules. Each of those modules exports a
plain object of methods declared with an explicit `this: GpuiSidebarRuntime`,
and they are copied onto the prototype here. `Object.defineProperty` (rather
than `Object.assign`) is used so the copied methods keep the exact property
attributes a `class` body would have given them: writable, configurable, and
NOT enumerable.

The declaration merge below is what makes `this.someGitMethod()` resolve from
inside `sessions-and-focus.ts` and vice versa: every module sees the whole
merged interface, so the mutual recursion the original class relied on still
type-checks. It works without a circular-inference error only because every
moved method carries an explicit return type annotation.
*/
export interface GpuiSidebarRuntime
  extends
    GpuiSidebarRuntimeSidebarGroupMethods,
    GpuiSidebarRuntimePresentationStreamMethods,
    GpuiSidebarRuntimeWorkspaceGroupMethods,
    GpuiSidebarRuntimeRemoteMachineMethods {}

function installGpuiSidebarRuntimeMethods(methods: Record<string, unknown>): void {
  for (const [name, value] of Object.entries(methods)) {
    Object.defineProperty(GpuiSidebarRuntime.prototype, name, {
      value,
      writable: true,
      enumerable: false,
      configurable: true,
    });
  }
}

installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeSidebarGroupMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimePresentationStreamMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeWorkspaceGroupMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeRemoteMachineMethods);

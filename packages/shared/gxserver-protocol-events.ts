import type { GxserverCloudBoxesChangedEvent } from './gxserver-protocol-cloud-boxes';
import type {
  GxserverSessionChatAppendedEvent,
  GxserverSessionChatReplacedEvent,
  GxserverSessionChatSnapshotEvent,
  GxserverSessionChatStateEvent,
} from './session-chat';
import type {
  GxserverProtocolVersion,
  GxserverServerId,
  GxserverEndpointPath,
  GxserverRendererCommand,
} from './gxserver-protocol-core';
import type {
  GxserverPresentationRevision,
  GxserverSidebarProjectCollectionsState,
  GxserverSidebarSpacesState,
  GxserverSidebarWorkspacesState,
  GxserverWorkspaceSessionGroupsState,
  GxserverCustomSessionTagsState,
  GxserverPresentationSnapshot,
  GxserverPresentationDelta,
} from './gxserver-protocol-presentation';

export type GxserverEvent =
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'eventStreamReady';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'serverStarted';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'serverStopping';
    }
  | {
      path: GxserverEndpointPath;
      protocolVersion: GxserverProtocolVersion;
      requestId: string;
      serverId: GxserverServerId;
      type: 'apiRequestHandled';
    }
  | {
      clientId?: string;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      snapshot: GxserverPresentationSnapshot;
      type: 'presentationSnapshot';
    }
  /*
   * CDXC:StateSync 2026-09-01:
   * The reply to a `subscribePresentation` whose `lastRevision` already names
   * the daemon's current revision. It carries no snapshot on purpose: the
   * subscriber has missed nothing, so re-sending the projection it just applied
   * over HTTP would be pure duplicate work on every reconnect. Any other
   * `lastRevision` — missing, behind (deltas were missed), or ahead — still
   * gets the full `presentationSnapshot`.
   */
  | {
      clientId?: string;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'presentationSnapshotCurrent';
    }
  | {
      delta: GxserverPresentationDelta;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'presentationDelta';
    }
  | {
      command: GxserverRendererCommand;
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'rendererCommand';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      sidebarProjectCollections: GxserverSidebarProjectCollectionsState;
      type: 'sidebarProjectCollectionsChanged';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      sidebarSpaces: GxserverSidebarSpacesState;
      type: 'sidebarSpacesChanged';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      sidebarWorkspaces: GxserverSidebarWorkspacesState;
      type: 'sidebarWorkspacesChanged';
    }
  | {
      customSessionTags: GxserverCustomSessionTagsState;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'customSessionTagsChanged';
    }
  | {
      groups: GxserverWorkspaceSessionGroupsState;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'workspaceGroupsChanged';
    }
  /*
   * CDXC:AgentLauncher 2026-08-07:
   * A Global Action write is not a project write, so it produces no
   * presentation delta and live surfaces would otherwise keep a stale list.
   * The event announces the change and bumps the presentation revision; it
   * carries no commands, because `/api/readSidebarHud` stays the single
   * projection of the Global Actions list.
   */
  | {
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'globalSidebarCommandsChanged';
    }
  /*
   * CDXC:Notifications 2026-09-11:
   * Announces that the notification feed changed (a new row, a read-state
   * change, a dismissal). It carries no rows and no presentation revision:
   * `/api/readNotificationFeed` stays the single projection, so clients refetch.
   */
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'notificationFeedChanged';
    }
  /*
   * Announces that a workspace's connectors changed (a sign-in, an app,
   * Executor's state, the workspace's rules). It carries only the workspace:
   * `/api/readConnectors` stays the single projection, so Settings refetches.
   */
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'connectorsChanged';
      workspaceId?: string;
    }
  /*
   * CDXC:CloudBoxes 2026-11-02 SEE-ALSO:
   * The Cloud Boxes registry changed (gxserver-protocol-cloud-boxes.ts).
   */
  | GxserverCloudBoxesChangedEvent
  /*
   * CDXC:AgentProviders 2026-09-22:
   * The published agent model catalog, pushed when gxserver's poll of GitHub
   * finds a change and once to each socket as it connects (chat-only sockets
   * included). `catalog` is the whole document; clients validate it with
   * `parseAgentModelCatalog` and keep the newer of it and what they hold.
   */
  | {
      catalog: unknown;
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'agentModelCatalogChanged';
    }
  | GxserverSessionChatSnapshotEvent
  | GxserverSessionChatAppendedEvent
  | GxserverSessionChatReplacedEvent
  | GxserverSessionChatStateEvent;

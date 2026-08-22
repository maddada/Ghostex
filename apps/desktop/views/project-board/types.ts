import {
  type BeadsBridgeRequest,
  type BoardStatusKey,
  type BoardTicket,
  type TshirtSize,
} from "../project-board-shared";
import { type ProjectBoardBridgeRequest } from "@/packages/shared/bead-conversation-links";

export type ConversationActionState =
  | { kind: "associate"; beadId: string }
  | { kind: "jump"; linkId: string }
  | { kind: "start"; beadId: string }
  | { kind: "unlink"; linkId: string }
  | undefined;

export type DetailDraft = {
  blockedByIds: string[];
  blockingIds: string[];
  comment: string;
  description: string;
  isDeleting: boolean;
  isSaving: boolean;
  labels: string[];
  priority: string;
  status: BoardStatusKey;
  title: string;
  tshirt?: TshirtSize;
  ticket?: BoardTicket;
};

export type TicketFormDraft = {
  blockedByIds: string[];
  blockingIds: string[];
  description: string;
  labels: string[];
  priority: string;
  status: BoardStatusKey;
  title: string;
  tshirt?: TshirtSize;
};

export type PendingBoardStatusMove = {
  beadsStatus: string;
  statusKey: BoardStatusKey;
  token: number;
};

export type ProjectBoardFocusOwnerEvent = "focusin" | "keydown" | "pointerdown";

export type ProjectBoardImageBridgeRequest = {
  action: "loadPreview" | "pasteImage";
  path?: string;
  requestId: string;
};

export type ProjectBoardImageBridgeResponse = {
  dataUrl?: string;
  error?: string;
  imagePath?: string;
  path?: string;
  requestId: string;
};

export type ProjectBeadsWebKitWindow = Window & {
  webkit?: {
    messageHandlers?: {
      ghostexProjectBeads?: {
        postMessage: (message: BeadsBridgeRequest) => void;
      };
      ghostexProjectBoard?: {
        postMessage: (message: ProjectBoardBridgeRequest) => void;
      };
      ghostexProjectBoardImages?: {
        postMessage: (message: ProjectBoardImageBridgeRequest) => void;
      };
    };
  };
};

export type BoardRefreshMode = "background" | "initial" | "manual" | "mutation";

export type BoardRefreshOptions = {
  mode?: BoardRefreshMode;
};

export type ProjectBoardCommandCompletedEventDetail = {
  action?: string;
  exitCode?: number;
};

export type ProjectBoardRunnableCommandAction =
  | "initializeBeads"
  | "installOrUpdateBeads"
  | "runBeadsMigration";

export type RunnableBeadsMigrationOption =
  | "migrate"
  | "adopt"
  | "adopt-fast-forward"
  | "reconcile-fork";

export function projectBoardCommandRunKey(
  action: ProjectBoardRunnableCommandAction,
  migrationOption?: RunnableBeadsMigrationOption,
): string {
  return migrationOption ? `${action}:${migrationOption}` : action;
}

export type ProjectSurfaceTab = "triage" | "automations" | "runs" | "board";

export type TicketContextMenuState = {
  confirmingDelete: boolean;
  ticketId: string;
  x: number;
  y: number;
};
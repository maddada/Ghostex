import { DragDropProvider, type DragDropEventHandlers } from "@dnd-kit/react";
import { isSortableOperation, useSortable } from "@dnd-kit/react/sortable";
import { IconCodeDots, IconGripVertical, IconPencil, IconPlus, IconTrash } from "@tabler/icons-react";
import { createPortal } from "react-dom";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/packages/components/ui/button";
import type { SidebarAgentButton } from "../shared/sidebar-agents";
import { AgentConfigModal, type AgentConfigDraft } from "./agent-config-modal";
import { getBrandAgentLogoStyle } from "./agent-logos";
import { useSidebarStore } from "./sidebar-store";
import type { WebviewApi } from "./webview-api";

type AgentEditorState = {
  draft: AgentConfigDraft;
};

type AgentDragData = {
  agentId: string;
  kind: "configure-agent";
};

export type ConfigureAgentsModalProps = {
  isOpen: boolean;
  onClose: () => void;
  vscode: WebviewApi;
};

/**
 * CDXC:SidebarAgents 2026-05-08-09:00
 * The sidebar-reference import needs a dedicated Configure Agents surface so
 * reference-mode project launchers can add, edit, delete, and reorder agents
 * without exposing the older Agents panel grid.
 */
export function ConfigureAgentsModal({ isOpen, onClose, vscode }: ConfigureAgentsModalProps) {
  const theme = useSidebarStore((state) => state.hud.theme);
  const agents = useSidebarStore((state) => state.hud.agents);
  const [editorState, setEditorState] = useState<AgentEditorState>();
  const [draftAgentIds, setDraftAgentIds] = useState<string[]>();

  useEffect(() => {
    setDraftAgentIds((previousDraft) => reconcileDraftAgentIds(previousDraft, agents));
  }, [agents]);

  useEffect(() => {
    if (!isOpen) {
      setEditorState(undefined);
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && editorState === undefined) {
        onClose();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [editorState, isOpen, onClose]);

  const openCreateAgentEditor = () => {
    setEditorState({
      draft: {
        command: "",
        name: "",
      },
    });
  };

  const openAgentEditor = (agent: SidebarAgentButton) => {
    setEditorState({
      draft: {
        acceptAllMode: agent.acceptAllMode ?? "inherit",
        agentId: agent.agentId,
        command: agent.command ?? "",
        icon: agent.icon,
        name: agent.name,
      },
    });
  };

  const saveAgent = (draft: AgentConfigDraft) => {
    vscode.postMessage({
      acceptAllMode: draft.acceptAllMode,
      agentId: draft.agentId,
      command: draft.command,
      icon: draft.icon,
      name: draft.name,
      type: "saveSidebarAgent",
    });
    setEditorState(undefined);
  };

  const deleteAgent = (agent: SidebarAgentButton) => {
    vscode.postMessage({
      agentId: agent.agentId,
      type: "deleteSidebarAgent",
    });
  };

  const orderedAgents = useMemo(() => {
    const agentById = new Map(agents.map((agent) => [agent.agentId, agent]));
    const orderedAgentIds = draftAgentIds
      ? mergeAgentIds(
          draftAgentIds,
          agents.map((agent) => agent.agentId),
        )
      : agents.map((agent) => agent.agentId);

    return orderedAgentIds
      .map((agentId) => agentById.get(agentId))
      .filter((agent): agent is SidebarAgentButton => agent !== undefined);
  }, [agents, draftAgentIds]);

  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }

    const { source, target } = event.operation;
    const sourceData = source ? getAgentDragData(source) : undefined;
    if (!source || !sourceData) {
      return;
    }

    const targetIndex =
      "index" in source && typeof source.index === "number" ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }

    const nextAgentIds = moveAgentId(
      orderedAgents.map((agent) => agent.agentId),
      source.initialIndex,
      targetIndex,
    );
    setDraftAgentIds(nextAgentIds);
    vscode.postMessage({
      agentIds: nextAgentIds,
      requestId: createReorderRequestId(),
      type: "syncSidebarAgentOrder",
    });
  }) satisfies DragDropEventHandlers["onDragEnd"];

  if (!isOpen) {
    return null;
  }

  const isEditorOpen = editorState !== undefined;

  /**
   * CDXC:SidebarAgents 2026-05-08-11:18
   * Configure Agents and Configure agent are one edit flow. Hide the management
   * modal while the agent editor is open so users never see stacked configure
   * dialogs when creating or editing an agent.
   */
  return createPortal(
    <>
      {!isEditorOpen ? (
        <div className="confirm-modal-root scroll-mask-y" role="presentation">
          <button className="confirm-modal-backdrop" onClick={onClose} type="button" />
          <div
            aria-label="Configure Agents"
            aria-modal="true"
            className="confirm-modal configure-actions-modal"
            data-sidebar-theme={theme}
            role="dialog"
          >
            <div className="confirm-modal-header">
              <div className="confirm-modal-title">Configure Agents</div>
            </div>
            <div className="configure-actions-list scroll-mask-y">
              {orderedAgents.length > 0 ? (
                <DragDropProvider onDragEnd={handleDragEnd}>
                  {orderedAgents.map((agent, index) => (
                    <SortableConfigureAgentRow
                      agent={agent}
                      index={index}
                      key={agent.agentId}
                      onDelete={() => deleteAgent(agent)}
                      onEdit={() => openAgentEditor(agent)}
                    />
                  ))}
                </DragDropProvider>
              ) : (
                <div className="configure-actions-empty-state">No agents configured.</div>
              )}
            </div>
            <div className="configure-agents-footer">
              <Button onClick={openCreateAgentEditor} type="button" variant="outline">
                <IconPlus aria-hidden="true" data-icon="inline-start" />
                Add Agent
              </Button>
            </div>
          </div>
        </div>
      ) : null}
      <AgentConfigModal
        draft={editorState?.draft ?? { command: "", name: "" }}
        isOpen={isEditorOpen}
        onCancel={() => setEditorState(undefined)}
        onSave={saveAgent}
        theme={theme}
      />
    </>,
    document.body,
  );
}

function SortableConfigureAgentRow({
  agent,
  index,
  onDelete,
  onEdit,
}: {
  agent: SidebarAgentButton;
  index: number;
  onDelete: () => void;
  onEdit: () => void;
}) {
  const sortable = useSortable({
    accept: "configure-agent",
    data: createAgentDragData(agent.agentId),
    group: "configure-agents",
    id: agent.agentId,
    index,
    type: "configure-agent",
  });

  const setRowRef = (element: HTMLDivElement | null) => {
    sortable.ref(element);
    sortable.sourceRef(element);
  };

  return (
    <div
      className="configure-actions-list-item configure-agents-list-item"
      data-dragging={String(Boolean(sortable.isDragging))}
      ref={setRowRef}
    >
      <span aria-hidden="true" className="configure-agents-row-grip" ref={sortable.handleRef}>
        <IconGripVertical size={16} stroke={1.8} />
      </span>
      <button className="configure-agents-row-main" onClick={onEdit} type="button">
        <span aria-hidden="true" className="configure-actions-list-icon">
          <ConfigureAgentIcon agent={agent} />
        </span>
        <span className="configure-actions-list-copy">
          <span className="configure-actions-list-title">{agent.name}</span>
          <span className="configure-actions-list-meta">
            {agent.command?.trim() || "Not configured"}
          </span>
        </span>
      </button>
      <span className="configure-agents-row-actions">
        <Button aria-label={`Edit ${agent.name}`} onClick={onEdit} size="icon-sm" type="button" variant="ghost">
          <IconPencil aria-hidden="true" />
        </Button>
        <Button
          aria-label={`Delete ${agent.name}`}
          onClick={onDelete}
          size="icon-sm"
          type="button"
          variant="destructive"
        >
          <IconTrash aria-hidden="true" />
        </Button>
      </span>
    </div>
  );
}

function ConfigureAgentIcon({ agent }: { agent: SidebarAgentButton }) {
  if (agent.icon) {
    return (
      <span
        aria-hidden="true"
        className="configure-agents-list-agent-icon"
        style={getBrandAgentLogoStyle(agent.icon)}
      />
    );
  }

  return <IconCodeDots aria-hidden="true" size={16} stroke={1.8} />;
}

function createAgentDragData(agentId: string): AgentDragData {
  return {
    agentId,
    kind: "configure-agent",
  };
}

function getAgentDragData(candidate: unknown): AgentDragData | undefined {
  if (!hasData(candidate)) {
    return undefined;
  }

  const data = candidate.data;
  if (!isObjectRecord(data) || data.kind !== "configure-agent" || typeof data.agentId !== "string") {
    return undefined;
  }

  return {
    agentId: data.agentId,
    kind: "configure-agent",
  };
}

function moveAgentId(agentIds: readonly string[], initialIndex: number, index: number): string[] {
  const nextAgentIds = [...agentIds];
  const [agentId] = nextAgentIds.splice(initialIndex, 1);
  if (agentId === undefined) {
    return nextAgentIds;
  }

  nextAgentIds.splice(index, 0, agentId);
  return nextAgentIds;
}

function mergeAgentIds(
  draftAgentIds: readonly string[],
  syncedAgentIds: readonly string[],
): string[] {
  const syncedAgentIdSet = new Set(syncedAgentIds);
  const mergedAgentIds = draftAgentIds.filter((agentId) => syncedAgentIdSet.has(agentId));

  for (const agentId of syncedAgentIds) {
    if (!mergedAgentIds.includes(agentId)) {
      mergedAgentIds.push(agentId);
    }
  }

  return mergedAgentIds;
}

function reconcileDraftAgentIds(
  draftAgentIds: readonly string[] | undefined,
  agents: readonly SidebarAgentButton[],
): string[] | undefined {
  if (!draftAgentIds) {
    return undefined;
  }

  const syncedAgentIds = agents.map((agent) => agent.agentId);
  const nextDraftAgentIds = mergeAgentIds(draftAgentIds, syncedAgentIds);
  return haveSameAgentOrder(nextDraftAgentIds, syncedAgentIds) ? undefined : nextDraftAgentIds;
}

function haveSameAgentOrder(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((agentId, index) => agentId === right[index]);
}

function createReorderRequestId(): string {
  return `configure-agents-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function hasData(candidate: unknown): candidate is { data?: unknown } {
  return isObjectRecord(candidate) && "data" in candidate;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

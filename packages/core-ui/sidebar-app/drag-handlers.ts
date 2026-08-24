import { move } from '@dnd-kit/helpers';
import type { DragDropEventHandlers } from '@dnd-kit/react';
import { useCallback, useEffectEvent, type Dispatch, type RefObject, type SetStateAction } from 'react';
import type { DiagnosticLoggingScenarioId, RemoteMachineSettings } from '../../shared/ghostex-settings';
import {
  moveSidebarV2GroupRows,
  projectSidebarV2GroupOrderByMachine,
  type SidebarV2GroupOrderRow,
} from '../../shared/sidebar-v2-group-order';
import { setSidebarTooltipsSuppressedForDrag } from '../app-tooltip';
import {
  getClientPoint,
  getSidebarDropData,
  moveSessionIdsByDropTarget,
  type SidebarGroupDropTarget,
  type SidebarSessionDropTarget,
} from '../sidebar-dnd';
import {
  moveProjectsToSidebarCollection,
  reorderSidebarProjectCollectionDefinitions,
  reorderSidebarProjectCollections,
  type SidebarProjectCollectionsState,
} from '../project-collections';
import type { WebviewApi } from '../webview-api';
import {
  areSameGroupDropTarget,
  areSameRemoteMachineDropTarget,
  areSameSessionDropTarget,
  createPinnedSessionDropResolutionDebugState,
  createPinnedSessionDropTargetLogKey,
  createPinnedSessionReorderDebugState,
  createProjectGroupOrderItems,
  createRemoteProjectListScopeId,
  createSessionPointerDragState,
  getDragNativeEvent,
  getProjectCollectionDragMetrics,
  getProjectCollectionFamilyProjectIds,
  getProjectGroupDragHeaderMetrics,
  getRemoteMachineDragHeaderMetrics,
  LOCAL_PROJECT_LIST_SCOPE_ID,
  moveCollectionIdToDropTarget,
  moveGroupIdsByProjectDropTarget,
  movePinnedSessionIdsByDropTarget,
  moveProjectGroupFamilyToEnd,
  resolveGroupDropTargetFromPoint,
  resolvePinnedSessionDropTargetFromPoint,
  resolveProjectCollectionDropTargetFromPoint,
  resolveProjectUngroupDropScopeFromPoint,
  resolveRemoteMachineDropTargetFromPoint,
  resolveSessionDropTargetFromPoint,
  updateGroupDragPreviewFromEvent,
  updateSessionPointerDragState,
  type SidebarPointerDownSessionTarget,
  type SidebarProjectCollectionDropTarget,
  type SidebarRemoteMachineDropTarget,
  type SidebarSessionPointerDragState,
} from './drag-drop-geometry';
import type {
  SidebarGroupDragPreview,
  SidebarProjectCollectionDragPreview,
  SidebarRemoteMachineDragPreview,
} from './drag-ghosts';
import {
  createPinnedFirstSessionOrder,
  findSessionGroupId,
  haveSameSessionOrder,
  haveSameSessionSet,
} from './session-ordering';
import type {
  SessionIdsByGroup,
  SidebarGroupsById,
  SidebarProjectCollectionRenderItem,
  SidebarSessionsById,
} from './types';

export type SidebarDragHandlersOptions = {
  authoritativeSessionIdsByGroup: SessionIdsByGroup;
  collapsedGroupsById: Record<string, true>;
  collapsedRemoteMachineSectionsById: Record<string, true>;
  displayedProjectCollectionItems: readonly SidebarProjectCollectionRenderItem[];
  effectiveSessionIdsByGroup: SessionIdsByGroup;
  enableProjectCollections: boolean;
  groupIdsRef: RefObject<string[]>;
  groupsById: SidebarGroupsById;
  isManualActiveSessionsSort: boolean;
  isSidebarV2GroupedActive: boolean;
  moveRemoteMachineSection: (remoteMachineId: string, target: SidebarRemoteMachineDropTarget) => void;
  pinnedSessionDropTargetLogKeyRef: RefObject<string | undefined>;
  pointerDownSessionTargetRef: RefObject<SidebarPointerDownSessionTarget | undefined>;
  postPinnedSessionReorderLog: (event: string, details: unknown) => void;
  postSidebarDebugLog: (scenarioId: DiagnosticLoggingScenarioId, event: string, details: unknown) => void;
  projectCollectionIdByProjectId: ReadonlyMap<string, string>;
  projectCollections: SidebarProjectCollectionsState;
  remoteMachines: readonly RemoteMachineSettings[];
  remoteProjectGroupIdsByMachineId: Record<string, string[]>;
  sessionIdsByGroupRef: RefObject<SessionIdsByGroup>;
  sessionPointerDragStateRef: RefObject<SidebarSessionPointerDragState | undefined>;
  sessionsById: SidebarSessionsById;
  setGroupDragPreview: Dispatch<SetStateAction<SidebarGroupDragPreview | undefined>>;
  setGroupDropIndicator: Dispatch<SetStateAction<SidebarGroupDropTarget | undefined>>;
  setIsProjectReorderDragActive: Dispatch<SetStateAction<boolean>>;
  setPinnedSessionDropIndicator: Dispatch<SetStateAction<SidebarSessionDropTarget | undefined>>;
  setProjectCollectionDragPreview: Dispatch<SetStateAction<SidebarProjectCollectionDragPreview | undefined>>;
  setProjectCollectionDropIndicator: Dispatch<SetStateAction<SidebarProjectCollectionDropTarget | undefined>>;
  setProjectCollections: Dispatch<SetStateAction<SidebarProjectCollectionsState>>;
  setProjectUngroupDropIndicatorScopeId: Dispatch<SetStateAction<string | undefined>>;
  setRemoteMachineDragPreview: Dispatch<SetStateAction<SidebarRemoteMachineDragPreview | undefined>>;
  setRemoteMachineDropIndicator: Dispatch<SetStateAction<SidebarRemoteMachineDropTarget | undefined>>;
  setSessionDropIndicator: Dispatch<SetStateAction<SidebarSessionDropTarget | undefined>>;
  sidebarV2GroupIdsByMachineId: Record<string, string[]>;
  sidebarV2GroupOrderRowsRef: RefObject<readonly SidebarV2GroupOrderRow[]>;
  vscode: WebviewApi;
};

/*
 * CDXC:SidebarHookDecomposition 2026-08-22:
 * Everything the sidebar's dnd-kit provider needs: the drop-candidate scoping
 * helpers, the live drop-indicator resolver, and the four drag lifecycle
 * handlers. They were one contiguous block in SidebarApp and share the same
 * refs and drop state, so they move together rather than concern-by-concern.
 */
export function useSidebarDragHandlers({
  authoritativeSessionIdsByGroup,
  collapsedGroupsById,
  collapsedRemoteMachineSectionsById,
  displayedProjectCollectionItems,
  effectiveSessionIdsByGroup,
  enableProjectCollections,
  groupIdsRef,
  groupsById,
  isManualActiveSessionsSort,
  isSidebarV2GroupedActive,
  moveRemoteMachineSection,
  pinnedSessionDropTargetLogKeyRef,
  pointerDownSessionTargetRef,
  postPinnedSessionReorderLog,
  postSidebarDebugLog,
  projectCollectionIdByProjectId,
  projectCollections,
  remoteMachines,
  remoteProjectGroupIdsByMachineId,
  sessionIdsByGroupRef,
  sessionPointerDragStateRef,
  sessionsById,
  setGroupDragPreview,
  setGroupDropIndicator,
  setIsProjectReorderDragActive,
  setPinnedSessionDropIndicator,
  setProjectCollectionDragPreview,
  setProjectCollectionDropIndicator,
  setProjectCollections,
  setProjectUngroupDropIndicatorScopeId,
  setRemoteMachineDragPreview,
  setRemoteMachineDropIndicator,
  setSessionDropIndicator,
  sidebarV2GroupIdsByMachineId,
  sidebarV2GroupOrderRowsRef,
  vscode,
}: SidebarDragHandlersOptions) {
  /*
   * CDXC:RemoteGroupReorder 2026-07-12:
   * Remote machine project groups reorder among their own machine's rows only.
   * Resolve drag candidates from the source group's scope so a remote drag
   * cannot target local Projects rows (and vice versa), while local project
   * drags keep using the collection-ordered id list.
   */
  const groupDragCandidateIdsForSource = (sourceGroupId: string): readonly string[] => {
    /*
     * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
     * Grouped V2 does not have per-machine sections to scope a drag to. Its rows
     * ARE logical projects that may span machines, so every rendered row is a
     * candidate and the machine split moves to the other end of the operation:
     * the drop is projected back onto each machine's own list on release.
     */
    if (isSidebarV2GroupedActive) {
      return sidebarV2GroupOrderRowsRef.current.map((row) => row.groupId);
    }
    const machineId = groupsById[sourceGroupId]?.remoteMachineContext?.machineId;
    if (machineId) {
      return remoteProjectGroupIdsByMachineId[machineId] ?? [];
    }
    return groupIdsRef.current;
  };

  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Written from V2's own render, read only inside a drag. It is a ref rather
   * than state on purpose: the rendered row list changes on every session
   * update, and mirroring it into state would re-render the whole sidebar for
   * information nothing paints. The identity is stable so V2's reporting effect
   * fires on row changes, not on every SidebarApp render.
   */
  const setSidebarV2GroupOrderRows = useCallback((rows: readonly SidebarV2GroupOrderRow[]) => {
    sidebarV2GroupOrderRowsRef.current = rows;
  }, []);

  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Grouped V2's no-op answer, supplied to the shared pointer resolver so the
   * drop line appears exactly where a release would actually reorder something.
   */
  const sidebarV2GroupNoOpTargetForSource = (sourceGroupId: string) =>
    isSidebarV2GroupedActive
      ? (target: SidebarGroupDropTarget) =>
          moveSidebarV2GroupRows(sidebarV2GroupOrderRowsRef.current, sourceGroupId, target) === undefined
      : undefined;

  const updateSessionDropIndicator = useEffectEvent(
    (event: Parameters<NonNullable<DragDropEventHandlers['onDragOver']>>[0]) => {
      const sourceData = getSidebarDropData(event.operation.source);
      if (sourceData?.kind === 'remote-machine') {
        setGroupDropIndicator(undefined);
        setPinnedSessionDropIndicator(undefined);
        setProjectCollectionDropIndicator(undefined);
        setProjectUngroupDropIndicatorScopeId(undefined);
        setSessionDropIndicator(undefined);
        const resolvedRemoteMachineDropTarget = resolveRemoteMachineDropTargetFromPoint(
          getDragNativeEvent(event),
          remoteMachines.map((machine) => machine.id),
          sourceData.remoteMachineId,
          getSidebarDropData(event.operation.target)
        );
        setRemoteMachineDropIndicator((previous) =>
          areSameRemoteMachineDropTarget(previous, resolvedRemoteMachineDropTarget)
            ? previous
            : resolvedRemoteMachineDropTarget
        );
        return;
      }

      setRemoteMachineDropIndicator(undefined);
      if (sourceData?.kind === 'group') {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        const nativeEvent = getDragNativeEvent(event);
        const sourceProjectId = groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const resolvedUngroupDropScopeId =
          sourceProjectId && projectCollectionIdByProjectId.has(sourceProjectId)
            ? resolveProjectUngroupDropScopeFromPoint(nativeEvent, sourceData.groupId, groupsById)
            : undefined;
        const resolvedGroupDropTarget = resolvedUngroupDropScopeId
          ? undefined
          : resolveGroupDropTargetFromPoint(
              nativeEvent,
              groupDragCandidateIdsForSource(sourceData.groupId),
              groupsById,
              getSidebarDropData(event.operation.target),
              sourceData,
              sidebarV2GroupNoOpTargetForSource(sourceData.groupId)
            );
        setProjectUngroupDropIndicatorScopeId((previous) =>
          previous === resolvedUngroupDropScopeId ? previous : resolvedUngroupDropScopeId
        );
        setGroupDropIndicator((previous) =>
          areSameGroupDropTarget(previous, resolvedGroupDropTarget) ? previous : resolvedGroupDropTarget
        );
        return;
      }

      setGroupDropIndicator(undefined);
      setProjectUngroupDropIndicatorScopeId(undefined);
      if (sourceData?.kind === 'project-collection') {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        const resolvedCollectionDropTarget = resolveProjectCollectionDropTargetFromPoint(
          getDragNativeEvent(event),
          displayedProjectCollectionItems.flatMap((item) =>
            item.kind === 'collection' ? [item.collection.collectionId] : []
          ),
          sourceData.collectionId,
          getSidebarDropData(event.operation.target)
        );
        setProjectCollectionDropIndicator((previous) =>
          previous?.collectionId === resolvedCollectionDropTarget?.collectionId &&
          previous?.position === resolvedCollectionDropTarget?.position
            ? previous
            : resolvedCollectionDropTarget
        );
        return;
      }

      setProjectCollectionDropIndicator(undefined);
      if (sourceData?.kind !== 'session') {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        return;
      }

      if (sessionsById[sourceData.sessionId]?.isPinned === true) {
        setSessionDropIndicator(undefined);
        const resolvedPinnedSessionDropTarget = resolvePinnedSessionDropTargetFromPoint(
          getDragNativeEvent(event),
          sourceData,
          sessionIdsByGroupRef.current,
          sessionsById
        );
        const pinnedTargetLogKey = createPinnedSessionDropTargetLogKey(sourceData, resolvedPinnedSessionDropTarget);
        if (pinnedSessionDropTargetLogKeyRef.current !== pinnedTargetLogKey) {
          pinnedSessionDropTargetLogKeyRef.current = pinnedTargetLogKey;
          postPinnedSessionReorderLog('targetChanged', {
            point: getClientPoint(getDragNativeEvent(event)),
            resolvedPinnedSessionDropTarget,
            sourceData,
            state: createPinnedSessionReorderDebugState(
              sourceData,
              sessionIdsByGroupRef.current,
              effectiveSessionIdsByGroup,
              authoritativeSessionIdsByGroup,
              sessionsById
            ),
          });
        }
        setPinnedSessionDropIndicator((previous) =>
          areSameSessionDropTarget(previous, resolvedPinnedSessionDropTarget)
            ? previous
            : resolvedPinnedSessionDropTarget
        );
        return;
      }

      setPinnedSessionDropIndicator(undefined);
      const resolvedSessionDropTarget = resolveSessionDropTargetFromPoint(
        getDragNativeEvent(event),
        sessionIdsByGroupRef.current,
        getSidebarDropData(event.operation.target),
        sourceData
      );

      /*
       * CDXC:SidebarDragDrop 2026-06-19-11:12:
       * Manual session sorting should always show an insertion line while the
       * pointer is over another session row: above the row midpoint means
       * before, below the midpoint means after. Store the resolved drop target
       * directly instead of only highlighting a target project so the visual
       * indicator does not disappear when dnd-kit reports the broader group.
       */
      setSessionDropIndicator((previous) =>
        areSameSessionDropTarget(previous, resolvedSessionDropTarget ?? undefined)
          ? previous
          : (resolvedSessionDropTarget ?? undefined)
      );
    }
  );

  const handleDragStart = ((event) => {
    setSidebarTooltipsSuppressedForDrag(true);
    const nativeEvent = getDragNativeEvent(event);
    const sourceData = getSidebarDropData(event.operation.source);
    const pointerDownSessionTarget = pointerDownSessionTargetRef.current;
    setIsProjectReorderDragActive(
      sourceData?.kind === 'group' || sourceData?.kind === 'project-collection' || sourceData?.kind === 'remote-machine'
    );
    if (sourceData?.kind === 'group') {
      const point = getClientPoint(nativeEvent);
      const group = groupsById[sourceData.groupId];
      const headerMetrics = point ? getProjectGroupDragHeaderMetrics(sourceData.groupId, point) : undefined;
      /**
       * CDXC:ProjectDragPreview 2026-05-21-11:45:
       * Project drag ghosts should be anchored to the live cursor and should
       * render only the project header, even when the source project is expanded.
       * Keep the source row in the list as the faint placeholder instead of
       * cloning the whole expanded project into the moving preview.
       *
       * CDXC:ProjectDragPreview 2026-05-28-12:35:
       * The project drag ghost should preserve the grabbed header button's
       * exact left edge and width, then move only on the vertical axis. Capture
       * the header row bounds at drag start and keep the pointer's initial
       * vertical offset so horizontal pointer drift never shifts the ghost.
       */
      setGroupDragPreview(
        point && headerMetrics && group?.projectContext
          ? {
              groupId: sourceData.groupId,
              isCollapsed: collapsedGroupsById[sourceData.groupId] === true,
              left: headerMetrics.left,
              pointerOffsetY: headerMetrics.pointerOffsetY,
              themeColor: group.projectContext.themeColor,
              title: group.title,
              top: headerMetrics.top,
              width: headerMetrics.width,
            }
          : undefined
      );
    } else {
      setGroupDragPreview(undefined);
    }
    if (sourceData?.kind === 'project-collection') {
      const point = getClientPoint(nativeEvent);
      const collection = projectCollections.collections.find(
        (candidate) => candidate.collectionId === sourceData.collectionId
      );
      const metrics = point
        ? getProjectCollectionDragMetrics(event.operation.source, sourceData.collectionId)
        : undefined;
      setProjectCollectionDragPreview(
        point && metrics && collection
          ? {
              collectionId: sourceData.collectionId,
              color: collection.color,
              left: metrics.left,
              pointerOffsetY: point.y - metrics.top,
              title: collection.title,
              top: metrics.top,
              width: metrics.width,
            }
          : undefined
      );
    } else {
      setProjectCollectionDragPreview(undefined);
    }
    if (sourceData?.kind === 'remote-machine') {
      const point = getClientPoint(nativeEvent);
      const machine = remoteMachines.find((candidate) => candidate.id === sourceData.remoteMachineId);
      const metrics = point ? getRemoteMachineDragHeaderMetrics(sourceData.remoteMachineId, point) : undefined;
      setRemoteMachineDragPreview(
        point && metrics && machine
          ? {
              collapsed: collapsedRemoteMachineSectionsById[sourceData.remoteMachineId] === true,
              left: metrics.left,
              machineId: sourceData.remoteMachineId,
              pointerOffsetY: metrics.pointerOffsetY,
              title: machine.name,
              top: metrics.top,
              width: metrics.width,
            }
          : undefined
      );
    } else {
      setRemoteMachineDragPreview(undefined);
    }
    sessionPointerDragStateRef.current =
      sourceData?.kind === 'session'
        ? createSessionPointerDragState(sourceData, pointerDownSessionTarget, nativeEvent)
        : undefined;
    pinnedSessionDropTargetLogKeyRef.current = undefined;
    setGroupDropIndicator(undefined);
    setPinnedSessionDropIndicator(undefined);
    setProjectCollectionDropIndicator(undefined);
    setProjectUngroupDropIndicatorScopeId(undefined);
    setRemoteMachineDropIndicator(undefined);
    setSessionDropIndicator(undefined);
    if (
      pointerDownSessionTarget &&
      sessionsById[pointerDownSessionTarget.sessionId]?.isPinned === true &&
      !(
        sourceData?.kind === 'session' &&
        sourceData.groupId === pointerDownSessionTarget.groupId &&
        sourceData.sessionId === pointerDownSessionTarget.sessionId
      )
    ) {
      postPinnedSessionReorderLog('dragStartSourceMismatch', {
        point: getClientPoint(nativeEvent),
        pointerDownSessionTarget,
        sourceData,
        sourceKind: sourceData?.kind,
        state: createPinnedSessionReorderDebugState(
          {
            groupId: pointerDownSessionTarget.groupId,
            kind: 'session',
            sessionId: pointerDownSessionTarget.sessionId,
          },
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById
        ),
        targetData: getSidebarDropData(event.operation.target),
      });
    }
    if (sourceData?.kind === 'session' && sessionsById[sourceData.sessionId]?.isPinned === true) {
      postPinnedSessionReorderLog('dragStart', {
        point: getClientPoint(nativeEvent),
        pointerDownSessionTarget,
        sourceData,
        state: createPinnedSessionReorderDebugState(
          sourceData,
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById
        ),
        targetData: getSidebarDropData(event.operation.target),
      });
    }
    postSidebarDebugLog('native.pane.reorder', 'session.dragStart', {
      nativeEventType: nativeEvent?.type,
      pointerDragState: sessionPointerDragStateRef.current,
      point: getClientPoint(nativeEvent),
      sourceData,
      targetData: getSidebarDropData(event.operation.target),
    });
  }) satisfies DragDropEventHandlers['onDragStart'];

  const handleDragMove = ((event) => {
    const nativeEvent = getDragNativeEvent(event);
    updateGroupDragPreviewFromEvent(setGroupDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setProjectCollectionDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setRemoteMachineDragPreview, nativeEvent);
    updateSessionPointerDragState(sessionPointerDragStateRef.current, nativeEvent);
    updateSessionDropIndicator(event);
  }) satisfies DragDropEventHandlers['onDragMove'];

  const handleDragOver = ((event) => {
    const nativeEvent = getDragNativeEvent(event);
    updateGroupDragPreviewFromEvent(setGroupDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setProjectCollectionDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setRemoteMachineDragPreview, nativeEvent);
    updateSessionPointerDragState(sessionPointerDragStateRef.current, nativeEvent);
    updateSessionDropIndicator(event);
  }) satisfies DragDropEventHandlers['onDragOver'];

  const handleDragEnd = ((event) => {
    setSidebarTooltipsSuppressedForDrag(false);
    setGroupDropIndicator(undefined);
    setGroupDragPreview(undefined);
    setProjectCollectionDragPreview(undefined);
    setRemoteMachineDragPreview(undefined);
    setIsProjectReorderDragActive(false);
    setPinnedSessionDropIndicator(undefined);
    setProjectCollectionDropIndicator(undefined);
    setProjectUngroupDropIndicatorScopeId(undefined);
    setRemoteMachineDropIndicator(undefined);
    setSessionDropIndicator(undefined);
    const currentGroupIds = groupIdsRef.current;
    const currentSessionIdsByGroup = sessionIdsByGroupRef.current;
    const previousSessionIdsByGroup = effectiveSessionIdsByGroup;

    const nativeEvent = getDragNativeEvent(event);
    const sourceData = getSidebarDropData(event.operation.source);
    const targetData = getSidebarDropData(event.operation.target);
    const sessionPointerDragState = sessionPointerDragStateRef.current;
    updateSessionPointerDragState(sessionPointerDragState, nativeEvent);
    sessionPointerDragStateRef.current = undefined;
    const resolvedSessionDropTarget =
      sourceData?.kind === 'session'
        ? resolveSessionDropTargetFromPoint(nativeEvent, currentSessionIdsByGroup, targetData, sourceData)
        : undefined;
    postSidebarDebugLog('native.pane.reorder', 'session.dragEnd', {
      canceled: event.canceled,
      nativeEventType: nativeEvent?.type,
      pointerDragState: sessionPointerDragState,
      point: getClientPoint(nativeEvent),
      resolvedSessionDropTarget,
      sourceData,
      targetData,
    });
    if (!sourceData) {
      return;
    }

    if (sourceData.kind === 'project-collection') {
      setProjectCollectionDropIndicator(undefined);
      if (event.canceled) {
        return;
      }

      /*
       * A collection drag moves its complete visible project block between the
       * existing collection slots. Ungrouped projects keep their slots, child
       * project order stays intact, and the resulting flat project order is
       * persisted through the same sync contract as ordinary project drags.
       *
       * CDXC:CollectionReorder 2026-07-21:
       * Collections drag with feedback "none" (like project cards), so dnd-kit
       * never reports a rect-overlap target for them: the source shape stays at
       * its resting position for the whole drag. Resolve the insertion boundary
       * from the pointer position against the visible collection panels — the
       * same pattern project rows use via resolveGroupDropTargetFromPoint.
       */
      const collectionItems = displayedProjectCollectionItems.filter(
        (item): item is Extract<SidebarProjectCollectionRenderItem, { kind: 'collection' }> =>
          item.kind === 'collection'
      );
      const collectionIds = collectionItems.map((item) => item.collection.collectionId);
      const resolvedCollectionDropTarget = resolveProjectCollectionDropTargetFromPoint(
        nativeEvent,
        collectionIds,
        sourceData.collectionId,
        targetData
      );
      if (!resolvedCollectionDropTarget) {
        return;
      }
      const nextCollectionIds = moveCollectionIdToDropTarget(
        collectionIds,
        sourceData.collectionId,
        resolvedCollectionDropTarget
      );
      if (!nextCollectionIds) {
        return;
      }

      const collectionItemById = new Map(collectionItems.map((item) => [item.collection.collectionId, item]));
      let nextCollectionIndex = 0;
      const nextRenderItems = displayedProjectCollectionItems.map((item) => {
        if (item.kind !== 'collection') {
          return item;
        }
        const collectionId = nextCollectionIds[nextCollectionIndex];
        nextCollectionIndex += 1;
        return collectionId ? (collectionItemById.get(collectionId) ?? item) : item;
      });
      const nextGroupIds = nextRenderItems.flatMap((item) =>
        item.kind === 'collection' ? item.groupIds : [item.groupId]
      );
      if (haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
        return;
      }

      const nextProjectIds = nextGroupIds.flatMap((groupId) => {
        const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
        return projectId ? [projectId] : [];
      });
      setProjectCollections((previous) =>
        reorderSidebarProjectCollections(
          reorderSidebarProjectCollectionDefinitions(previous, nextCollectionIds),
          nextProjectIds
        )
      );
      vscode.postMessage({
        groupIds: nextGroupIds,
        type: 'syncGroupOrder',
      });
      return;
    }

    if (sourceData.kind === 'remote-machine') {
      if (event.canceled) {
        return;
      }
      const resolvedRemoteMachineDropTarget = resolveRemoteMachineDropTargetFromPoint(
        nativeEvent,
        remoteMachines.map((machine) => machine.id),
        sourceData.remoteMachineId,
        targetData
      );
      if (!resolvedRemoteMachineDropTarget) {
        return;
      }
      moveRemoteMachineSection(sourceData.remoteMachineId, resolvedRemoteMachineDropTarget);
      return;
    }

    if (sourceData.kind === 'group') {
      if (event.canceled) {
        return;
      }

      /*
       * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
       * A grouped V2 row is a LOGICAL project: one header can stand for several
       * physical checkouts, on several machines. So the drop cannot be one
       * reordered id list — `syncGroupOrder` rejects a mixed local/remote or
       * cross-machine list, because each machine owns its own project order.
       *
       * Instead the row's new index among the LOGICAL rows is projected onto each
       * participating machine's own list, and one `syncGroupOrder` goes out per
       * machine that actually changed. The projection itself is pure and unit
       * tested (`packages/shared/sidebar-v2-group-order.ts`); everything DOM-dependent —
       * which boundary the pointer is over — stays in the shared resolver above,
       * so the committed reorder is the same boundary the drop line drew.
       *
       * This branch also bypasses V1's collection/ungroup handling below on
       * purpose: grouped V2 renders neither collections nor per-machine sections,
       * so there is no collection to drop out of and no machine list to leave.
       */
      if (isSidebarV2GroupedActive) {
        const rows = sidebarV2GroupOrderRowsRef.current;
        const resolvedTarget = resolveGroupDropTargetFromPoint(
          nativeEvent,
          rows.map((row) => row.groupId),
          groupsById,
          targetData,
          sourceData,
          sidebarV2GroupNoOpTargetForSource(sourceData.groupId)
        );
        if (!resolvedTarget) {
          return;
        }
        const projectedOrders = projectSidebarV2GroupOrderByMachine({
          groupIdsByMachineId: sidebarV2GroupIdsByMachineId,
          rows,
          sourceGroupId: sourceData.groupId,
          target: resolvedTarget,
        });
        for (const machineGroupIds of Object.values(projectedOrders)) {
          vscode.postMessage({
            groupIds: machineGroupIds,
            type: 'syncGroupOrder',
          });
        }
        return;
      }

      /*
       * CDXC:RemoteGroupReorder 2026-07-12:
       * Remote machine groups reorder within their machine section and post the
       * machine-scoped id order through the same syncGroupOrder contract; the
       * host persists the per-machine order. Collections apply to local
       * projects only.
       */
      const remoteMachineId = groupsById[sourceData.groupId]?.remoteMachineContext?.machineId;
      if (remoteMachineId) {
        const machineGroupIds = remoteProjectGroupIdsByMachineId[remoteMachineId] ?? [];
        const sourceProjectId = groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const resolvedUngroupDropScopeId = resolveProjectUngroupDropScopeFromPoint(
          nativeEvent,
          sourceData.groupId,
          groupsById
        );
        if (
          sourceProjectId &&
          projectCollectionIdByProjectId.has(sourceProjectId) &&
          resolvedUngroupDropScopeId === createRemoteProjectListScopeId(remoteMachineId)
        ) {
          const nextMachineGroupIds = moveProjectGroupFamilyToEnd(machineGroupIds, sourceData.groupId, groupsById);
          setProjectCollections((previous) =>
            moveProjectsToSidebarCollection(
              previous,
              getProjectCollectionFamilyProjectIds(sourceProjectId, machineGroupIds, groupsById),
              undefined
            )
          );
          if (!haveSameSessionOrder(machineGroupIds, nextMachineGroupIds)) {
            vscode.postMessage({
              groupIds: nextMachineGroupIds,
              type: 'syncGroupOrder',
            });
          }
          return;
        }
        const resolvedRemoteDropTarget = resolveGroupDropTargetFromPoint(
          nativeEvent,
          machineGroupIds,
          groupsById,
          targetData,
          sourceData
        );
        if (!resolvedRemoteDropTarget) {
          return;
        }
        const nextMachineGroupIds = moveGroupIdsByProjectDropTarget(
          machineGroupIds,
          sourceData.groupId,
          resolvedRemoteDropTarget,
          groupsById
        );
        if (haveSameSessionOrder(machineGroupIds, nextMachineGroupIds)) {
          return;
        }
        vscode.postMessage({
          groupIds: nextMachineGroupIds,
          type: 'syncGroupOrder',
        });
        return;
      }

      const sourceProjectId = groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
      const resolvedUngroupDropScopeId = resolveProjectUngroupDropScopeFromPoint(
        nativeEvent,
        sourceData.groupId,
        groupsById
      );
      if (
        sourceProjectId &&
        projectCollectionIdByProjectId.has(sourceProjectId) &&
        resolvedUngroupDropScopeId === LOCAL_PROJECT_LIST_SCOPE_ID
      ) {
        const nextGroupIds = moveProjectGroupFamilyToEnd(currentGroupIds, sourceData.groupId, groupsById);
        setProjectCollections((previous) =>
          moveProjectsToSidebarCollection(
            previous,
            getProjectCollectionFamilyProjectIds(sourceProjectId, currentGroupIds, groupsById),
            undefined
          )
        );
        if (!haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
          vscode.postMessage({
            groupIds: nextGroupIds,
            type: 'syncGroupOrder',
          });
        }
        return;
      }
      const resolvedGroupDropTarget = resolveGroupDropTargetFromPoint(
        nativeEvent,
        currentGroupIds,
        groupsById,
        targetData,
        sourceData
      );
      const isProjectGroupOrder =
        createProjectGroupOrderItems(currentGroupIds, groupsById).length === currentGroupIds.length;
      const nextGroupIds = resolvedGroupDropTarget
        ? moveGroupIdsByProjectDropTarget(currentGroupIds, sourceData.groupId, resolvedGroupDropTarget, groupsById)
        : targetData?.kind === 'group' && !isProjectGroupOrder
          ? move(currentGroupIds, event)
          : currentGroupIds;
      if (haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
        return;
      }

      if (enableProjectCollections && resolvedGroupDropTarget) {
        const sourceProjectId = groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const targetProjectId = groupsById[resolvedGroupDropTarget.groupId]?.projectContext?.editor.projectId;
        if (sourceProjectId && targetProjectId) {
          const targetCollectionId = projectCollectionIdByProjectId.get(targetProjectId);
          const sourceFamilyProjectIds = getProjectCollectionFamilyProjectIds(
            sourceProjectId,
            currentGroupIds,
            groupsById
          );
          const nextProjectIds = nextGroupIds.flatMap((groupId) => {
            const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
            return projectId ? [projectId] : [];
          });
          setProjectCollections((previous) =>
            reorderSidebarProjectCollections(
              moveProjectsToSidebarCollection(previous, sourceFamilyProjectIds, targetCollectionId),
              nextProjectIds
            )
          );
        }
      }

      vscode.postMessage({
        groupIds: nextGroupIds,
        type: 'syncGroupOrder',
      });
      return;
    }

    if (sourceData.kind !== 'session') {
      return;
    }

    if (sessionPointerDragState?.startPoint && !sessionPointerDragState.didMove) {
      if (sessionsById[sourceData.sessionId]?.isPinned === true) {
        postPinnedSessionReorderLog('dragEndIgnoredWithoutPointerMovement', {
          point: getClientPoint(nativeEvent),
          pointerDragState: sessionPointerDragState,
          sourceData,
        });
      }
      postSidebarDebugLog('native.pane.reorder', 'session.dragEndIgnoredWithoutPointerMovement', {
        point: getClientPoint(nativeEvent),
        sourceData,
      });
      return;
    }

    if (event.canceled) {
      if (sessionsById[sourceData.sessionId]?.isPinned === true) {
        postPinnedSessionReorderLog('dragEndCanceled', {
          point: getClientPoint(nativeEvent),
          sourceData,
          targetData,
        });
      }
      return;
    }

    if (sessionsById[sourceData.sessionId]?.isPinned === true) {
      const resolvedPinnedSessionDropTarget = resolvePinnedSessionDropTargetFromPoint(
        nativeEvent,
        sourceData,
        currentSessionIdsByGroup,
        sessionsById
      );
      postPinnedSessionReorderLog('dragEndResolved', {
        point: getClientPoint(nativeEvent),
        resolution: createPinnedSessionDropResolutionDebugState(
          nativeEvent,
          sourceData,
          currentSessionIdsByGroup,
          sessionsById
        ),
        resolvedPinnedSessionDropTarget,
        resolvedSessionDropTarget,
        sourceData,
        state: createPinnedSessionReorderDebugState(
          sourceData,
          currentSessionIdsByGroup,
          previousSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById
        ),
        targetData,
      });
      if (!resolvedPinnedSessionDropTarget) {
        postPinnedSessionReorderLog('dragEndSkipped', {
          reason: 'noPinnedDropTarget',
          sourceData,
          targetData,
        });
        return;
      }

      const previousPinnedSessionIds = (previousSessionIdsByGroup[sourceData.groupId] ?? []).filter(
        (sessionId) => sessionsById[sessionId]?.isPinned === true
      );
      const nextPinnedSessionIds = movePinnedSessionIdsByDropTarget(
        previousPinnedSessionIds,
        sourceData.sessionId,
        resolvedPinnedSessionDropTarget
      );
      if (
        haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds) ||
        !haveSameSessionSet(previousPinnedSessionIds, nextPinnedSessionIds)
      ) {
        postPinnedSessionReorderLog('dragEndSkipped', {
          nextPinnedSessionIds,
          previousPinnedSessionIds,
          reason: haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds)
            ? 'samePinnedOrder'
            : 'pinnedSetMismatch',
          resolvedPinnedSessionDropTarget,
          sourceData,
        });
        return;
      }

      /**
       * CDXC:PinnedSessions 2026-05-28-14:29:
       * Dropping a pinned project session must persist exactly the row slot
       * indicated during drag. Resolve pinned drops from pointer position
       * against the pinned partition, then save pinned rows first while leaving
       * non-pinned project sessions in their authoritative order.
       */
      const nextSessionIds = createPinnedFirstSessionOrder(
        (authoritativeSessionIdsByGroup[sourceData.groupId] ?? []).length > 0
          ? (authoritativeSessionIdsByGroup[sourceData.groupId] ?? [])
          : (previousSessionIdsByGroup[sourceData.groupId] ?? []),
        nextPinnedSessionIds,
        sessionsById
      );
      vscode.postMessage({
        groupId: sourceData.groupId,
        sessionIds: nextSessionIds,
        type: 'syncSessionOrder',
      });
      postPinnedSessionReorderLog('syncSessionOrderPosted', {
        nextPinnedSessionIds,
        nextSessionIds,
        previousPinnedSessionIds,
        resolvedPinnedSessionDropTarget,
        sourceData,
      });
      return;
    }

    if (resolvedSessionDropTarget === null) {
      return;
    }

    if (!targetData && resolvedSessionDropTarget === undefined) {
      return;
    }

    const nextSessionIdsByGroup =
      resolvedSessionDropTarget !== undefined
        ? moveSessionIdsByDropTarget(currentSessionIdsByGroup, sourceData.sessionId, resolvedSessionDropTarget)
        : move(currentSessionIdsByGroup, event);
    const nextListedSessionIds = new Set(Object.values(nextSessionIdsByGroup).flat());
    const omittedSessionIds = Object.values(currentSessionIdsByGroup)
      .flat()
      .filter((sessionId) => !nextListedSessionIds.has(sessionId));
    postSidebarDebugLog('native.pane.reorder', 'session.dragComputedOrder', {
      currentSessionIdsByGroup,
      nextSessionIdsByGroup,
      omittedSessionIds,
      resolvedSessionDropTarget,
      sourceData,
      targetData,
    });
    const previousGroupId = findSessionGroupId(previousSessionIdsByGroup, sourceData.sessionId);
    const nextGroupId = findSessionGroupId(nextSessionIdsByGroup, sourceData.sessionId);
    if (!previousGroupId || !nextGroupId) {
      return;
    }

    if (previousGroupId !== nextGroupId) {
      if (sessionsById[sourceData.sessionId]?.isPinned === true) {
        /**
         * CDXC:PinnedSessions 2026-05-28-12:04:
         * Project pinned sessions are only reorderable inside their owning
         * project. A pinned drag that lands over another project must not turn
         * into a cross-project move just because pinned cards are draggable in
         * the reference sidebar.
         */
        return;
      }

      const targetIndex = nextSessionIdsByGroup[nextGroupId]?.indexOf(sourceData.sessionId);
      if (targetIndex == null || targetIndex < 0) {
        return;
      }

      vscode.postMessage({
        groupId: nextGroupId,
        sessionId: sourceData.sessionId,
        targetIndex,
        type: 'moveSessionToGroup',
      });
      return;
    }

    if (!isManualActiveSessionsSort) {
      if (sessionsById[sourceData.sessionId]?.isPinned === true) {
        const authoritativeSessionIds = authoritativeSessionIdsByGroup[nextGroupId] ?? [];
        const previousSessionIds = previousSessionIdsByGroup[nextGroupId] ?? [];
        const nextDisplaySessionIds = nextSessionIdsByGroup[nextGroupId] ?? [];
        const nextPinnedSessionIds = nextDisplaySessionIds.filter(
          (sessionId) => sessionsById[sessionId]?.isPinned === true
        );
        const previousPinnedSessionIds = previousSessionIds.filter(
          (sessionId) => sessionsById[sessionId]?.isPinned === true
        );
        if (
          !haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds) &&
          haveSameSessionSet(previousPinnedSessionIds, nextPinnedSessionIds)
        ) {
          /**
           * CDXC:PinnedSessions 2026-05-28-12:04:
           * Last-activity mode still needs pinned rows to be manually
           * rearrangeable within a project. Persist only the pinned partition
           * order, then keep non-pinned sessions in their authoritative order
           * so activity sorting remains display-only for the rest of the group.
           */
          vscode.postMessage({
            groupId: nextGroupId,
            sessionIds: createPinnedFirstSessionOrder(
              authoritativeSessionIds.length > 0 ? authoritativeSessionIds : previousSessionIds,
              nextPinnedSessionIds,
              sessionsById
            ),
            type: 'syncSessionOrder',
          });
        }
      }
      return;
    }

    const previousSessionIds = previousSessionIdsByGroup[nextGroupId] ?? [];
    const nextSessionIds = nextSessionIdsByGroup[nextGroupId] ?? [];
    if (haveSameSessionOrder(previousSessionIds, nextSessionIds)) {
      return;
    }

    vscode.postMessage({
      groupId: nextGroupId,
      sessionIds: nextSessionIds,
      type: 'syncSessionOrder',
    });
  }) satisfies DragDropEventHandlers['onDragEnd'];

  return {
    handleDragEnd,
    handleDragMove,
    handleDragOver,
    handleDragStart,
    setSidebarV2GroupOrderRows,
  };
}

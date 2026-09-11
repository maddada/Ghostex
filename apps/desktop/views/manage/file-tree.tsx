import {
  forwardRef,
  memo,
  useCallback,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentProps,
} from 'react';
import type { ProjectDocsFileEntry } from '@/packages/shared/project-docs';
import type { ManageDropTarget } from './types';
import { ManageFileRow } from './file-tree-ui';
import { isManageDescendantPath } from './file-tree-utils';

const ROW_HEIGHT = 34;
const OVERSCAN = 8;
type RowProps = ComponentProps<typeof ManageFileRow>;
export type ManageFileTreeHandle = { reveal: (path: string) => void };
type Props = Pick<
  RowProps,
  'onEntryDragOver' | 'onEntryDrop' | 'onDragEnd' | 'onDragStart' | 'onOpenContextMenu' | 'onSelect'
> & {
  entries: ProjectDocsFileEntry[];
  selectedPath?: string;
  annotationCountsByPath: Map<string, number>;
  directoryPathsWithChildren: Set<string>;
  collapsedDirectoryPaths: Set<string>;
  loadedDirectories: Set<string>;
  directoryFailures: Map<string, string>;
  indexing: boolean;
  indexError?: string;
  isFileSearchActive: boolean;
  fileContextMenuPath?: string;
  dragPath?: string;
  dropTarget?: ManageDropTarget;
};

/**
 * CDXC:Docs 2026-09-11 WHY:
 * Large search results and Expand All used to mount every row, and editor keystrokes rerendered them.
 * Keep a memoized tree with fixed-height rows, preserving focused, dragged and context-menu rows while they are offscreen.
 */
export const ManageFileTree = memo(
  forwardRef<ManageFileTreeHandle, Props>(function ManageFileTree(props, ref) {
    const container = useRef<HTMLDivElement>(null);
    const [viewport, setViewport] = useState({ top: 0, height: 500 });
    const [focusedPath, setFocusedPath] = useState<string>();
    const [pendingFocus, setPendingFocus] = useState<string>();
    const { entries } = props;
    const measure = useCallback(() => {
      const node = container.current;
      if (!node) return;
      setViewport((old) =>
        old.top === node.scrollTop && old.height === node.clientHeight
          ? old
          : { top: node.scrollTop, height: node.clientHeight }
      );
    }, []);
    useLayoutEffect(() => {
      const node = container.current;
      if (!node) return;
      const observer = new ResizeObserver(measure);
      observer.observe(node);
      measure();
      return () => observer.disconnect();
    }, [measure]);
    useLayoutEffect(measure, [entries.length, measure]);

    const focusIndex = useCallback(
      (index: number, center = false) => {
        const node = container.current;
        const entry = entries[index];
        if (!node || !entry) return;
        const top = index * ROW_HEIGHT;
        if (center) node.scrollTop = Math.max(0, top - (node.clientHeight - ROW_HEIGHT) / 2);
        else if (top < node.scrollTop) node.scrollTop = top;
        else if (top + ROW_HEIGHT > node.scrollTop + node.clientHeight)
          node.scrollTop = top + ROW_HEIGHT - node.clientHeight;
        setFocusedPath(entry.path);
        setPendingFocus(entry.path);
        measure();
      },
      [entries, measure]
    );
    useImperativeHandle(
      ref,
      () => ({
        reveal(path) {
          focusIndex(
            entries.findIndex((entry) => entry.path === path),
            true
          );
        },
      }),
      [entries, focusIndex]
    );
    useLayoutEffect(() => {
      if (!pendingFocus) return;
      const row = [...(container.current?.querySelectorAll<HTMLButtonElement>('.manage-file-row') ?? [])].find(
        (row) => row.dataset.path === pendingFocus
      );
      row?.focus({ preventScroll: true });
      setPendingFocus(undefined);
    }, [pendingFocus, viewport]);

    const first = Math.max(0, Math.floor(viewport.top / ROW_HEIGHT) - OVERSCAN);
    const last = Math.min(entries.length, Math.ceil((viewport.top + viewport.height) / ROW_HEIGHT) + OVERSCAN);
    const indices = new Set<number>();
    for (let index = first; index < last; index++) indices.add(index);
    for (const path of [focusedPath, props.dragPath, props.fileContextMenuPath]) {
      if (!path) continue;
      const index = entries.findIndex((entry) => entry.path === path);
      if (index >= 0) indices.add(index);
    }
    let previous = 0;
    const rows = [...indices]
      .sort((a, b) => a - b)
      .flatMap((index) => {
        const entry = entries[index];
        const gap = index - previous;
        previous = index + 1;
        return [
          gap > 0 ? (
            <div aria-hidden='true' key={`gap-${entry.path}`} style={{ height: gap * ROW_HEIGHT, flexShrink: 0 }} />
          ) : null,
          <ManageFileRow
            key={entry.path}
            entry={entry}
            annotationCount={props.annotationCountsByPath.get(entry.path) ?? 0}
            hasActiveFileDescendant={
              entry.kind === 'directory' &&
              props.selectedPath !== undefined &&
              isManageDescendantPath(props.selectedPath, entry.path)
            }
            hasChildren={props.directoryPathsWithChildren.has(entry.path)}
            isContextMenuOpen={props.fileContextMenuPath === entry.path}
            isDragging={props.dragPath === entry.path}
            isDropTarget={props.dropTarget?.kind === 'entry' && props.dropTarget.path === entry.path}
            isExpanded={props.isFileSearchActive || !props.collapsedDirectoryPaths.has(entry.path)}
            isSelected={props.selectedPath === entry.path}
            canOpenContextMenu
            loadState={
              props.directoryFailures.has(entry.path)
                ? 'error'
                : entry.kind === 'directory' && !props.loadedDirectories.has(entry.path)
                  ? 'pending'
                  : undefined
            }
            loadError={props.directoryFailures.get(entry.path)}
            onEntryDragOver={props.onEntryDragOver}
            onEntryDrop={props.onEntryDrop}
            onDragEnd={props.onDragEnd}
            onDragStart={props.onDragStart}
            onOpenContextMenu={props.onOpenContextMenu}
            onSelect={props.onSelect}
          />,
        ];
      });
    const status =
      props.indexError ?? (props.indexing ? 'Updating files…' : entries.length === 0 ? 'No files found' : undefined);
    return (
      <>
        {status ? (
          <div className='manage-index-status' role='status'>
            {status}
          </div>
        ) : null}
        <div
          className='manage-file-list'
          data-root-drop-target={String(props.dropTarget?.kind === 'root')}
          ref={container}
          role='tree'
          aria-label='Project files'
          aria-busy={props.indexing}
          onScroll={measure}
          onFocusCapture={(event) => {
            const path = (event.target as HTMLElement).closest<HTMLElement>('[data-path]')?.dataset.path;
            if (path) setFocusedPath(path);
          }}
          onBlurCapture={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocusedPath(undefined);
          }}
          onDragOver={(event) => {
            const node = event.currentTarget;
            const bounds = node.getBoundingClientRect();
            if (event.clientY < bounds.top + 30) node.scrollTop -= 12;
            else if (event.clientY > bounds.bottom - 30) node.scrollTop += 12;
          }}
          onKeyDown={(event) => {
            const path = (event.target as HTMLElement).closest<HTMLElement>('[data-path]')?.dataset.path;
            if (!path || event.altKey || event.metaKey || event.ctrlKey) return;
            const index = entries.findIndex((entry) => entry.path === path);
            const page = Math.max(1, Math.floor(viewport.height / ROW_HEIGHT));
            let next: number | undefined;
            if (event.key === 'ArrowDown') next = index + 1;
            if (event.key === 'ArrowUp') next = index - 1;
            if (event.key === 'Home') next = 0;
            if (event.key === 'End') next = entries.length - 1;
            if (event.key === 'PageDown') next = index + page;
            if (event.key === 'PageUp') next = index - page;
            if (event.key === 'Tab') {
              next = index + (event.shiftKey ? -1 : 1);
              if (next < 0 || next >= entries.length) return;
            }
            if (next !== undefined) {
              event.preventDefault();
              focusIndex(Math.max(0, Math.min(entries.length - 1, next)));
            }
          }}
        >
          {rows}
          {previous < entries.length ? (
            <div aria-hidden='true' style={{ height: (entries.length - previous) * ROW_HEIGHT, flexShrink: 0 }} />
          ) : null}
        </div>
      </>
    );
  })
);

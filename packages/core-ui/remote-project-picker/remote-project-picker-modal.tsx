import { IconArrowBackUp, IconFolder, IconFolderPlus } from '@tabler/icons-react';
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from '@/packages/components/ui/command';
import { cn } from '@/packages/components/utils';
import {
  appendBrowsePathSegment,
  canNavigateUp,
  getBrowseDirectoryPath,
  getBrowseLeafPathSegment,
  getBrowseParentPath,
  hasTrailingPathSeparator,
  isFilesystemBrowseQuery,
  isUnsupportedWindowsProjectPath,
  resolveProjectPathForDispatch,
} from './remote-project-paths';
import {
  buildBrowseGroups,
  filterBrowseEntries,
  REMOTE_PICKER_ITEM_ICON_CLASS,
  type RemoteCommandPaletteActionItem,
} from './remote-command-palette-logic';
import type { RemoteFilesystemBrowseInput, RemoteFilesystemBrowseResult } from './remote-filesystem';

const EMPTY_BROWSE_ENTRIES: RemoteFilesystemBrowseResult['entries'] = [];

export type RemoteProjectPickerModalProps = {
  actionLabel?: string;
  description?: string;
  initialQuery?: string;
  isOpen: boolean;
  machineName: string;
  onAddProject: (path: string) => Promise<void> | void;
  onBrowse: (input: RemoteFilesystemBrowseInput) => Promise<RemoteFilesystemBrowseResult | null>;
  onClose: () => void;
  pendingLabel?: string;
  platform?: string;
  title?: string;
};

export function RemoteProjectPickerModal({
  actionLabel = 'Add',
  description,
  initialQuery = '~/',
  isOpen,
  machineName,
  onAddProject,
  onBrowse,
  onClose,
  pendingLabel = 'Adding',
  platform = typeof navigator === 'undefined' ? '' : navigator.platform,
  title = 'Add remote project',
}: RemoteProjectPickerModalProps) {
  const [query, setQuery] = useState(initialQuery);
  const [browseGeneration, setBrowseGeneration] = useState(0);
  const [browseResult, setBrowseResult] = useState<RemoteFilesystemBrowseResult | null>(null);
  const [browseError, setBrowseError] = useState<string | undefined>();
  const [highlightedItemValue, setHighlightedItemValue] = useState<string | null>(null);
  const [isBrowsePending, setIsBrowsePending] = useState(false);
  const [isAddingProject, setIsAddingProject] = useState(false);
  const browseRequestRef = useRef(0);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    setQuery(initialQuery);
    setBrowseResult(null);
    setBrowseError(undefined);
    setHighlightedItemValue(null);
    setBrowseGeneration((generation) => generation + 1);
  }, [initialQuery, isOpen]);

  const isBrowsing = isFilesystemBrowseQuery(query, platform);
  const browseDirectoryPath = isBrowsing ? getBrowseDirectoryPath(query) : '';
  const browseFilterQuery = isBrowsing && !hasTrailingPathSeparator(query) ? getBrowseLeafPathSegment(query) : '';
  const unsupportedWindowsPath = isUnsupportedWindowsProjectPath(query.trim(), platform);

  useEffect(() => {
    if (!isOpen || !isBrowsing || browseDirectoryPath.length === 0 || unsupportedWindowsPath) {
      return;
    }
    const requestId = browseRequestRef.current + 1;
    browseRequestRef.current = requestId;
    setIsBrowsePending(true);
    setBrowseError(undefined);
    void onBrowse({ partialPath: browseDirectoryPath })
      .then((result) => {
        if (browseRequestRef.current !== requestId) {
          return;
        }
        setBrowseResult(result);
      })
      .catch((error: unknown) => {
        if (browseRequestRef.current !== requestId) {
          return;
        }
        setBrowseResult(null);
        setBrowseError(error instanceof Error ? error.message : 'Unable to browse that directory.');
      })
      .finally(() => {
        if (browseRequestRef.current === requestId) {
          setIsBrowsePending(false);
        }
      });
  }, [browseDirectoryPath, browseGeneration, isBrowsing, isOpen, onBrowse, unsupportedWindowsPath]);

  const browseEntries = browseResult?.entries ?? EMPTY_BROWSE_ENTRIES;
  const { exactEntry, filteredEntries, highlightedEntry } = useMemo(
    () => filterBrowseEntries({ browseEntries, browseFilterQuery, highlightedItemValue }),
    [browseEntries, browseFilterQuery, highlightedItemValue]
  );

  function browseTo(name: string): void {
    setHighlightedItemValue(null);
    setQuery(appendBrowsePathSegment(query, name));
    setBrowseGeneration((generation) => generation + 1);
  }

  function browseUp(): void {
    const parentPath = getBrowseParentPath(query);
    if (parentPath === null) {
      return;
    }
    setHighlightedItemValue(null);
    setQuery(parentPath);
    setBrowseGeneration((generation) => generation + 1);
  }

  const resolvedAddProjectPath = hasTrailingPathSeparator(query)
    ? (browseResult?.parentPath ?? query.trim())
    : (exactEntry?.fullPath ?? query.trim());
  const hasHighlightedBrowseItem = highlightedEntry !== null;
  const canSubmitBrowsePath = isBrowsing && !unsupportedWindowsPath;
  const willCreateProjectPath =
    canSubmitBrowsePath &&
    !isBrowsePending &&
    query.trim().length > 0 &&
    !hasHighlightedBrowseItem &&
    (hasTrailingPathSeparator(query) ? !browseResult : exactEntry === null);
  const submitActionLabel = willCreateProjectPath ? `Create & ${actionLabel}` : actionLabel;
  const useMetaForMod = platform.toLowerCase().includes('mac');
  const submitModifierLabel = useMetaForMod ? '\u2318' : 'Ctrl';
  const addShortcutLabel = hasHighlightedBrowseItem ? `${submitModifierLabel} Enter` : 'Enter';

  const browseGroups = buildBrowseGroups({
    browseEntries: filteredEntries,
    browseQuery: query,
    canBrowseUp: isBrowsing && !unsupportedWindowsPath && canNavigateUp(browseDirectoryPath),
    upIcon: <IconArrowBackUp className={REMOTE_PICKER_ITEM_ICON_CLASS} />,
    directoryIcon: <IconFolder className={REMOTE_PICKER_ITEM_ICON_CLASS} />,
    browseUp,
    browseTo,
  });

  function isPrimaryModifierPressed(event: KeyboardEvent<HTMLInputElement>): boolean {
    return useMetaForMod ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  }

  async function submitAddProject(path: string): Promise<void> {
    const resolvedPath = resolveProjectPathForDispatch(path);
    if (!resolvedPath || isAddingProject) {
      return;
    }
    setIsAddingProject(true);
    try {
      await onAddProject(resolvedPath);
      onClose();
    } catch (error) {
      setBrowseError(error instanceof Error ? error.message : 'Unable to add that remote project.');
    } finally {
      setIsAddingProject(false);
    }
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>): void {
    const shouldSubmitBrowsePath =
      canSubmitBrowsePath && event.key === 'Enter' && (!hasHighlightedBrowseItem || isPrimaryModifierPressed(event));

    if (shouldSubmitBrowsePath) {
      event.preventDefault();
      void submitAddProject(resolvedAddProjectPath);
    }
  }

  function executeItem(item: RemoteCommandPaletteActionItem): void {
    void item.run();
  }

  return (
    <CommandDialog
      className='remote-project-picker-dialog max-w-2xl'
      description={description ?? `Browse project folders on ${machineName}`}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
      open={isOpen}
      title={title}
    >
      {/*
       * CDXC:RemoteProjectPicker 2026-06-02-23:22:
       * Remote Add Project uses a path-aware browse model: typing a path browses
       * that machine's gxserver, hidden folders appear only for a dot prefix,
       * Enter adds the typed/exact path, and highlighted directories require
       * the primary modifier plus Enter to add instead of navigate.
       *
       * CDXC:RemoteProjectPicker 2026-06-24-21:09:
       * Worktree Add Project should open from the top of the native child window
       * with equal top and side insets. The path input also needs explicit inline
       * text padding so typed project paths do not hug the input border or action.
       */}
      <Command
        key={`${browseGeneration}-${isBrowsing}`}
        onValueChange={(value) => {
          setHighlightedItemValue(value || null);
        }}
        value={highlightedItemValue ?? undefined}
      >
        <div className='relative'>
          <CommandInput
            className={cn(
              'remote-project-picker-input',
              isBrowsing ? (willCreateProjectPath ? 'pr-36' : 'pr-20') : undefined
            )}
            onKeyDown={handleKeyDown}
            onValueChange={(value) => {
              setHighlightedItemValue(null);
              setQuery(value);
            }}
            placeholder='Enter project path (e.g. ~/projects/my-app)'
            value={query}
          />
          {isBrowsing ? (
            <Button
              aria-label={`${submitActionLabel} (${addShortcutLabel})`}
              className={cn(
                'absolute top-1/2 right-2 h-6 -translate-y-1/2 gap-1 px-2 text-xs',
                hasHighlightedBrowseItem ? 'w-24' : 'w-16'
              )}
              disabled={unsupportedWindowsPath || isAddingProject}
              onClick={() => {
                void submitAddProject(resolvedAddProjectPath);
              }}
              type='button'
              variant='outline'
            >
              {isAddingProject ? pendingLabel : submitActionLabel}
            </Button>
          ) : null}
        </div>
        <CommandList className='max-h-[min(28rem,70vh)]'>
          {unsupportedWindowsPath ? (
            <CommandEmpty>Windows paths are only supported on Windows remote machines.</CommandEmpty>
          ) : browseError ? (
            <CommandEmpty>{browseError}</CommandEmpty>
          ) : !isBrowsing ? (
            <CommandEmpty>Enter an absolute path or ~/ path to browse.</CommandEmpty>
          ) : isBrowsePending && filteredEntries.length === 0 ? (
            <CommandEmpty>Loading directories...</CommandEmpty>
          ) : browseGroups[0]?.items.length === 0 ? (
            <CommandEmpty>
              {willCreateProjectPath ? 'Press Enter to create this folder and add it as a project.' : 'No directories.'}
            </CommandEmpty>
          ) : (
            browseGroups.map((group) => (
              <CommandGroup heading={group.label} key={group.value}>
                {group.items.map((item) => (
                  <CommandItem
                    key={item.value}
                    onMouseDown={(event) => {
                      event.preventDefault();
                    }}
                    onSelect={() => executeItem(item)}
                    value={item.value}
                  >
                    {item.icon}
                    <span className='min-w-0 flex-1 truncate'>{item.title}</span>
                    {item.value.startsWith('browse:') && item.value !== 'browse:up' ? (
                      <CommandShortcut>Enter</CommandShortcut>
                    ) : null}
                  </CommandItem>
                ))}
              </CommandGroup>
            ))
          )}
        </CommandList>
        <div className='flex items-center gap-3 border-t border-border px-3 py-2 text-xs text-muted-foreground'>
          <span>Enter Select</span>
          <span>
            {addShortcutLabel} {submitActionLabel}
          </span>
          <span>Esc Close</span>
          <span className='ml-auto inline-flex items-center gap-1'>
            <IconFolderPlus aria-hidden='true' className='size-3' />
            {machineName}
          </span>
        </div>
      </Command>
    </CommandDialog>
  );
}

import { IconRefresh, IconSearch } from '@tabler/icons-react';
import { useMemo, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Field, FieldLabel } from '@/packages/components/ui/field';
import { InputGroup, InputGroupAddon, InputGroupInput } from '@/packages/components/ui/input-group';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import { APP_MODAL_SELECT_CONTENT_CLASS } from '@/packages/core-ui/app-modal-shell';
import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionPlacement,
  GhostexInstalledExtension,
} from '@/packages/shared/ghostex-extensions';
import { InstalledExtensionCard, StoreExtensionCard } from './extension-card';
import { ExtensionEmptyState, ExtensionGroup } from './extension-surface';

type TypeFilter = 'all' | GhostexExtensionPlacement | 'terminal-pane';

function entrySupportsType(
  entry: Pick<GhostexExtensionCatalogEntry, 'kind' | 'placements'>,
  type: TypeFilter
): boolean {
  if (type === 'all') return true;
  return entry.kind === 'terminal-pane'
    ? type === 'terminal-pane'
    : entry.placements?.includes(type as GhostexExtensionPlacement) === true;
}

function typeLabel(type: TypeFilter): string {
  if (type === 'all') return 'All types';
  if (type === 'chat-bar') return 'Chat bar';
  if (type === 'terminal-pane') return 'Terminal pane';
  return type[0].toUpperCase() + type.slice(1);
}

export function StoreTab({
  catalog,
  iconUrlForCatalogEntry,
  iconUrlForInstalled,
  installed,
  loading,
  onInstalledDetails,
  onRefresh,
  onRemove,
  onSetChatBarAutoOpen,
  onSetEnabled,
  onStoreDetails,
  pendingIds,
}: {
  catalog: readonly GhostexExtensionCatalogEntry[];
  iconUrlForCatalogEntry: (entry: GhostexExtensionCatalogEntry) => string | undefined;
  iconUrlForInstalled: (extension: GhostexInstalledExtension) => string | undefined;
  installed: readonly GhostexInstalledExtension[];
  loading: boolean;
  onInstalledDetails: (extension: GhostexInstalledExtension) => void;
  onRefresh: () => void;
  onRemove: (extension: GhostexInstalledExtension) => void;
  onSetChatBarAutoOpen: (extension: GhostexInstalledExtension, autoOpen: boolean) => void;
  onSetEnabled: (extension: GhostexInstalledExtension, enabled: boolean) => void;
  onStoreDetails: (entry: GhostexExtensionCatalogEntry) => void;
  pendingIds: ReadonlySet<string>;
}) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState('all');
  const [type, setType] = useState<TypeFilter>('all');
  const categories = useMemo(
    () =>
      Array.from(
        new Set([
          ...catalog.flatMap((entry) => entry.categories),
          ...installed.flatMap((extension) => extension.manifest.categories),
        ])
      ).sort((left, right) => left.localeCompare(right)),
    [catalog, installed]
  );
  const installedById = useMemo(() => new Map(installed.map((extension) => [extension.id, extension])), [installed]);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matchesFilters = (entry: GhostexExtensionCatalogEntry | GhostexInstalledExtension['manifest']) => {
    const matchesQuery =
      !normalizedQuery ||
      `${entry.title} ${entry.description} ${entry.author} ${entry.categories.join(' ')}`
        .toLocaleLowerCase()
        .includes(normalizedQuery);
    return (
      matchesQuery && (category === 'all' || entry.categories.includes(category)) && entrySupportsType(entry, type)
    );
  };
  const filteredInstalled = installed.filter((extension) => matchesFilters(extension.manifest));
  const filteredStore = catalog.filter((entry) => !installedById.has(entry.name) && matchesFilters(entry));
  const shownCount = filteredInstalled.length + filteredStore.length;

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='extensions-modal-toolbar flex shrink-0 items-center gap-2 px-5 py-3'>
        <Field className='min-w-48 flex-1 gap-0'>
          <FieldLabel className='sr-only' htmlFor='extensions-store-search'>
            Search extensions
          </FieldLabel>
          <InputGroup className='h-8'>
            <InputGroupAddon>
              <IconSearch aria-hidden='true' />
            </InputGroupAddon>
            <InputGroupInput
              /*
               * Extensions opens on a search surface, so the query field is the
               * first control. Without this the dialog's focus fallback lands on
               * the close button and paints a focus ring over the ghost X.
               */
              autoFocus
              className='h-8 font-normal'
              id='extensions-store-search'
              onChange={(event) => setQuery(event.currentTarget.value)}
              placeholder='Search extensions'
              value={query}
            />
          </InputGroup>
        </Field>
        <Select onValueChange={(value) => setType(value as TypeFilter)} value={type}>
          <SelectTrigger aria-label='Filter extensions by type' className='w-36 font-normal'>
            <SelectValue>{typeLabel(type)}</SelectValue>
          </SelectTrigger>
          <SelectContent align='end' className={APP_MODAL_SELECT_CONTENT_CLASS}>
            <SelectGroup>
              {(['all', 'view', 'chat-bar', 'popup', 'modal', 'terminal-pane'] as const).map((value) => (
                <SelectItem key={value} value={value}>
                  {typeLabel(value)}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        {categories.length ? (
          <Select onValueChange={setCategory} value={category}>
            <SelectTrigger aria-label='Filter extensions by category' className='w-40 font-normal'>
              <SelectValue>{category === 'all' ? 'All categories' : category}</SelectValue>
            </SelectTrigger>
            <SelectContent align='end' className={APP_MODAL_SELECT_CONTENT_CLASS}>
              <SelectGroup>
                <SelectItem value='all'>All categories</SelectItem>
                {categories.map((value) => (
                  <SelectItem key={value} value={value}>
                    {value}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        ) : null}
        <span className='shrink-0 text-xs font-normal text-muted-foreground'>{shownCount} shown</span>
        <Button
          aria-label='Refresh extensions'
          disabled={loading}
          onClick={onRefresh}
          size='icon-sm'
          type='button'
          variant='ghost'
        >
          <IconRefresh />
        </Button>
      </div>
      {shownCount ? (
        <div className='vertical-scroll-fade-mask min-h-0 flex-1 overflow-y-auto px-5 py-4 [--edge-fade-distance:16px]'>
          <ExtensionGroup>
            {filteredInstalled.map((extension) => (
              <InstalledExtensionCard
                extension={extension}
                iconUrl={iconUrlForInstalled(extension)}
                key={extension.id}
                onDetails={() => onInstalledDetails(extension)}
                onRemove={() => onRemove(extension)}
                onSetChatBarAutoOpen={(autoOpen) => onSetChatBarAutoOpen(extension, autoOpen)}
                onSetEnabled={(enabled) => onSetEnabled(extension, enabled)}
                pending={pendingIds.has(extension.id)}
              />
            ))}
            {filteredStore.map((entry) => (
              <StoreExtensionCard
                entry={entry}
                iconUrl={iconUrlForCatalogEntry(entry)}
                key={entry.name}
                onDetails={() => onStoreDetails(entry)}
              />
            ))}
          </ExtensionGroup>
        </div>
      ) : (
        <ExtensionEmptyState
          description='Try a different search or clear one of the filters.'
          icon={IconSearch}
          title='No matching extensions'
        />
      )}
    </div>
  );
}

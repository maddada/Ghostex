import { IconSearch } from '@tabler/icons-react';
import { useMemo, useState } from 'react';
import { Field, FieldLabel } from '@/packages/components/ui/field';
import { Input } from '@/packages/components/ui/input';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionPlacement,
  GhostexInstalledExtension,
} from '@/packages/shared/ghostex-extensions';
import { StoreExtensionCard } from './extension-card';
import { ExtensionEmptyState, ExtensionGroup } from './extension-surface';

type TypeFilter = 'all' | GhostexExtensionPlacement | 'terminal-pane';

function entrySupportsType(entry: GhostexExtensionCatalogEntry, type: TypeFilter): boolean {
  if (type === 'all') return true;
  return entry.kind === 'terminal-pane'
    ? type === 'terminal-pane'
    : entry.placements.includes(type as GhostexExtensionPlacement);
}

function typeLabel(type: TypeFilter): string {
  if (type === 'all') return 'All types';
  if (type === 'chat-bar') return 'Chat bar';
  if (type === 'terminal-pane') return 'Terminal pane';
  return type[0].toUpperCase() + type.slice(1);
}

export function StoreTab({
  catalog,
  iconUrlFor,
  installed,
  onDetails,
}: {
  catalog: readonly GhostexExtensionCatalogEntry[];
  iconUrlFor: (entry: GhostexExtensionCatalogEntry) => string | undefined;
  installed: readonly GhostexInstalledExtension[];
  onDetails: (entry: GhostexExtensionCatalogEntry) => void;
}) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState('all');
  const [type, setType] = useState<TypeFilter>('all');
  const categories = useMemo(
    () =>
      Array.from(new Set(catalog.flatMap((entry) => entry.categories))).sort((left, right) =>
        left.localeCompare(right)
      ),
    [catalog]
  );
  const installedById = useMemo(() => new Map(installed.map((extension) => [extension.id, extension])), [installed]);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const filtered = catalog.filter((entry) => {
    const matchesQuery =
      !normalizedQuery ||
      `${entry.title} ${entry.description} ${entry.author} ${entry.categories.join(' ')}`
        .toLocaleLowerCase()
        .includes(normalizedQuery);
    return (
      matchesQuery && (category === 'all' || entry.categories.includes(category)) && entrySupportsType(entry, type)
    );
  });

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex shrink-0 items-center gap-2 border-b border-border/60 px-5 py-3'>
        <Field className='relative min-w-48 flex-1'>
          <FieldLabel className='sr-only' htmlFor='extensions-store-search'>
            Search extensions
          </FieldLabel>
          <IconSearch
            aria-hidden='true'
            className='pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground'
          />
          <Input
            className='h-8 bg-white/[0.03] pl-8 font-normal'
            id='extensions-store-search'
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder='Search extensions'
            value={query}
          />
        </Field>
        <Select onValueChange={(value) => setType(value as TypeFilter)} value={type}>
          <SelectTrigger aria-label='Filter extensions by type' className='w-36 bg-white/[0.03] font-normal' size='sm'>
            <SelectValue>{typeLabel(type)}</SelectValue>
          </SelectTrigger>
          <SelectContent align='end'>
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
            <SelectTrigger
              aria-label='Filter extensions by category'
              className='w-40 bg-white/[0.03] font-normal'
              size='sm'
            >
              <SelectValue>{category === 'all' ? 'All categories' : category}</SelectValue>
            </SelectTrigger>
            <SelectContent align='end'>
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
        <span className='shrink-0 text-xs font-normal text-muted-foreground'>{filtered.length} shown</span>
      </div>
      {filtered.length ? (
        <div className='vertical-scroll-fade-mask min-h-0 flex-1 overflow-y-auto p-3 [--edge-fade-distance:16px]'>
          <ExtensionGroup>
            {filtered.map((entry) => (
              <StoreExtensionCard
                entry={entry}
                iconUrl={iconUrlFor(entry)}
                installedVersion={installedById.get(entry.name)?.state.version}
                key={entry.name}
                onDetails={() => onDetails(entry)}
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

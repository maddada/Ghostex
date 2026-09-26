import { IconSearch } from '@tabler/icons-react';
import { Fragment, type ReactNode } from 'react';
import type { GhostexExtensionCatalogEntry, GhostexInstalledExtension } from '@/packages/shared/ghostex-extensions';
import { ExtensionCardGrid, ExtensionCardGridWide, InstalledExtensionCard, StoreExtensionCard } from './extension-card';
import {
  catalogTypes,
  extensionFilterMatches,
  type ExtensionFilter,
  type ExtensionFilterSubject,
} from './extension-filter';
import { ExtensionCardGroup, ExtensionEmptyState } from './extension-surface';

export function installedFilterSubject(extension: GhostexInstalledExtension): ExtensionFilterSubject {
  return {
    categories: extension.manifest.categories,
    searchText: [extension.manifest.description, extension.manifest.author],
    source: 'installed',
    title: extension.manifest.title,
    types: catalogTypes(extension.manifest),
  };
}

export function catalogFilterSubject(entry: GhostexExtensionCatalogEntry): ExtensionFilterSubject {
  return {
    categories: entry.categories,
    searchText: [entry.description, entry.author],
    source: 'store',
    title: entry.title,
    types: catalogTypes(entry),
  };
}

export function storeCategories(
  catalog: readonly GhostexExtensionCatalogEntry[],
  installed: readonly GhostexInstalledExtension[]
): string[] {
  return Array.from(
    new Set([
      ...catalog.flatMap((entry) => entry.categories),
      ...installed.flatMap((extension) => extension.manifest.categories),
    ])
  );
}

/** The installed list and the not-yet-installed Store entries that match the filter. */
export function filterStoreExtensions(
  filter: ExtensionFilter,
  catalog: readonly GhostexExtensionCatalogEntry[],
  installed: readonly GhostexInstalledExtension[]
) {
  const installedIds = new Set(installed.map((extension) => extension.id));
  return {
    installed: installed.filter((extension) => extensionFilterMatches(filter, installedFilterSubject(extension))),
    store: catalog.filter(
      (entry) => !installedIds.has(entry.name) && extensionFilterMatches(filter, catalogFilterSubject(entry))
    ),
  };
}

export function StoreTab({
  catalog,
  editingScopeId,
  filter,
  iconUrlForCatalogEntry,
  iconUrlForInstalled,
  installed,
  onEditScope,
  onInstall,
  onInstalledDetails,
  onRemove,
  onSetChatBarAutoOpen,
  onSetEnabled,
  onStoreDetails,
  pendingIds,
  renderScopeEditor,
  scopeSummaryFor,
}: {
  catalog: readonly GhostexExtensionCatalogEntry[];
  /** The installed extension whose scope editor is open, so its card shows the editing ring. */
  editingScopeId?: string;
  filter: ExtensionFilter;
  iconUrlForCatalogEntry: (entry: GhostexExtensionCatalogEntry) => string | undefined;
  iconUrlForInstalled: (extension: GhostexInstalledExtension) => string | undefined;
  installed: readonly GhostexInstalledExtension[];
  onEditScope?: (extension: GhostexInstalledExtension) => void;
  onInstall?: (entry: GhostexExtensionCatalogEntry) => void;
  onInstalledDetails: (extension: GhostexInstalledExtension) => void;
  onRemove: (extension: GhostexInstalledExtension) => void;
  onSetChatBarAutoOpen: (extension: GhostexInstalledExtension, autoOpen: boolean) => void;
  onSetEnabled: (extension: GhostexInstalledExtension, enabled: boolean) => void;
  onStoreDetails: (entry: GhostexExtensionCatalogEntry) => void;
  pendingIds: ReadonlySet<string>;
  renderScopeEditor?: (extension: GhostexInstalledExtension) => ReactNode;
  scopeSummaryFor?: (extension: GhostexInstalledExtension) => string | undefined;
}) {
  const filtered = filterStoreExtensions(filter, catalog, installed);
  if (!filtered.installed.length && !filtered.store.length) {
    return (
      <ExtensionEmptyState
        description='Try a different search or clear one of the filters.'
        icon={IconSearch}
        title='No matching extensions'
      />
    );
  }
  return (
    <div className='flex flex-col'>
      {filtered.installed.length ? (
        <ExtensionCardGroup count={filtered.installed.length} label='Installed'>
          <ExtensionCardGrid>
            {filtered.installed.map((extension) => (
              <Fragment key={extension.id}>
                <InstalledExtensionCard
                  editing={editingScopeId === extension.id}
                  extension={extension}
                  iconUrl={iconUrlForInstalled(extension)}
                  onDetails={() => onInstalledDetails(extension)}
                  onEditScope={onEditScope ? () => onEditScope(extension) : undefined}
                  onRemove={() => onRemove(extension)}
                  onSetChatBarAutoOpen={(autoOpen) => onSetChatBarAutoOpen(extension, autoOpen)}
                  onSetEnabled={(enabled) => onSetEnabled(extension, enabled)}
                  pending={pendingIds.has(extension.id)}
                  scopeSummary={scopeSummaryFor?.(extension)}
                />
                <ExtensionCardGridWide>{renderScopeEditor?.(extension)}</ExtensionCardGridWide>
              </Fragment>
            ))}
          </ExtensionCardGrid>
        </ExtensionCardGroup>
      ) : null}
      {filtered.store.length ? (
        <ExtensionCardGroup count={filtered.store.length} label='Available'>
          <ExtensionCardGrid>
            {filtered.store.map((entry) => (
              <StoreExtensionCard
                entry={entry}
                iconUrl={iconUrlForCatalogEntry(entry)}
                installing={pendingIds.has(entry.name)}
                key={entry.name}
                onDetails={() => onStoreDetails(entry)}
                onInstall={onInstall ? () => onInstall(entry) : undefined}
              />
            ))}
          </ExtensionCardGrid>
        </ExtensionCardGroup>
      ) : null}
    </div>
  );
}

/*
 * CDXC:Extensions 2026-08-30:
 * Extensions is no longer a standalone app modal. The Settings Extensions page
 * owns the layout: the store list renders inside its "Extensions Store"
 * section, while opening an extension's details replaces the whole page. To
 * support that split without losing state between the two placements, all the
 * browser state lives in `useExtensionsBrowserState`, and the list and detail
 * surfaces are separate components fed the same state object.
 */
import { IconPuzzle, IconRefresh } from '@tabler/icons-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionCatalog,
  GhostexExtensionStatePatch,
  GhostexInstalledExtension,
} from '@/packages/shared/ghostex-extensions';
import { InstalledExtensionDetail, StoreExtensionDetail } from './extension-detail';
import { InstallConsentDialog } from './install-consent';
import { StoreTab } from './store-tab';
import { ExtensionEmptyState } from './extension-surface';
import { extensionStaticAssetUrl, type ExtensionsModalTransport } from './transport';

type CatalogSnapshot = {
  catalog: GhostexExtensionCatalog;
  url: string;
};

function assetUrl(catalogUrl: string, path: string): string {
  return new URL(path, catalogUrl).toString();
}

function replaceInstalled(
  extensions: readonly GhostexInstalledExtension[],
  next: GhostexInstalledExtension
): GhostexInstalledExtension[] {
  const exists = extensions.some((extension) => extension.id === next.id);
  return (
    exists ? extensions.map((extension) => (extension.id === next.id ? next : extension)) : [...extensions, next]
  ).sort((left, right) => left.manifest.title.localeCompare(right.manifest.title));
}

export function useExtensionsBrowserState({
  active,
  transport,
}: {
  active: boolean;
  transport?: ExtensionsModalTransport;
}) {
  const [installed, setInstalled] = useState<GhostexInstalledExtension[]>([]);
  const [catalogSnapshot, setCatalogSnapshot] = useState<CatalogSnapshot>();
  const [selectedInstalledId, setSelectedInstalledId] = useState<string>();
  const [selectedStoreId, setSelectedStoreId] = useState<string>();
  const [consentEntry, setConsentEntry] = useState<GhostexExtensionCatalogEntry>();
  const [pendingIds, setPendingIds] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [readmeMarkdown, setReadmeMarkdown] = useState<string>();
  const [changelogMarkdown, setChangelogMarkdown] = useState<string>();
  const [loadingContent, setLoadingContent] = useState(false);

  const load = useCallback(async () => {
    if (!transport) return;
    setLoading(true);
    setError(undefined);
    try {
      const [installedResult, catalogResult] = await Promise.all([transport.list(), transport.catalog()]);
      setInstalled(
        [...installedResult.extensions].sort((left, right) => left.manifest.title.localeCompare(right.manifest.title))
      );
      setCatalogSnapshot({ catalog: catalogResult.catalog, url: catalogResult.url });
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : 'Extensions could not be loaded.');
    } finally {
      setLoading(false);
    }
  }, [transport]);

  useEffect(() => {
    if (!active) return;
    setSelectedInstalledId(undefined);
    setSelectedStoreId(undefined);
    setConsentEntry(undefined);
    void load();
  }, [active, load]);

  const catalog = catalogSnapshot?.catalog.extensions ?? [];
  const catalogById = useMemo(() => new Map(catalog.map((entry) => [entry.name, entry])), [catalog]);
  const selectedInstalled = selectedInstalledId
    ? installed.find((extension) => extension.id === selectedInstalledId)
    : undefined;
  const selectedStore = selectedStoreId ? catalogById.get(selectedStoreId) : undefined;

  useEffect(() => {
    const source = selectedStore;
    const catalogUrl = catalogSnapshot?.url;
    if (!source || !catalogUrl) {
      setReadmeMarkdown(undefined);
      setChangelogMarkdown(undefined);
      setLoadingContent(false);
      return;
    }
    const controller = new AbortController();
    setLoadingContent(true);
    Promise.all([
      fetch(assetUrl(catalogUrl, source.readme), { signal: controller.signal }),
      fetch(assetUrl(catalogUrl, source.changelog), { signal: controller.signal }),
    ])
      .then(async ([readmeResponse, changelogResponse]) => {
        if (!readmeResponse.ok || !changelogResponse.ok) {
          throw new Error('Extension documentation could not be loaded.');
        }
        const [readme, changelog] = await Promise.all([readmeResponse.text(), changelogResponse.text()]);
        setReadmeMarkdown(readme);
        setChangelogMarkdown(changelog);
      })
      .catch((contentError: unknown) => {
        if ((contentError as { name?: string }).name !== 'AbortError') {
          setReadmeMarkdown(undefined);
          setChangelogMarkdown(undefined);
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoadingContent(false);
      });
    return () => controller.abort();
  }, [catalogSnapshot?.url, selectedStore]);

  const runForExtension = useCallback(async (id: string, operation: () => Promise<void>) => {
    setPendingIds((current) => new Set(current).add(id));
    setError(undefined);
    try {
      await operation();
    } catch (operationError) {
      setError(operationError instanceof Error ? operationError.message : 'The extension operation failed.');
    } finally {
      setPendingIds((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  }, []);

  const setExtensionState = useCallback(
    async (extension: GhostexInstalledExtension, patch: GhostexExtensionStatePatch) => {
      if (!transport) return;
      await runForExtension(extension.id, async () => {
        const result = await transport.setState(extension.id, patch);
        setInstalled((current) => replaceInstalled(current, result.extension));
      });
    },
    [runForExtension, transport]
  );

  const uninstallExtension = useCallback(
    async (extension: GhostexInstalledExtension) => {
      if (!transport) return;
      await runForExtension(extension.id, async () => {
        await transport.uninstall(extension.id);
        setInstalled((current) => current.filter((candidate) => candidate.id !== extension.id));
        setSelectedInstalledId((current) => (current === extension.id ? undefined : current));
      });
    },
    [runForExtension, transport]
  );

  const installExtension = useCallback(
    async (entry: GhostexExtensionCatalogEntry) => {
      if (!transport) return;
      await runForExtension(entry.name, async () => {
        const result = await transport.install(entry.name);
        setInstalled((current) => replaceInstalled(current, result.extension));
        setConsentEntry(undefined);
      });
    },
    [runForExtension, transport]
  );

  const iconUrlForCatalogEntry = useCallback(
    (entry: GhostexExtensionCatalogEntry) =>
      installed.some((extension) => extension.id === entry.name)
        ? extensionStaticAssetUrl(entry.name, entry.icon)
        : undefined,
    [installed]
  );
  const iconUrlForInstalled = useCallback(
    (extension: GhostexInstalledExtension) => extensionStaticAssetUrl(extension.id, extension.manifest.icon),
    []
  );

  return {
    catalog,
    catalogById,
    catalogSnapshot,
    changelogMarkdown,
    consentEntry,
    detailOpen: Boolean(selectedInstalled || selectedStore),
    error,
    iconUrlForCatalogEntry,
    iconUrlForInstalled,
    installExtension,
    installed,
    load,
    loading,
    loadingContent,
    pendingIds,
    readmeMarkdown,
    selectedInstalled,
    selectedStore,
    setConsentEntry,
    setExtensionState,
    setSelectedInstalledId,
    setSelectedStoreId,
    uninstallExtension,
  };
}

export type ExtensionsBrowserState = ReturnType<typeof useExtensionsBrowserState>;

function ExtensionsErrorBanner({ error }: { error?: string }) {
  if (!error) return null;
  return (
    <div className='extensions-group bg-destructive/10 px-4 py-2.5 text-[13px] font-normal text-destructive'>
      {error}
    </div>
  );
}

/** The store/installed list, rendered inside the Extensions Store section. */
export function ExtensionsBrowserList({ state }: { state: ExtensionsBrowserState }) {
  return (
    <div className='flex flex-col gap-3'>
      <ExtensionsErrorBanner error={state.error} />
      {state.loading && !state.catalogSnapshot ? (
        <ExtensionEmptyState
          description='Reading the installed registry and extension catalog.'
          icon={IconPuzzle}
          title='Loading extensions…'
        />
      ) : state.error && !state.catalogSnapshot ? (
        <ExtensionEmptyState
          action={
            <Button
              className='font-normal'
              onClick={() => void state.load()}
              size='sm'
              type='button'
              variant='outline'
            >
              <IconRefresh data-icon='inline-start' />
              Try again
            </Button>
          }
          description={state.error}
          icon={IconPuzzle}
          title='Extensions unavailable'
        />
      ) : (
        <StoreTab
          catalog={state.catalog}
          iconUrlForCatalogEntry={state.iconUrlForCatalogEntry}
          iconUrlForInstalled={state.iconUrlForInstalled}
          installed={state.installed}
          loading={state.loading}
          onInstalledDetails={(extension) => state.setSelectedInstalledId(extension.id)}
          onRefresh={() => void state.load()}
          onRemove={(extension) => void state.uninstallExtension(extension)}
          onSetChatBarAutoOpen={(extension, chatBarAutoOpen) =>
            void state.setExtensionState(extension, { chatBarAutoOpen })
          }
          onSetEnabled={(extension, enabled) => void state.setExtensionState(extension, { enabled })}
          onStoreDetails={(entry) => state.setSelectedStoreId(entry.name)}
          pendingIds={state.pendingIds}
        />
      )}
    </div>
  );
}

/** One extension's detail page, rendered in place of the whole Extensions page. */
export function ExtensionsBrowserDetail({ state }: { state: ExtensionsBrowserState }) {
  const { catalogSnapshot, consentEntry, selectedInstalled, selectedStore } = state;
  return (
    <div className='flex flex-col gap-3'>
      <ExtensionsErrorBanner error={state.error} />
      {selectedInstalled ? (
        <InstalledExtensionDetail
          catalogEntry={state.catalogById.get(selectedInstalled.id)}
          extension={selectedInstalled}
          iconUrl={state.iconUrlForInstalled(selectedInstalled)}
          onBack={() => state.setSelectedInstalledId(undefined)}
          onSetState={(patch) => state.setExtensionState(selectedInstalled, patch)}
          onUninstall={() => state.uninstallExtension(selectedInstalled)}
          onUpdate={() => {
            const entry = state.catalogById.get(selectedInstalled.id);
            return entry ? state.installExtension(entry) : Promise.resolve();
          }}
          pending={state.pendingIds.has(selectedInstalled.id)}
        />
      ) : selectedStore ? (
        <StoreExtensionDetail
          changelogMarkdown={state.changelogMarkdown}
          entry={selectedStore}
          iconUrl={state.iconUrlForCatalogEntry(selectedStore)}
          installedVersion={state.installed.find((extension) => extension.id === selectedStore.name)?.state.version}
          loadingContent={state.loadingContent}
          onBack={() => state.setSelectedStoreId(undefined)}
          onInstall={() => state.setConsentEntry(selectedStore)}
          readmeMarkdown={state.readmeMarkdown}
          screenshotUrls={
            catalogSnapshot ? selectedStore.screenshots.map((path) => assetUrl(catalogSnapshot.url, path)) : []
          }
        />
      ) : null}
      <InstallConsentDialog
        entry={consentEntry}
        installing={Boolean(consentEntry && state.pendingIds.has(consentEntry.name))}
        onCancel={() => state.setConsentEntry(undefined)}
        onConfirm={() => {
          if (consentEntry) void state.installExtension(consentEntry);
        }}
        open={Boolean(consentEntry)}
      />
    </div>
  );
}

export function ExtensionsBrowser({ active, transport }: { active: boolean; transport: ExtensionsModalTransport }) {
  const state = useExtensionsBrowserState({ active, transport });
  return state.detailOpen ? <ExtensionsBrowserDetail state={state} /> : <ExtensionsBrowserList state={state} />;
}

export { InstalledExtensionCard, StoreExtensionCard } from './extension-card';
export { InstalledExtensionDetail, StoreExtensionDetail } from './extension-detail';
export { InstallConsentDialog } from './install-consent';
export { InstalledTab } from './installed-tab';
export { PreferencesForm } from './preferences-form';
export { createExtensionsModalTransport, type ExtensionsModalTransport } from './transport';

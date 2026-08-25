import { IconPuzzle, IconRefresh } from '@tabler/icons-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/packages/components/ui/dialog';
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/packages/components/ui/empty';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/packages/components/ui/tabs';
import { cn } from '@/packages/components/utils';
import type { SidebarTheme } from '@/packages/shared/session-grid-contract';
import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionCatalog,
  GhostexExtensionStatePatch,
  GhostexInstalledExtension,
} from '@/packages/shared/ghostex-extensions';
import { InstalledExtensionDetail, StoreExtensionDetail } from './extension-detail';
import { InstallConsentDialog } from './install-consent';
import { InstalledTab } from './installed-tab';
import { StoreTab } from './store-tab';
import { createExtensionsModalTransport, type ExtensionsModalTransport } from './transport';

export type ExtensionsModalTab = 'store' | 'installed';

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

export function ExtensionsModal({
  initialTab = 'store',
  isOpen,
  onClose,
  theme = 'dark-blue',
  transport,
}: {
  initialTab?: ExtensionsModalTab;
  isOpen: boolean;
  onClose: () => void;
  theme?: SidebarTheme;
  transport?: ExtensionsModalTransport;
}) {
  const defaultTransport = useMemo(() => createExtensionsModalTransport(), []);
  const dataSource = transport ?? defaultTransport;
  const [activeTab, setActiveTab] = useState<ExtensionsModalTab>(initialTab);
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
    setLoading(true);
    setError(undefined);
    try {
      const [installedResult, catalogResult] = await Promise.all([dataSource.list(), dataSource.catalog()]);
      setInstalled(
        [...installedResult.extensions].sort((left, right) => left.manifest.title.localeCompare(right.manifest.title))
      );
      setCatalogSnapshot({ catalog: catalogResult.catalog, url: catalogResult.url });
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : 'Extensions could not be loaded.');
    } finally {
      setLoading(false);
    }
  }, [dataSource]);

  useEffect(() => {
    if (!isOpen) return;
    setActiveTab(initialTab);
    setSelectedInstalledId(undefined);
    setSelectedStoreId(undefined);
    setConsentEntry(undefined);
    void load();
  }, [initialTab, isOpen, load]);

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
      await runForExtension(extension.id, async () => {
        const result = await dataSource.setState(extension.id, patch);
        setInstalled((current) => replaceInstalled(current, result.extension));
      });
    },
    [dataSource, runForExtension]
  );

  const uninstallExtension = useCallback(
    async (extension: GhostexInstalledExtension) => {
      await runForExtension(extension.id, async () => {
        await dataSource.uninstall(extension.id);
        setInstalled((current) => current.filter((candidate) => candidate.id !== extension.id));
        setSelectedInstalledId((current) => (current === extension.id ? undefined : current));
      });
    },
    [dataSource, runForExtension]
  );

  const installExtension = useCallback(
    async (entry: GhostexExtensionCatalogEntry) => {
      await runForExtension(entry.name, async () => {
        const result = await dataSource.install(entry.name);
        setInstalled((current) => replaceInstalled(current, result.extension));
        setConsentEntry(undefined);
      });
    },
    [dataSource, runForExtension]
  );

  const iconUrlForCatalogEntry = useCallback(
    (entry: GhostexExtensionCatalogEntry) => (catalogSnapshot ? assetUrl(catalogSnapshot.url, entry.icon) : undefined),
    [catalogSnapshot]
  );
  const iconUrlForInstalled = useCallback(
    (extension: GhostexInstalledExtension) => {
      const entry = catalogById.get(extension.id);
      return entry ? iconUrlForCatalogEntry(entry) : undefined;
    },
    [catalogById, iconUrlForCatalogEntry]
  );

  const dark = !(theme.startsWith('light-') || theme === 'plain-light');
  return (
    <Dialog
      disablePointerDismissal
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
      open={isOpen}
    >
      <DialogContent
        className={cn(
          'extensions-modal-dialog flex h-[min(850px,calc(100vh-2rem))] max-h-[calc(100vh-2rem)] flex-col gap-0 overflow-hidden bg-[#0e0e0e] p-0 font-sans sm:max-w-[1120px]',
          dark && 'dark'
        )}
        data-sidebar-theme={theme}
        showCloseButton
      >
        <DialogHeader className='shrink-0 border-b border-border/60 px-6 py-5 pr-16'>
          <DialogTitle className='text-xl font-normal'>Extensions</DialogTitle>
          <DialogDescription>Browse audited extensions and manage what is installed.</DialogDescription>
        </DialogHeader>
        {error ? (
          <div className='shrink-0 border-b border-destructive/30 bg-destructive/10 px-6 py-2 text-sm text-destructive'>
            {error}
          </div>
        ) : null}
        {loading && !catalogSnapshot ? (
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant='icon'>
                <IconPuzzle />
              </EmptyMedia>
              <EmptyTitle>Loading extensions…</EmptyTitle>
              <EmptyDescription>Reading the installed registry and extension catalog.</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : error && !catalogSnapshot ? (
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant='icon'>
                <IconPuzzle />
              </EmptyMedia>
              <EmptyTitle>Extensions unavailable</EmptyTitle>
              <EmptyDescription>{error}</EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button onClick={() => void load()} type='button' variant='outline'>
                <IconRefresh data-icon='inline-start' />
                Try again
              </Button>
            </EmptyContent>
          </Empty>
        ) : selectedInstalled ? (
          <InstalledExtensionDetail
            catalogEntry={catalogById.get(selectedInstalled.id)}
            extension={selectedInstalled}
            iconUrl={iconUrlForInstalled(selectedInstalled)}
            onBack={() => setSelectedInstalledId(undefined)}
            onSetState={(patch) => setExtensionState(selectedInstalled, patch)}
            onUninstall={() => uninstallExtension(selectedInstalled)}
            onUpdate={() => {
              const entry = catalogById.get(selectedInstalled.id);
              return entry ? installExtension(entry) : Promise.resolve();
            }}
            pending={pendingIds.has(selectedInstalled.id)}
          />
        ) : selectedStore ? (
          <StoreExtensionDetail
            changelogMarkdown={changelogMarkdown}
            entry={selectedStore}
            iconUrl={iconUrlForCatalogEntry(selectedStore)}
            installedVersion={installed.find((extension) => extension.id === selectedStore.name)?.state.version}
            loadingContent={loadingContent}
            onBack={() => setSelectedStoreId(undefined)}
            onInstall={() => setConsentEntry(selectedStore)}
            readmeMarkdown={readmeMarkdown}
            screenshotUrls={
              catalogSnapshot ? selectedStore.screenshots.map((path) => assetUrl(catalogSnapshot.url, path)) : []
            }
          />
        ) : (
          <Tabs
            className='min-h-0 flex-1 gap-0'
            onValueChange={(value) => setActiveTab(value as ExtensionsModalTab)}
            value={activeTab}
          >
            <div className='flex h-12 shrink-0 items-center justify-between border-b border-border/60 px-6'>
              <TabsList variant='line'>
                <TabsTrigger value='store'>Store</TabsTrigger>
                <TabsTrigger value='installed'>Installed ({installed.length})</TabsTrigger>
              </TabsList>
              <Button
                aria-label='Refresh extensions'
                disabled={loading}
                onClick={() => void load()}
                size='icon'
                variant='ghost'
              >
                <IconRefresh />
              </Button>
            </div>
            <TabsContent className='min-h-0 overflow-y-auto p-6' value='store'>
              <StoreTab
                catalog={catalog}
                iconUrlFor={iconUrlForCatalogEntry}
                installed={installed}
                onDetails={(entry) => setSelectedStoreId(entry.name)}
              />
            </TabsContent>
            <TabsContent className='min-h-0 overflow-y-auto p-6' value='installed'>
              <InstalledTab
                extensions={installed}
                iconUrlFor={iconUrlForInstalled}
                onDetails={(extension) => setSelectedInstalledId(extension.id)}
                onRemove={(extension) => void uninstallExtension(extension)}
                onSetEnabled={(extension, enabled) => void setExtensionState(extension, { enabled })}
                pendingIds={pendingIds}
              />
            </TabsContent>
          </Tabs>
        )}
        <InstallConsentDialog
          entry={consentEntry}
          installing={Boolean(consentEntry && pendingIds.has(consentEntry.name))}
          onCancel={() => setConsentEntry(undefined)}
          onConfirm={() => {
            if (consentEntry) void installExtension(consentEntry);
          }}
          open={Boolean(consentEntry)}
        />
      </DialogContent>
    </Dialog>
  );
}

export { InstalledExtensionCard, StoreExtensionCard } from './extension-card';
export { InstalledExtensionDetail, StoreExtensionDetail } from './extension-detail';
export { InstallConsentDialog } from './install-consent';
export { InstalledTab } from './installed-tab';
export { PreferencesForm } from './preferences-form';
export type { ExtensionsModalTransport } from './transport';

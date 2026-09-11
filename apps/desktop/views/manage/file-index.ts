import { useEffect, useMemo, useSyncExternalStore } from 'react';
import type { ProjectDocsFileEntry, ProjectDocsRequest, ProjectDocsResponse } from '@/packages/shared/project-docs';

type Entry = ProjectDocsFileEntry;
type Request = (request: Omit<ProjectDocsRequest, 'requestId'>) => Promise<ProjectDocsResponse>;
type Directory = { entries: Entry[]; revision?: string; scannedEntries?: number };
type Snapshot = {
  entries: Entry[];
  indexing: boolean;
  initialized: boolean;
  error?: string;
  loaded: Set<string>;
  failures: Map<string, string>;
};
const CACHE_PREFIX = 'ghostex-docs-index-v1:';

function sameEntries(previous: Entry[], next: Entry[]): boolean {
  return (
    previous.length === next.length &&
    previous.every((entry, index) => {
      const other = next[index];
      return (
        entry.path === other.path &&
        entry.name === other.name &&
        entry.kind === other.kind &&
        entry.depth === other.depth &&
        entry.displayPath === other.displayPath &&
        entry.modifiedAt === other.modifiedAt &&
        entry.size === other.size &&
        entry.childrenLoaded === other.childrenLoaded
      );
    })
  );
}

/**
 * CDXC:Docs 2026-09-11 DECISION:
 * User approved cached snapshots, progressive folder loading and background indexing without losing complete search or existing file actions.
 * A directory becomes empty only after its response arrives; failed and pending directories keep an explicit state.
 */
export class ManageFileIndex {
  private directories = new Map<string, Directory>();
  private scope?: string;
  private cacheDirty = false;
  private entriesDirty = false;
  private legacyFullList = false;
  private listeners = new Set<() => void>();
  private active = false;
  private generation = 0;
  private running?: Promise<void>;
  private queuedRefresh = false;
  private queue: string[] = [];
  private timer?: ReturnType<typeof setTimeout>;
  private snapshot: Snapshot = {
    entries: [],
    indexing: false,
    initialized: false,
    loaded: new Set(),
    failures: new Map(),
  };
  private key: string;

  constructor(
    private projectId: string,
    private projectEditorId: string,
    private request: Request
  ) {
    this.key = CACHE_PREFIX + JSON.stringify([projectId, projectEditorId]);
    try {
      const cached = JSON.parse(sessionStorage.getItem(this.key) ?? 'null');
      if (cached && typeof cached.scope === 'string' && Array.isArray(cached.directories)) {
        const directories = cached.directories as [string, Directory][];
        if (
          directories.every(
            ([path, value]) =>
              typeof path === 'string' &&
              Array.isArray(value.entries) &&
              value.entries.every(
                (entry) => typeof entry.path === 'string' && (entry.kind === 'file' || entry.kind === 'directory')
              )
          )
        ) {
          this.scope = cached.scope;
          this.legacyFullList = cached.legacyFullList === true;
          this.directories = new Map(directories);
          this.rebuild();
        }
      }
    } catch {
      /* A cached snapshot is optional; the authoritative directory request still runs. */
    }
  }

  getSnapshot = () => this.snapshot;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<Snapshot>) {
    if (Object.entries(patch).every(([key, value]) => this.snapshot[key as keyof Snapshot] === value)) return;
    this.snapshot = { ...this.snapshot, ...patch };
    for (const listener of this.listeners) listener();
  }

  private rebuild(patch: Partial<Snapshot> = {}) {
    const entries: Entry[] = [];
    const loaded = new Set<string>();
    const visited = new Set<string>();
    const append = (path: string) => {
      if (visited.has(path)) return;
      visited.add(path);
      const directory = this.directories.get(path);
      if (!directory) return;
      loaded.add(path);
      entries.push(...directory.entries);
      for (const entry of directory.entries) {
        if (entry.kind === 'directory') {
          if (this.legacyFullList || entry.childrenLoaded) loaded.add(entry.path);
          else append(entry.path);
        }
      }
    };
    append('');
    this.entriesDirty = false;
    this.publish({ entries, loaded, ...patch });
  }

  setEntries = (update: Entry[] | ((entries: Entry[]) => Entry[])) => {
    const next = typeof update === 'function' ? update(this.snapshot.entries) : update;
    const byPath = new Map(next.map((entry) => [entry.path, entry]));
    for (const directory of this.directories.values()) {
      directory.entries = directory.entries.map((entry) => byPath.get(entry.path) ?? entry);
    }
    this.cacheDirty = true;
    this.publish({ entries: next });
  };

  prioritize = (path: string) => {
    const position = this.queue.indexOf(path);
    if (position > 0) this.queue.unshift(...this.queue.splice(position, 1));
    if (!this.running && (!this.snapshot.loaded.has(path) || this.snapshot.failures.has(path))) void this.refresh();
  };

  start = () => {
    if (this.active) return;
    this.active = true;
    void this.refresh();
    const poll = () => {
      if (!this.active) return;
      if (document.visibilityState === 'visible') void this.refresh();
      this.timer = setTimeout(poll, 5000);
    };
    this.timer = setTimeout(poll, 5000);
  };
  stop = () => {
    this.active = false;
    this.generation++;
    clearTimeout(this.timer);
    this.running = undefined;
    this.queuedRefresh = false;
  };

  refresh = (force = false): Promise<void> => {
    if (this.running) {
      this.queuedRefresh ||= force;
      return this.running;
    }
    const generation = this.generation;
    const run = async () => {
      do {
        this.queuedRefresh = false;
        await this.scan(force, generation);
        force = true;
      } while (this.active && generation === this.generation && this.queuedRefresh);
    };
    this.running = run().finally(() => {
      if (generation === this.generation) this.running = undefined;
    });
    return this.running;
  };

  completeEntries = async (): Promise<Entry[]> => {
    await this.refresh(true);
    if (!this.snapshot.initialized || this.snapshot.failures.size) {
      throw new Error(this.snapshot.error ?? 'Wait for Docs to finish loading folders.');
    }
    return this.snapshot.entries;
  };

  private async scan(force: boolean, generation: number) {
    if (!this.active || generation !== this.generation) return;
    const current = () => this.active && generation === this.generation;
    if (!this.snapshot.initialized) this.publish({ indexing: true });
    const visited = new Set<string>();
    const failures = new Map<string, string>();
    let lastPublished = 0;
    const counts = new Map<string, number>();
    const exhausted = new Set<string>();
    const rootFor = (path: string) => (path.split('/')[0] === '.ghostex-docs-root' ? 'Docs directory' : 'Project Docs');
    let progressive = true;
    /**
     * CDXC:Docs 2026-09-11 WHY:
     * Root revisions cover only immediate entries, so every reconciliation still checks descendants, including remote hosts without watcher events.
     * Unchanged directory replies reuse the flattened snapshot and skip cache serialization; each scan owns its queue so late stopped requests cannot consume a restarted scan's work.
     */
    const queue: string[] = [];
    this.queue = queue;
    const load = async (path: string) => {
      if (!current() || visited.has(path) || exhausted.has(rootFor(path))) return;
      visited.add(path);
      try {
        const old = this.directories.get(path);
        const response = await this.request({
          action: 'list',
          directoryOnly: true,
          path,
          revision: old?.revision,
          force: force && path === '',
          projectId: this.projectId,
          projectEditorId: this.projectEditorId,
        });
        if (!current()) return;
        if (response.error) throw new Error(response.error);
        // Older remote servers return their complete list when they do not implement directoryOnly.
        if (!response.progressive) {
          progressive = false;
          const entries = response.entries ?? [];
          if (!this.legacyFullList || !old || !sameEntries(old.entries, entries)) {
            this.directories = new Map([['', { entries }]]);
            this.entriesDirty = true;
            this.cacheDirty = true;
          }
          this.legacyFullList = true;
          return;
        }
        if (path === '' && (response.scope !== this.scope || this.legacyFullList)) {
          this.directories.clear();
          this.scope = response.scope;
          this.legacyFullList = false;
          this.publish({ indexing: true });
          this.entriesDirty = true;
          this.cacheDirty = true;
        }
        if (path !== '' && response.scope !== this.scope) {
          this.queuedRefresh = true;
          throw new Error('Docs folders changed while indexing. Refreshing…');
        }
        if (response.unchanged && !this.directories.has(path))
          throw new Error('Docs returned an unchanged folder without a cached listing.');
        const root = rootFor(path);
        const scannedEntries =
          response.scannedEntries ?? response.entries?.length ?? old?.scannedEntries ?? old?.entries.length ?? 0;
        const count = (counts.get(root) ?? 0) + scannedEntries;
        counts.set(root, count);
        if (count > 20000) {
          exhausted.add(root);
          throw new Error(`${root} exceeds the scan limit of 20,000 files and folders.`);
        }
        if (!response.unchanged) {
          const previous = this.directories.get(path);
          const entries = response.entries ?? [];
          if (!previous || !sameEntries(previous.entries, entries)) {
            this.directories.set(path, { entries, revision: response.revision, scannedEntries });
            this.entriesDirty = true;
            this.cacheDirty = true;
          } else if (previous.revision !== response.revision) {
            previous.revision = response.revision;
            previous.scannedEntries = scannedEntries;
            this.cacheDirty = true;
          }
        }
        const directory = this.directories.get(path)!;
        for (const entry of directory.entries) {
          if (entry.kind === 'directory' && !entry.childrenLoaded && !visited.has(entry.path)) {
            queue.push(entry.path);
            if (!this.snapshot.loaded.has(entry.path)) this.publish({ indexing: true });
          }
        }
        if (this.entriesDirty && (path === '' || performance.now() - lastPublished > 40)) {
          this.rebuild();
          lastPublished = performance.now();
        }
      } catch (error) {
        if (!current()) return;
        const message = error instanceof Error ? error.message : 'Could not load folder.';
        failures.set(path, message);
        if (path === '') queue.length = 0;
      }
    };
    await load('');
    if (progressive) {
      const worker = async () => {
        while (current() && queue.length) {
          const path = queue.shift()!;
          await load(path);
        }
      };
      await Promise.all([worker(), worker()]);
    }
    if (!current()) return;
    if (progressive && this.cacheDirty) {
      // A failed request says nothing about its cached descendants. Prune only folders
      // disconnected by an authoritative parent listing, including removed subtrees.
      const reachable = new Set<string>();
      const pending = [''];
      while (pending.length) {
        const path = pending.pop()!;
        if (reachable.has(path)) continue;
        reachable.add(path);
        for (const entry of this.directories.get(path)?.entries ?? []) {
          if (entry.kind === 'directory' && !entry.childrenLoaded) pending.push(entry.path);
        }
      }
      for (const path of this.directories.keys()) {
        if (!reachable.has(path)) {
          this.directories.delete(path);
          this.entriesDirty = true;
        }
      }
    }
    const sameFailures =
      failures.size === this.snapshot.failures.size &&
      [...failures].every(([path, message]) => this.snapshot.failures.get(path) === message);
    const patch = {
      indexing: false,
      initialized: true,
      failures: sameFailures ? this.snapshot.failures : failures,
      error: failures.size ? [...failures.values()][0] : undefined,
    };
    if (this.entriesDirty) this.rebuild(patch);
    else this.publish(patch);
    if (!failures.size && this.scope && this.cacheDirty) {
      try {
        const serialized = JSON.stringify({
          scope: this.scope,
          legacyFullList: this.legacyFullList,
          directories: [...this.directories],
        });
        if (serialized.length < 1500000) {
          const keys = Object.keys(sessionStorage).filter((key) => key.startsWith(CACHE_PREFIX) && key !== this.key);
          while (keys.length >= 2) sessionStorage.removeItem(keys.shift()!);
          sessionStorage.setItem(this.key, serialized);
        } else sessionStorage.removeItem(this.key);
      } catch {
        /* Storage quotas do not affect the live index. */
      }
      this.cacheDirty = false;
    }
  }
}

export function useManageFileIndex(projectId: string, projectEditorId: string, request: Request) {
  const index = useMemo(
    () => new ManageFileIndex(projectId, projectEditorId, request),
    [projectId, projectEditorId, request]
  );
  const snapshot = useSyncExternalStore(index.subscribe, index.getSnapshot);
  useEffect(() => {
    index.start();
    return index.stop;
  }, [index]);
  return {
    ...snapshot,
    setEntries: index.setEntries,
    refreshIndex: index.refresh,
    prioritizeDirectory: index.prioritize,
    completeEntries: index.completeEntries,
  };
}

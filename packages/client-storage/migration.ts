import { definitionForKey, storageCatalog, type StoreId } from './catalog';
import { readBrowser, scanBrowser, writeBrowser } from './adapters/browser';
import {
  commitDatabase,
  readDatabase,
  readLegacyDatabase,
  transactionDatabase,
  type Mutation,
} from './adapters/database';
import { recordStorageEvent } from './diagnostics';
import { storageBytes } from './budgets';
import type { StorageInspection, StoreDefinition } from './types';
const unmigratedLegacy: StorageInspection['unknown'] = [];
export function legacyStorageIssues(): StorageInspection['unknown'] {
  return unmigratedLegacy.slice();
}

/** Keys a deleted feature wrote before it went away; nothing reads them, so the storage report would list them as unregistered forever. */
const RETIRED_LOCAL_KEYS = [
  // Docs files list width, fixed since 2026-09 (2af5a6761).
  'ghostex.manage.sidebarWidth',
  // The React sidebar's import intro, deleted 2026-09-24.
  'ghostex.sidebar.import-sessions-intro-seen.v1',
];

/** Copy raw values unchanged; retire a source only after a strict commit and an exact read-back. */
export async function migrateStorage(): Promise<void> {
  unmigratedLegacy.length = 0;
  for (const key of RETIRED_LOCAL_KEYS) if (readBrowser('local', key) !== null) writeBrowser('local', key, null);
  const candidates: Mutation[] = [];
  for (const [key, raw] of scanBrowser('local')) {
    const definition = definitionForKey(key);
    if (definition?.backend === 'indexeddb')
      candidates.push({ key, raw, store: definition.id as StoreId, onlyIfAbsent: true });
  }
  if (candidates.length) {
    // One bounded receipt per legacy namespace prevents a second renderer from replaying a stale import after its destination was acknowledged or removed.
    for (const store of new Set(candidates.map((entry) => entry.store)))
      await commitDatabase(
        candidates.filter((entry) => entry.store === store),
        true,
        `local:${store}`
      );
    const saved = new Map((await readDatabase()).map((entry) => [entry.key, entry.raw]));
    for (const entry of candidates) {
      if (saved.get(entry.key) !== entry.raw || readBrowser('local', entry.key) !== entry.raw) continue;
      writeBrowser('local', entry.key, null);
      recordStorageEvent({ store: entry.store, operation: 'migrate', bytes: storageBytes(entry.key, entry.raw!) });
    }
  }
  for (const legacy of [
    {
      database: 'ghostex-draft-outbox',
      table: 'drafts',
      store: 'draftOutbox' as const,
      key: (value: Record<string, unknown>) => `ghostex.sessionChat.outbox.${value.id}`,
    },
    {
      database: 'ghostex-session-chat-v1',
      table: 'snapshots',
      store: 'chatSnapshots' as const,
      key: (value: Record<string, unknown>) => `ghostex.sessionChat.snapshot.${value.key}`,
    },
  ]) {
    const source = await readLegacyDatabase(legacy.database, legacy.table);
    if (!source?.rows.length) {
      source?.close();
      continue;
    }
    const entries = source.rows.map((value) => ({
      key: legacy.key(value as Record<string, unknown>),
      store: legacy.store,
      raw: JSON.stringify(
        legacy.store === 'draftOutbox'
          ? Object.fromEntries(Object.entries(value as Record<string, unknown>).filter(([key]) => key !== 'id'))
          : value
      ),
      onlyIfAbsent: true,
    }));
    await commitDatabase(entries, true, `database:${legacy.database}`);
    const saved = new Map((await readDatabase()).map((entry) => [entry.key, entry.raw]));
    // Preserve an older independent copy when the destination has a different revision payload.
    const matched = entries.flatMap((entry, index) => (saved.get(entry.key) === entry.raw ? [index] : []));
    for (const entry of entries.filter((entry) => saved.get(entry.key) !== entry.raw))
      unmigratedLegacy.push({
        backend: 'indexeddb',
        key: `${legacy.database}/${legacy.table}/${entry.key}`,
        bytes: storageBytes(entry.key, entry.raw),
      });
    if (matched.length) await source.retire(matched);
    source.close();
    recordStorageEvent({
      store: legacy.store,
      operation: 'migrate',
      bytes: entries.reduce((sum, entry) => sum + storageBytes(entry.key, entry.raw), 0),
    });
  }
}

/** A missing upgrade is an explicit startup error; it must never erase data or silently change its interpretation. */
export async function upgradeStorageSchemas(): Promise<void> {
  const stored = await readDatabase();
  const ids = [
    ...new Set(
      stored
        .filter((entry) => {
          const definition: StoreDefinition = storageCatalog[entry.store as StoreId];
          return definition && (entry.schemaVersion ?? 1) !== definition.version;
        })
        .map((entry) => entry.store as StoreId)
    ),
  ];
  if (!ids.length) return;
  await transactionDatabase(
    (rows) =>
      rows.flatMap((entry) => {
        const definition: StoreDefinition = storageCatalog[entry.store as StoreId];
        const from = entry.schemaVersion ?? 1;
        if (from === definition.version) return [];
        if (from > definition.version || !definition.upgrade)
          throw new Error(`${definition.owner} requires a supported storage schema upgrade from version ${from}.`);
        const raw = definition.upgrade(entry.raw, from);
        definition.codec.decode(raw);
        return [{ key: entry.key, store: entry.store as StoreId, raw }];
      }),
    ids,
    true
  );
}

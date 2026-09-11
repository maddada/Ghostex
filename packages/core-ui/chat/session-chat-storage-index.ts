/**
 * CDXC:Drafts 2026-09-11 WHY:
 * Enumerating all localStorage keys on every save, acknowledgement, and recovery receipt multiplied a small text history into millions of synchronous reads.
 * Each page scans a namespace once, then indexes writes and cross-page storage events by session; durable storage remains authoritative.
 */
export class SessionChatStorageIndex<T> {
  private rows: Map<string, T> | undefined;
  private groups = new Map<string, Map<string, T>>();
  private listening = false;

  constructor(
    private readonly prefix: string,
    private readonly decode: (raw: string) => T | null,
    private readonly groupKey: (entry: T) => string
  ) {}

  private update(key: string, raw: string | null): void {
    if (!this.rows) return;
    const previous = this.rows.get(key);
    if (previous) {
      const group = this.groups.get(this.groupKey(previous));
      group?.delete(key);
      if (group?.size === 0) this.groups.delete(this.groupKey(previous));
    }
    this.rows.delete(key);
    const entry = raw === null ? null : this.decode(raw);
    if (!entry) return;
    this.rows.set(key, entry);
    const name = this.groupKey(entry);
    let group = this.groups.get(name);
    if (!group) this.groups.set(name, (group = new Map()));
    group.set(key, entry);
  }

  private reset(): void {
    this.rows = undefined;
    this.groups.clear();
  }

  private ensureLoaded(): void {
    if (this.rows) return;
    if (!this.listening) {
      window.addEventListener(
        'storage',
        (event) => {
          if (event.storageArea !== window.localStorage) return;
          if (event.key === null) {
            this.reset();
          } else if (event.key.startsWith(this.prefix)) {
            try {
              // Read the current value: an older queued event must not undo a
              // newer write this page has already made to the same key.
              this.update(event.key, window.localStorage.getItem(event.key));
            } catch {
              this.reset();
            }
          }
        },
        { capture: true }
      );
      window.addEventListener('pageshow', (event) => {
        if (event.persisted) this.reset();
      });
      this.listening = true;
    }
    this.rows = new Map();
    try {
      const storage = window.localStorage;
      for (const key of Object.keys(storage)) {
        if (key.startsWith(this.prefix)) this.update(key, storage.getItem(key));
      }
    } catch (error) {
      this.reset();
      throw error;
    }
  }

  entries(group?: string): [string, T][] {
    this.ensureLoaded();
    return [...(group === undefined ? this.rows!.entries() : (this.groups.get(group)?.entries() ?? []))];
  }

  set(key: string, value: T): void {
    const raw = JSON.stringify(value);
    window.localStorage.setItem(key, raw);
    this.update(key, raw);
  }

  remove(key: string): void {
    window.localStorage.removeItem(key);
    this.update(key, null);
  }
}

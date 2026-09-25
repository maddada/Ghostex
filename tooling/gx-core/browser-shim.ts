/**
 * The smallest browser the sidebar's TypeScript builders need to run outside a webview: a `window`
 * with two in-memory `Storage` areas and the GPUI bridge method Split Right tests for.
 *
 * Imported first by the parity harness so the client-storage adapter finds a real `Storage`
 * prototype. Nothing here persists: every harness run starts with empty storage, which is what
 * makes a scenario reproducible.
 */
class MemoryStorage {
  private entries = new Map<string, string>();

  get length(): number {
    return this.entries.size;
  }

  getItem(key: string): string | null {
    return this.entries.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.entries.set(key, String(value));
  }

  removeItem(key: string): void {
    this.entries.delete(key);
  }

  clear(): void {
    this.entries.clear();
  }

  key(index: number): string | null {
    return [...this.entries.keys()][index] ?? null;
  }
}

const globals = globalThis as Record<string, unknown>;
if (globals.Storage === undefined) globals.Storage = MemoryStorage;
if (globals.window === undefined) {
  globals.window = {
    localStorage: new MemoryStorage(),
    sessionStorage: new MemoryStorage(),
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent() {
      return true;
    },
    // Split Right is offered only where the host can focus a workspace pane.
    ghostexGpui: { postWorkspaceTerminalFocus() {} },
  };
}
if (globals.CustomEvent === undefined) {
  globals.CustomEvent = class {
    constructor(
      public type: string,
      public init?: unknown
    ) {}
  };
}

/** Empties both areas, so one scenario's writes cannot reach the next. */
export function resetBrowserStorage(): void {
  const window = globals.window as { localStorage: MemoryStorage; sessionStorage: MemoryStorage };
  window.localStorage.clear();
  window.sessionStorage.clear();
}

export function writeStorageItem(key: string, value: string): void {
  (globals.window as { localStorage: MemoryStorage }).localStorage.setItem(key, value);
}

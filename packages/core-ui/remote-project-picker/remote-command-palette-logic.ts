import type { ReactNode } from "react";
import type { RemoteFilesystemBrowseEntry } from "./remote-filesystem";

export const REMOTE_PICKER_ITEM_ICON_CLASS = "size-4 text-muted-foreground/80";
export const REMOTE_PICKER_ADDON_ICON_CLASS = "size-4";

export interface RemoteCommandPaletteItem {
  readonly description?: string;
  readonly disabled?: boolean;
  readonly icon: ReactNode;
  readonly kind: "action" | "submenu";
  readonly searchTerms: ReadonlyArray<string>;
  readonly title: ReactNode;
  readonly value: string;
}

export interface RemoteCommandPaletteActionItem extends RemoteCommandPaletteItem {
  readonly kind: "action";
  readonly keepOpen?: boolean;
  readonly run: () => Promise<void>;
}

export interface RemoteCommandPaletteGroup {
  readonly items: ReadonlyArray<RemoteCommandPaletteActionItem>;
  readonly label: string;
  readonly value: string;
}

export function filterBrowseEntries(input: {
  browseEntries: ReadonlyArray<RemoteFilesystemBrowseEntry>;
  browseFilterQuery: string;
  highlightedItemValue: string | null;
}): {
  exactEntry: RemoteFilesystemBrowseEntry | null;
  filteredEntries: RemoteFilesystemBrowseEntry[];
  highlightedEntry: RemoteFilesystemBrowseEntry | null;
} {
  const lowerFilter = input.browseFilterQuery.toLowerCase();
  const showHidden = input.browseFilterQuery.startsWith(".");

  const filteredEntries = input.browseEntries.filter(
    (entry) =>
      entry.name.toLowerCase().startsWith(lowerFilter) &&
      (showHidden || !entry.name.startsWith(".")),
  );

  let highlightedEntry: RemoteFilesystemBrowseEntry | null = null;
  if (input.highlightedItemValue?.startsWith("browse:")) {
    const highlightedPath = input.highlightedItemValue.slice("browse:".length);
    highlightedEntry = filteredEntries.find((entry) => entry.fullPath === highlightedPath) ?? null;
  }

  const exactEntry =
    input.browseFilterQuery.length > 0
      ? (filteredEntries.find((entry) => entry.name === input.browseFilterQuery) ?? null)
      : null;

  return { exactEntry, filteredEntries, highlightedEntry };
}

export function buildBrowseGroups(input: {
  browseEntries: ReadonlyArray<RemoteFilesystemBrowseEntry>;
  browseQuery: string;
  browseTo: (name: string) => void;
  browseUp: () => void;
  canBrowseUp: boolean;
  directoryIcon: ReactNode;
  upIcon: ReactNode;
}): RemoteCommandPaletteGroup[] {
  const items: RemoteCommandPaletteActionItem[] = [];

  if (input.canBrowseUp) {
    items.push({
      icon: input.upIcon,
      keepOpen: true,
      kind: "action",
      searchTerms: [input.browseQuery, ".."],
      title: "..",
      value: "browse:up",
      run: async () => {
        input.browseUp();
      },
    });
  }

  for (const entry of input.browseEntries) {
    items.push({
      icon: input.directoryIcon,
      keepOpen: true,
      kind: "action",
      searchTerms: [input.browseQuery, entry.fullPath, entry.name],
      title: entry.name,
      value: `browse:${entry.fullPath}`,
      run: async () => {
        input.browseTo(entry.name);
      },
    });
  }

  return [{ items, label: "Directories", value: "directories" }];
}

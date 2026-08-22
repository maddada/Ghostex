export interface RemoteFilesystemBrowseInput {
  cwd?: string;
  partialPath: string;
}

export interface RemoteFilesystemBrowseEntry {
  fullPath: string;
  name: string;
}

export interface RemoteFilesystemBrowseResult {
  entries: RemoteFilesystemBrowseEntry[];
  parentPath: string;
}

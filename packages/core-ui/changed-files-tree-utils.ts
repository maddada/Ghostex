import type { SidebarGitChangedFile } from "../shared/sidebar-git";

export type ChangedFilesTreeStat = {
  additions: number;
  deletions: number;
};

export type ChangedFilesTreeDirectoryNode = {
  children: ChangedFilesTreeNode[];
  kind: "directory";
  name: string;
  path: string;
  stat: ChangedFilesTreeStat;
};

export type ChangedFilesTreeFileNode = {
  kind: "file";
  name: string;
  path: string;
  stat: ChangedFilesTreeStat;
};

export type ChangedFilesTreeNode = ChangedFilesTreeDirectoryNode | ChangedFilesTreeFileNode;

type MutableDirectoryNode = {
  directories: Map<string, MutableDirectoryNode>;
  files: ChangedFilesTreeFileNode[];
  name: string;
  path: string;
  stat: ChangedFilesTreeStat;
};

const SORT_LOCALE_OPTIONS: Intl.CollatorOptions = { numeric: true, sensitivity: "base" };

function normalizePathSegments(pathValue: string): string[] {
  return pathValue
    .replaceAll("\\", "/")
    .split("/")
    .filter((segment) => segment.length > 0);
}

function compareByName(left: { name: string }, right: { name: string }): number {
  return left.name.localeCompare(right.name, undefined, SORT_LOCALE_OPTIONS);
}

function compactDirectoryNode(node: ChangedFilesTreeDirectoryNode): ChangedFilesTreeDirectoryNode {
  const compactedChildren = node.children.map((child) =>
    child.kind === "directory" ? compactDirectoryNode(child) : child,
  );

  let compactedNode: ChangedFilesTreeDirectoryNode = {
    ...node,
    children: compactedChildren,
  };

  while (compactedNode.children.length === 1 && compactedNode.children[0]?.kind === "directory") {
    const onlyChild = compactedNode.children[0];
    compactedNode = {
      children: onlyChild.children,
      kind: "directory",
      name: `${compactedNode.name}/${onlyChild.name}`,
      path: onlyChild.path,
      stat: onlyChild.stat,
    };
  }

  return compactedNode;
}

function toTreeNodes(directory: MutableDirectoryNode): ChangedFilesTreeNode[] {
  const subdirectories = Array.from(directory.directories.values())
    .toSorted(compareByName)
    .map<ChangedFilesTreeDirectoryNode>((subdirectory) => ({
      children: toTreeNodes(subdirectory),
      kind: "directory",
      name: subdirectory.name,
      path: subdirectory.path,
      stat: {
        additions: subdirectory.stat.additions,
        deletions: subdirectory.stat.deletions,
      },
    }))
    .map((subdirectory) => compactDirectoryNode(subdirectory));

  return [...subdirectories, ...directory.files.toSorted(compareByName)];
}

export function buildChangedFilesTree(
  files: ReadonlyArray<SidebarGitChangedFile>,
): ChangedFilesTreeNode[] {
  /**
   * CDXC:Worktrees 2026-05-18-23:07:
   * Git review surfaces show changed files as a compact directory tree with aggregated additions/deletions so worktree commit/PR flows can select files without flattening large project paths.
   */
  const root: MutableDirectoryNode = {
    directories: new Map(),
    files: [],
    name: "",
    path: "",
    stat: { additions: 0, deletions: 0 },
  };

  for (const file of files) {
    const segments = normalizePathSegments(file.path);
    if (segments.length === 0) {
      continue;
    }

    const fileName = segments.at(-1);
    if (!fileName) {
      continue;
    }

    const filePath = segments.join("/");
    const stat = {
      additions: Number.isFinite(file.additions) ? Math.max(0, file.additions) : 0,
      deletions: Number.isFinite(file.deletions) ? Math.max(0, file.deletions) : 0,
    };
    const ancestors: MutableDirectoryNode[] = [root];
    let currentDirectory = root;

    for (const segment of segments.slice(0, -1)) {
      const nextPath = currentDirectory.path ? `${currentDirectory.path}/${segment}` : segment;
      const existing = currentDirectory.directories.get(segment);
      if (existing) {
        currentDirectory = existing;
      } else {
        const created: MutableDirectoryNode = {
          directories: new Map(),
          files: [],
          name: segment,
          path: nextPath,
          stat: { additions: 0, deletions: 0 },
        };
        currentDirectory.directories.set(segment, created);
        currentDirectory = created;
      }
      ancestors.push(currentDirectory);
    }

    currentDirectory.files.push({
      kind: "file",
      name: fileName,
      path: filePath,
      stat,
    });

    for (const ancestor of ancestors) {
      ancestor.stat.additions += stat.additions;
      ancestor.stat.deletions += stat.deletions;
    }
  }

  return toTreeNodes(root);
}

export function summarizeChangedFiles(
  files: ReadonlyArray<SidebarGitChangedFile>,
): ChangedFilesTreeStat {
  return files.reduce(
    (total, file) => ({
      additions: total.additions + Math.max(0, file.additions),
      deletions: total.deletions + Math.max(0, file.deletions),
    }),
    { additions: 0, deletions: 0 },
  );
}

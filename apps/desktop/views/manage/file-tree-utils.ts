import { IconEdit, IconFile, IconFileTypeHtml, IconMarkdown } from '@tabler/icons-react';
import {
  type ProjectDocsFileEntry as ManageFileEntry,
  type ProjectDocsFilePreview as ManageFilePreview,
} from '@/packages/shared/project-docs';
import { MANAGE_DOCS_EXTRA_ROOT_MOUNT_PATH, MANAGE_DOCS_ROOT_PATH } from './constants';
import { ManageAnnotation, ManageArtifactKind } from './types';
import { createEmptyExcalidrawFile } from './excalidraw-io';

export function manageFileMetadataSignature(file: Pick<ManageFilePreview, 'modifiedAt' | 'path' | 'size'>): string {
  return `${file.path}\u0000${file.modifiedAt ?? ''}\u0000${file.size ?? ''}`;
}

export function createUniqueArtifactPath(
  entries: ManageFileEntry[],
  kind: ManageArtifactKind,
  directoryPath = MANAGE_DOCS_ROOT_PATH
): string {
  const occupiedPaths = new Set(entries.map((entry) => entry.path.toLocaleLowerCase()));
  const { extension, stem } = artifactNameParts(kind);
  for (let index = 1; index < 10_000; index += 1) {
    const suffix = index === 1 ? '' : `-${index}`;
    const path = `${directoryPath}/${stem}${suffix}.${extension}`;
    if (!occupiedPaths.has(path.toLocaleLowerCase())) {
      return path;
    }
  }
  return `${directoryPath}/${stem}-${Date.now()}.${extension}`;
}

export function createUniqueFolderPath(entries: ManageFileEntry[], directoryPath = MANAGE_DOCS_ROOT_PATH): string {
  const occupiedPaths = new Set(entries.map((entry) => entry.path.toLocaleLowerCase()));
  for (let index = 1; index < 10_000; index += 1) {
    const suffix = index === 1 ? '' : `-${index}`;
    const path = `${directoryPath}/folder${suffix}`;
    if (!occupiedPaths.has(path.toLocaleLowerCase())) {
      return path;
    }
  }
  return `${directoryPath}/folder-${Date.now()}`;
}

export function createDuplicateManageFilePath(entries: ManageFileEntry[], path: string): string {
  const occupiedPaths = new Set(entries.map((entry) => entry.path.toLocaleLowerCase()));
  const parentPath = parentManagePath(path);
  const fileName = basenameManagePath(path);
  const extensionIndex = fileName.lastIndexOf('.');
  const hasExtension = extensionIndex > 0 && extensionIndex < fileName.length;
  const stem = hasExtension ? fileName.slice(0, extensionIndex) : fileName;
  const extension = hasExtension ? fileName.slice(extensionIndex) : '';
  for (let index = 1; index < 10_000; index += 1) {
    const candidateName = `${stem} (${index})${extension}`;
    const candidatePath = parentPath ? `${parentPath}/${candidateName}` : candidateName;
    if (!occupiedPaths.has(candidatePath.toLocaleLowerCase())) {
      return candidatePath;
    }
  }
  const fallbackName = `${stem} (${Date.now()})${extension}`;
  return parentPath ? `${parentPath}/${fallbackName}` : fallbackName;
}

export function orderManageEntriesForTree(entries: readonly ManageFileEntry[]): ManageFileEntry[] {
  const childrenByParentPath = new Map<string, ManageFileEntry[]>();
  for (const entry of entries) {
    const parentPath = parentManagePath(entry.path);
    const siblings = childrenByParentPath.get(parentPath);
    if (siblings) {
      siblings.push(entry);
    } else {
      childrenByParentPath.set(parentPath, [entry]);
    }
  }

  const orderedEntries: ManageFileEntry[] = [];
  const visitedPaths = new Set<string>();
  const appendChildren = (parentPath: string) => {
    for (const child of childrenByParentPath.get(parentPath) ?? []) {
      if (visitedPaths.has(child.path)) {
        continue;
      }
      visitedPaths.add(child.path);
      orderedEntries.push(child);
      if (child.kind === 'directory') {
        appendChildren(child.path);
      }
    }
  };

  appendChildren('');
  for (const entry of entries) {
    if (!visitedPaths.has(entry.path)) {
      orderedEntries.push(entry);
    }
  }
  return orderedEntries;
}

export function canOpenManageEntryContextMenu(entry: ManageFileEntry): boolean {
  return entry.kind === 'file' || entry.kind === 'directory';
}

export function canRenameManageEntry(entry: ManageFileEntry): boolean {
  return !(entry.kind === 'directory' && entry.depth === 0);
}

export function canDeleteManageEntry(entry: ManageFileEntry): boolean {
  return entry.path !== MANAGE_DOCS_EXTRA_ROOT_MOUNT_PATH;
}

export function canCreateManageEntryChildren(entry: ManageFileEntry): boolean {
  return entry.kind === 'directory';
}

export function artifactNameParts(kind: ManageArtifactKind): { extension: string; stem: string } {
  switch (kind) {
    case 'excalidraw':
      return { extension: 'excalidraw', stem: 'drawing' };
    case 'html':
      return { extension: 'html', stem: 'page' };
    case 'markdown':
      return { extension: 'md', stem: 'note' };
  }
}

export function validateManageRenameFileName(name: string): string | undefined {
  if (!name) {
    return 'Enter a file name.';
  }
  if (name === '.' || name === '..') {
    return 'Use a normal file name.';
  }
  if (name.includes('/') || name.includes('\\') || name.includes('\0')) {
    return 'File names cannot contain path separators.';
  }
  return undefined;
}

export function renameManageFilePath(path: string, nextName: string): string {
  const separatorIndex = path.lastIndexOf('/');
  if (separatorIndex === -1) {
    return nextName;
  }
  return `${path.slice(0, separatorIndex + 1)}${nextName}`;
}

export function parentManagePath(path: string): string {
  const separatorIndex = path.lastIndexOf('/');
  return separatorIndex === -1 ? '' : path.slice(0, separatorIndex);
}

export function basenameManagePath(path: string): string {
  const separatorIndex = path.lastIndexOf('/');
  return separatorIndex === -1 ? path : path.slice(separatorIndex + 1);
}

export function isManageDescendantPath(path: string, ancestorPath: string): boolean {
  return path.startsWith(`${ancestorPath}/`);
}

export function createInitialCollapsedManageDirectoryPaths(entries: ManageFileEntry[]): Set<string> {
  const parentPaths = new Set<string>();
  for (const entry of entries) {
    const parentPath = parentManagePath(entry.path);
    if (parentPath) {
      parentPaths.add(parentPath);
    }
  }
  return new Set(
    entries.filter((entry) => entry.kind === 'directory' && parentPaths.has(entry.path)).map((entry) => entry.path)
  );
}

export function hasCollapsedManageAncestor(path: string, collapsedDirectoryPaths: Set<string>): boolean {
  for (const collapsedPath of collapsedDirectoryPaths) {
    if (isManageDescendantPath(path, collapsedPath)) {
      return true;
    }
  }
  return false;
}

/**
 * CDXC:Docs 2026-06-30-21:39:
 * Docs file search must keep each matching row's existing parent folders visible so nested matches retain folder context, while nonmatching siblings stay hidden and the user's collapsed-folder state remains unchanged outside search mode.
 */
export function filterManageEntriesForSearch(
  treeOrderedEntries: readonly ManageFileEntry[],
  normalizedQuery: string
): ManageFileEntry[] {
  const entryPaths = new Set(treeOrderedEntries.map((entry) => entry.path));
  const visiblePaths = new Set<string>();

  for (const entry of treeOrderedEntries) {
    if (!entry.path.toLocaleLowerCase().includes(normalizedQuery)) {
      continue;
    }
    visiblePaths.add(entry.path);

    let parentPath = parentManagePath(entry.path);
    while (parentPath) {
      if (entryPaths.has(parentPath)) {
        visiblePaths.add(parentPath);
      }
      parentPath = parentManagePath(parentPath);
    }
  }

  return treeOrderedEntries.filter((entry) => visiblePaths.has(entry.path));
}

export function moveManagePathToDirectory(path: string, targetDirectoryPath: string): string | undefined {
  const fileName = basenameManagePath(path);
  if (!fileName || targetDirectoryPath.length === 0) {
    return undefined;
  }
  return `${targetDirectoryPath}/${fileName}`;
}

export function dropDirectoryPathForManageEntry(entry: ManageFileEntry): string {
  return entry.kind === 'directory' ? entry.path : parentManagePath(entry.path) || MANAGE_DOCS_ROOT_PATH;
}

export function isNoOpManageEntryDrop(draggedEntry: ManageFileEntry, targetEntry: ManageFileEntry): boolean {
  return (
    draggedEntry.path === targetEntry.path ||
    (targetEntry.kind === 'file' && parentManagePath(draggedEntry.path) === parentManagePath(targetEntry.path))
  );
}

export function canMoveManageEntryToDirectory(
  entry: ManageFileEntry,
  targetDirectoryPath: string,
  entries: readonly ManageFileEntry[]
): boolean {
  if (targetDirectoryPath !== MANAGE_DOCS_ROOT_PATH) {
    const targetEntry = entries.find((candidate) => candidate.path === targetDirectoryPath);
    if (targetEntry?.kind !== 'directory') {
      return false;
    }
  }
  if (entry.path === targetDirectoryPath || parentManagePath(entry.path) === targetDirectoryPath) {
    return false;
  }
  if (entry.kind === 'directory' && isManageDescendantPath(targetDirectoryPath, entry.path)) {
    return false;
  }
  const nextPath = moveManagePathToDirectory(entry.path, targetDirectoryPath);
  if (!nextPath || nextPath === entry.path) {
    return false;
  }
  return !entries.some(
    (candidate) => candidate.path !== entry.path && candidate.path.toLocaleLowerCase() === nextPath.toLocaleLowerCase()
  );
}

export function remapManagePathByMove(path: string, sourcePath: string, destinationPath: string): string | undefined {
  if (path === sourcePath) {
    return destinationPath;
  }
  if (isManageDescendantPath(path, sourcePath)) {
    return `${destinationPath}${path.slice(sourcePath.length)}`;
  }
  return undefined;
}

export function remapManageAnnotationPathsForMove(
  annotationsByPath: Record<string, ManageAnnotation[]>,
  sourcePath: string,
  destinationPath: string
): Record<string, ManageAnnotation[]> {
  let changed = false;
  const next: Record<string, ManageAnnotation[]> = {};
  for (const [path, annotations] of Object.entries(annotationsByPath)) {
    const movedPath = remapManagePathByMove(path, sourcePath, destinationPath);
    if (movedPath) {
      changed = true;
      next[movedPath] = [...(next[movedPath] ?? []), ...annotations];
    } else {
      next[path] = [...(next[path] ?? []), ...annotations];
    }
  }
  return changed ? next : annotationsByPath;
}

export function remapManagePathSetForMove(
  paths: Set<string>,
  sourcePath: string,
  destinationPath: string
): Set<string> {
  const next = new Set<string>();
  for (const path of paths) {
    next.add(remapManagePathByMove(path, sourcePath, destinationPath) ?? path);
  }
  return next;
}

export function removeManageAnnotationPathsForDeletedEntry(
  annotationsByPath: Record<string, ManageAnnotation[]>,
  deletedPath: string
): Record<string, ManageAnnotation[]> {
  let changed = false;
  const next: Record<string, ManageAnnotation[]> = {};
  for (const [path, annotations] of Object.entries(annotationsByPath)) {
    if (path === deletedPath || isManageDescendantPath(path, deletedPath)) {
      changed = true;
      continue;
    }
    next[path] = [...annotations];
  }
  return changed ? next : annotationsByPath;
}

export function removeManagePathSetForDeletedEntry(paths: Set<string>, deletedPath: string): Set<string> {
  let changed = false;
  const next = new Set<string>();
  for (const path of paths) {
    if (path === deletedPath || isManageDescendantPath(path, deletedPath)) {
      changed = true;
      continue;
    }
    next.add(path);
  }
  return changed ? next : paths;
}

export function createInitialArtifactContent(kind: ManageArtifactKind): string {
  switch (kind) {
    case 'excalidraw':
      return `${JSON.stringify(createEmptyExcalidrawFile(), null, 2)}\n`;
    case 'html':
      return createDefaultHtmlDocument();
    case 'markdown':
      return '# Untitled\n\n';
  }
}

export function createDefaultHtmlDocument(): string {
  /*
   * CDXC:Docs 2026-06-28-07:17:
   * The default HTML document is user-facing onboarding copy, not a blank placeholder. It should teach users to ask an agent for a polished explanatory HTML page, then review and annotate exact rendered sections with Agentation.
   * Keep the document self-contained with inline dark-mode styles and no scripts so it remains portable, while Manage now preserves author styles for real HTML rendering.
   *
   * CDXC:Docs 2026-06-28-07:58:
   * The starter copy should describe Agentation as an idle bottom-left control on open. Users explicitly start feedback mode from Agentation only when they are ready to annotate.
   */
  return [
    '<!doctype html>',
    '<html lang="en">',
    '<head>',
    '  <meta charset="utf-8">',
    '  <meta name="viewport" content="width=device-width, initial-scale=1">',
    '  <meta name="color-scheme" content="dark">',
    '  <title>Ask an agent for an HTML explainer</title>',
    '  <style>',
    '    :root { color-scheme: dark; background: #0e0e0e; }',
    '    * { box-sizing: border-box; }',
    '    html { background: #0e0e0e; min-width: 0; }',
    '    body { margin: 0; min-width: 0; overflow-x: hidden; background: #0e0e0e; color: #c8cdd5; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif; }',
    '    main { min-height: 100vh; width: 100%; background: #0e0e0e; padding: 42px 30px 52px; }',
    '    .docs-shell { width: min(100%, 980px); margin: 0 auto; display: grid; gap: 18px; }',
    '    .docs-hero { background: #151515; border: 1px solid rgba(255, 255, 255, 0.12); border-radius: 8px; padding: 30px; box-shadow: 0 20px 60px rgba(0, 0, 0, 0.28); }',
    '    .docs-eyebrow { margin: 0 0 12px; color: #95d7f6; font-size: 12px; font-weight: 760; letter-spacing: 0; text-transform: uppercase; }',
    '    h1 { margin: 0; color: #f3f4f6; font-size: 46px; line-height: 1.02; letter-spacing: 0; max-width: 780px; }',
    '    .docs-lede { margin: 18px 0 0; color: #a6adb6; font-size: 17px; line-height: 1.65; max-width: 760px; }',
    '    .docs-card-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }',
    '    .docs-card { min-width: 0; background: #181818; border: 1px solid rgba(255, 255, 255, 0.11); border-radius: 8px; padding: 18px; }',
    '    .docs-card-kicker { margin: 0 0 10px; color: #95d7f6; font-size: 12px; font-weight: 760; text-transform: uppercase; }',
    '    .docs-card h2, .docs-prompt h2 { margin: 0 0 8px; color: #f3f4f6; font-size: 20px; line-height: 1.2; letter-spacing: 0; }',
    '    .docs-card p { margin: 0; color: #a6adb6; font-size: 14px; line-height: 1.55; }',
    '    .docs-card p + p { margin-top: 10px; }',
    '    .docs-card strong { color: #f3f4f6; }',
    '    .docs-prompt { background: #101112; border: 1px solid rgba(255, 255, 255, 0.12); border-radius: 8px; padding: 22px; }',
    '    .docs-prompt pre { margin: 0; overflow-x: auto; white-space: pre-wrap; background: #222426; border: 1px solid rgba(255, 255, 255, 0.10); border-radius: 8px; color: #e5e7eb; font: 13px/1.65 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace; padding: 16px; }',
    '    @media (max-width: 760px) { main { padding: 30px 18px 42px; } .docs-hero { padding: 24px; } h1 { font-size: 38px; } .docs-card-grid { grid-template-columns: 1fr; } }',
    '    @media (max-width: 520px) { main { padding: 24px 14px 36px; } .docs-hero, .docs-card, .docs-prompt { padding: 16px; } h1 { font-size: 32px; } .docs-lede { font-size: 15px; } }',
    '  </style>',
    '</head>',
    '<body>',
    '  <main aria-labelledby="docs-title">',
    '    <section class="docs-shell">',
    '      <header class="docs-hero">',
    '        <p class="docs-eyebrow">Ghostex Docs</p>',
    '        <h1 id="docs-title">Ask your agent for an HTML explainer</h1>',
    '        <p class="docs-lede">Use this starter as a prompt target. Ask an agent to replace it with a focused HTML document that explains a feature, workflow, bug, decision, or research topic in a way your team can scan and discuss.</p>',
    '      </header>',
    '      <section class="docs-card-grid">',
    '        <article class="docs-card">',
    '          <p class="docs-card-kicker">1. Ask</p>',
    '          <h2>Tell your agent what to explain</h2>',
    '          <p>Name the topic, audience, and level of detail. Ask for sections, examples, diagrams, tables, or callouts when they help.</p>',
    '        </article>',
    '        <article class="docs-card">',
    '          <p class="docs-card-kicker">2. Review</p>',
    '          <h2>Open the rendered document</h2>',
    '          <p>Read it in Docs like a real page. Check whether the structure, labels, and examples make the explanation clear.</p>',
    '        </article>',
    '        <article class="docs-card">',
    '          <p class="docs-card-kicker">3. Annotate</p>',
    '          <h2>Use Agentation for feedback</h2>',
    '          <p>Use the bottom-left Agentation control when you are ready. Point at the exact paragraph, diagram, or layout issue, then leave notes your agent can act on.</p>',
    '        </article>',
    '        <article class="docs-card">',
    '          <p class="docs-card-kicker">4. Refine</p>',
    '          <h2>Make feedback actionable</h2>',
    '          <p><strong>Good requests are specific.</strong> Ask for the job the document should do: onboard a teammate, explain a tradeoff, compare options, summarize an incident, or teach a workflow.</p>',
    '          <p><strong>Good annotations are precise.</strong> Mark the part that is confusing, missing, too dense, or visually off, then ask your agent to revise this HTML file.</p>',
    '        </article>',
    '      </section>',
    '      <section class="docs-prompt">',
    '        <h2>Prompt to try</h2>',
    '        <pre>Create an HTML document in docs/ that explains &lt;topic&gt; for &lt;audience&gt;. Make it dark, polished, and easy to scan. Use document-owned styles, clear sections, practical examples, and a small diagram or table if it helps. Keep it self-contained so I can annotate it in Ghostex Docs with Agentation.</pre>',
    '      </section>',
    '    </section>',
    '  </main>',
    '</body>',
    '</html>',
    '',
  ].join('\n');
}

export function formatFileSize(size: number): string {
  if (size < 1024) {
    return `${size} B`;
  }
  const units = ['KB', 'MB', 'GB'];
  let value = size / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}

export function languageLabelForPath(path: string): string {
  const extension = path.split('.').pop()?.toLocaleLowerCase();
  if (!extension || extension === path) {
    return 'Text';
  }
  const labels: Record<string, string> = {
    css: 'CSS',
    excalidraw: 'Excalidraw',
    go: 'Go',
    h: 'C/C++',
    html: 'HTML',
    js: 'JavaScript',
    json: 'JSON',
    jsx: 'React',
    md: 'Markdown',
    mjs: 'JavaScript',
    py: 'Python',
    rs: 'Rust',
    sh: 'Shell',
    swift: 'Swift',
    ts: 'TypeScript',
    tsx: 'React',
    txt: 'Text',
    yaml: 'YAML',
    yml: 'YAML',
    zig: 'Zig',
  };
  return labels[extension] ?? extension.toLocaleUpperCase();
}

export function fileIconForPath(path: string) {
  if (isMarkdownPath(path)) {
    return IconMarkdown;
  }
  if (isHtmlPath(path)) {
    return IconFileTypeHtml;
  }
  if (isExcalidrawPath(path)) {
    return IconEdit;
  }
  return IconFile;
}

export function isMarkdownPath(path: string): boolean {
  return /\.(md|markdown|mdown|mkdn)$/iu.test(path);
}

export function isExcalidrawPath(path: string): boolean {
  return /\.excalidraw$/iu.test(path);
}

export function shouldAutosaveManageFile(path: string): boolean {
  return isMarkdownPath(path) || isExcalidrawPath(path);
}

export function isHtmlPath(path: string): boolean {
  return /\.html?$/iu.test(path);
}

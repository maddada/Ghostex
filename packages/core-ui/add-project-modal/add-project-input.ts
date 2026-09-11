import { isFilesystemBrowseQuery } from '../remote-project-picker/remote-project-paths';
import type { AddProjectMachineOption, AddProjectSourceId } from './types';

export interface DetectedCloneInput {
  readonly kind: 'clone';
  readonly query: string;
  readonly source: AddProjectSourceId;
  readonly remoteUrl: string;
  readonly branchName: string;
  readonly destination: string;
  readonly cloneMainOnly: boolean;
  readonly shallowClone: boolean;
}

export type DetectedProjectInput =
  | { readonly kind: 'browse'; readonly query: string; readonly machineId?: string }
  | { readonly kind: 'ambiguous'; readonly query: string; readonly clone: DetectedCloneInput }
  | { readonly kind: 'error'; readonly message: string }
  | DetectedCloneInput;

/**
 * CDXC:AddProject 2026-09-11 DECISION:
 * User: use the local machine for pasted paths unless a machine was selected; detect cd commands, escaped spaces, provider URLs, clone options, and saved-machine paths; offer a choice when a folder and repository shorthand both make sense.
 */
export function classifyAddProjectInput(
  input: string,
  machines: readonly AddProjectMachineOption[] = []
): DetectedProjectInput | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  const machineMatches = machines.filter((machine) =>
    [machine.machineId, machine.label].some((name) => trimmed.toLowerCase().startsWith(`${name.toLowerCase()}:`))
  );
  if (machineMatches.length > 1) {
    return { kind: 'error', message: 'More than one machine has that name. Select the machine first.' };
  }
  const matchedMachine = machineMatches[0];
  if (matchedMachine) {
    const query = normalizePastedProjectPath(trimmed.slice(trimmed.indexOf(':') + 1));
    return query ? { kind: 'browse', query, machineId: matchedMachine.machineId } : null;
  }
  const path = normalizePastedProjectPath(trimmed);
  if (path) return { kind: 'browse', query: path };

  const clone = parseAddProjectCloneInput(trimmed);
  if (!clone || clone.kind === 'error') return clone;
  if (/^[\w.-]+\/[\w.-]+\/?$/u.test(trimmed) && !trimmed.split('/')[0].includes('.')) {
    return { kind: 'ambiguous', query: trimmed, clone };
  }
  return clone;
}

export function normalizePastedProjectPath(input: string): string | null {
  let value = input.trim();
  const cd = value.match(/^cd\s+(?:--\s+)?([\s\S]+)$/u);
  if (cd) value = cd[1];
  const quoted = /^(["'`])[\s\S]*\1$/u.test(value);
  if (quoted) value = value.slice(1, -1);
  else if (!/^(?:[a-z]:[\\/]|\\\\)/iu.test(value)) value = value.replace(/\\([ \t()'"\[\]&#;$`])/gu, '$1');
  if (/^file:\/\//iu.test(value)) {
    try {
      const url = new URL(value);
      if (url.hostname && url.hostname !== 'localhost') return null;
      value = decodeURIComponent(url.pathname).replace(/^\/([a-z]:\/)/iu, '$1');
    } catch {
      return null;
    }
  }
  if (value === '~') value = '~/';
  if (value === '.' || value === '..') value += '/';
  if (cd && !isFilesystemBrowseQuery(value, 'Win32')) value = `./${value}`;
  return isFilesystemBrowseQuery(value, 'Win32') ? value : null;
}

// Tokenize copied commands as text. Only the options represented in the review form are accepted.
function commandWords(input: string): string[] | null {
  const words: string[] = [];
  let word = '';
  let quote = '';
  for (let i = 0; i < input.length; i++) {
    const char = input[i];
    if (char === '\\' && quote !== "'" && i + 1 < input.length && /[\s\\"']/u.test(input[i + 1])) {
      word += input[++i];
    } else if (quote) {
      if (char === quote) quote = '';
      else word += char;
    } else if (char === '"' || char === "'") {
      quote = char;
    } else if (/\s/u.test(char)) {
      if (word) words.push(word);
      word = '';
    } else {
      word += char;
    }
  }
  if (quote) return null;
  if (word) words.push(word);
  return words;
}

export function parseAddProjectCloneInput(
  input: string,
  defaultSource: AddProjectSourceId = 'github'
): DetectedCloneInput | Extract<DetectedProjectInput, { kind: 'error' }> | null {
  const words = commandWords(input.trim());
  if (!words?.length) return null;
  const commandLength =
    words[0] === 'git' && words[1] === 'clone'
      ? 2
      : ['gh', 'glab'].includes(words[0]) && words[1] === 'repo' && words[2] === 'clone'
        ? 3
        : 0;
  let branchName = '';
  let destination = '';
  let cloneMainOnly = false;
  let shallowClone = false;
  const positional: string[] = [];
  if (commandLength) {
    let options = true;
    for (let i = commandLength; i < words.length; i++) {
      const word = words[i];
      if (options && word === '--') {
        options = commandLength === 3;
        continue;
      }
      if (options && (word === '-b' || word === '--branch')) {
        branchName = words[++i] ?? '';
        if (!branchName) return null;
      } else if (options && (word.startsWith('--branch=') || /^-b.+/u.test(word))) {
        branchName = word.startsWith('--branch=') ? word.slice(9) : word.slice(2);
      } else if (options && word === '--single-branch') cloneMainOnly = true;
      else if (options && (word === '--depth' || word.startsWith('--depth='))) {
        const depth = word === '--depth' ? words[++i] : word.slice(8);
        if (depth !== '1')
          return { kind: 'error', message: 'The clone form supports --depth 1. Remove the depth option or use 1.' };
        shallowClone = true;
      } else if (options && word.startsWith('-')) {
        return { kind: 'error', message: `The clone form does not support ${word}. Remove it to continue.` };
      } else positional.push(word);
    }
    if (positional.length > 2)
      return { kind: 'error', message: 'Enter one repository and an optional destination folder.' };
    destination = positional[1] ?? '';
  } else positional.push(input.trim().replace(/^(["'`])([\s\S]*)\1$/u, '$2'));
  let token = positional[0];
  if (!token) return null;
  const isGitlabShorthand = words[0] === 'glab' || (!commandLength && defaultSource === 'gitlab');
  const shorthand =
    (isGitlabShorthand ? /^[\w.-]+(?:\/[\w.-]+)+\/?$/u : /^[\w.-]+\/[\w.-]+\/?$/u).test(token) &&
    !token.split('/')[0].includes('.');
  if (shorthand)
    token = `https://${isGitlabShorthand ? 'gitlab.com' : !commandLength && defaultSource === 'bitbucket' ? 'bitbucket.org' : 'github.com'}/${token}`;
  const scp = token.match(/^([^@\s]+)@([^:/\s]+):(.+)$/u);
  let url: URL;
  try {
    url = new URL(
      scp ? `ssh://${scp[1]}@${scp[2]}/${scp[3]}` : /^(?:https?|ssh):\/\//iu.test(token) ? token : `https://${token}`
    );
  } catch {
    return null;
  }
  if (!['https:', 'http:', 'ssh:'].includes(url.protocol) || !url.hostname.includes('.')) return null;
  const host = url.hostname.toLowerCase();
  const source: AddProjectSourceId =
    host === 'github.com'
      ? 'github'
      : host === 'gitlab.com'
        ? 'gitlab'
        : host === 'bitbucket.org'
          ? 'bitbucket'
          : host === 'dev.azure.com' || host === 'ssh.dev.azure.com' || host.endsWith('.visualstudio.com')
            ? 'azure-devops'
            : 'url';
  const parts = url.pathname.split('/').filter(Boolean);
  let repositoryParts = parts;
  if (source === 'github' || source === 'bitbucket') {
    if (parts.length < 2) return null;
    repositoryParts = parts.slice(0, 2);
    // A single tree segment (including an encoded slash) unambiguously names the ref.
    if (!branchName && parts[2] === 'tree' && parts.length === 4) branchName = decodePart(parts[3]);
  } else if (source === 'gitlab') {
    const separator = parts.indexOf('-');
    if (separator >= 0) {
      repositoryParts = parts.slice(0, separator);
      if (!branchName && parts[separator + 1] === 'tree' && parts.length === separator + 3)
        branchName = decodePart(parts[separator + 2]);
    }
    if (repositoryParts.length < 2) return null;
  } else if (source === 'azure-devops') {
    const git = parts.indexOf('_git');
    if (git >= 0) repositoryParts = parts.slice(0, git + 2);
    else if (host === 'ssh.dev.azure.com' && parts[0] === 'v3' && parts.length === 4) {
      repositoryParts = parts;
    } else return null;
    if (!repositoryParts.at(-1) || repositoryParts.at(-1) === '_git') return null;
    const version = url.searchParams.get('version');
    if (!branchName && version?.startsWith('GB')) branchName = version.slice(2);
  } else if (!parts.length) return null;
  if (source !== 'azure-devops' && !repositoryParts.at(-1)?.toLowerCase().endsWith('.git')) {
    repositoryParts = [...repositoryParts.slice(0, -1), `${repositoryParts.at(-1)}.git`];
  }
  url.pathname = `/${repositoryParts.join('/')}`;
  url.search = '';
  url.hash = '';
  const remoteUrl = scp ? `${scp[1]}@${scp[2]}:${repositoryParts.join('/')}` : url.toString();
  return {
    kind: 'clone',
    query: input.trim(),
    source,
    remoteUrl,
    branchName,
    destination,
    cloneMainOnly,
    shallowClone,
  };
}

function decodePart(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

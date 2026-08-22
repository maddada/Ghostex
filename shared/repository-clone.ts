export type ParsedRepositoryCloneInput = {
  cloneUrl: string;
  repositoryName: string;
};

const MAX_REPOSITORY_BRANCH_NAME_LENGTH = 255;
const DEFAULT_REPOSITORY_HOST = "github.com";
const REPOSITORY_BROWSER_PATH_STOP_SEGMENTS = new Set([
  "-",
  "branches",
  "commit",
  "commits",
  "issues",
  "pull",
  "pulls",
  "releases",
  "src",
  "tree",
  "wiki",
]);

/**
 * CDXC:AddRepository 2026-05-29-11:45:
 * The Clone Repository modal accepts pasted clone commands, owner/repo shorthand,
 * HTTPS URLs, SSH scp URLs, and host/path strings. Normalize the input once so
 * cloning and destination-folder naming do not depend on the exact paste style.
 */
export function parseRepositoryCloneInput(input: string): ParsedRepositoryCloneInput | undefined {
  const token = extractRepositoryInputToken(input);
  if (!token) {
    return undefined;
  }

  const sshMatch = token.match(/^git@([^:]+):(.+)$/i);
  if (sshMatch) {
    const host = sshMatch[1]?.trim();
    const path = normalizeRepositoryPath(sshMatch[2] ?? "");
    if (!host || !path) {
      return undefined;
    }
    return {
      cloneUrl: `git@${host}:${path}`,
      repositoryName: repositoryNameFromPath(path),
    };
  }

  const sshUrlMatch = token.match(/^ssh:\/\/(?:[^@/\s]+@)?([^/\s]+)\/(.+)$/i);
  if (sshUrlMatch) {
    const host = sshUrlMatch[1]?.trim();
    const path = normalizeRepositoryPath(sshUrlMatch[2] ?? "");
    if (!host || !path) {
      return undefined;
    }
    return {
      cloneUrl: `ssh://git@${host}/${path}`,
      repositoryName: repositoryNameFromPath(path),
    };
  }

  const httpMatch = token.match(/^(?:https?:\/\/)?([^/\s]+\.[^/\s]+)\/(.+)$/i);
  if (httpMatch) {
    const host = httpMatch[1]?.trim();
    const path = normalizeRepositoryPath(httpMatch[2] ?? "");
    if (!host || !path) {
      return undefined;
    }
    return {
      cloneUrl: `https://${host}/${path}`,
      repositoryName: repositoryNameFromPath(path),
    };
  }

  const shorthandPath = normalizeRepositoryPath(token);
  if (!shorthandPath || shorthandPath.split("/").length < 2) {
    return undefined;
  }
  return {
    cloneUrl: `https://${DEFAULT_REPOSITORY_HOST}/${shorthandPath}`,
    repositoryName: repositoryNameFromPath(shorthandPath),
  };
}

/*
CDXC:AddProjectCloneReview 2026-08-22:
The unified Add Project confirmation step accepts an optional branch name.
Empty means Git uses the repository default branch, usually main or master;
typed names must be valid ref-like branch names before Clone & Add is enabled.
*/
export function isRepositoryCloneBranchNameInputValid(input: string): boolean {
  const branchName = input.trim();
  if (!branchName) {
    return true;
  }
  if (
    branchName.length > MAX_REPOSITORY_BRANCH_NAME_LENGTH ||
    branchName === "@" ||
    branchName.startsWith("-") ||
    branchName.startsWith("/") ||
    branchName.endsWith("/") ||
    branchName.endsWith(".") ||
    branchName.includes("..") ||
    branchName.includes("@{") ||
    /[\s~^:?*[\\\x00-\x1F\x7F]/.test(branchName)
  ) {
    return false;
  }

  return branchName.split("/").every((segment) => {
    return Boolean(segment) && !segment.startsWith(".") && !segment.endsWith(".lock");
  });
}

function extractRepositoryInputToken(input: string): string | undefined {
  const trimmed = input.trim();
  if (!trimmed) {
    return undefined;
  }

  const tokens = trimmed
    .split(/\s+/)
    .map(cleanRepositoryInputToken)
    .filter(Boolean);
  const ghCloneIndex = tokens.findIndex((token, index) =>
    token === "clone" && tokens[index - 1] === "repo" && tokens[index - 2] === "gh"
  );
  if (ghCloneIndex >= 0) {
    return tokens.slice(ghCloneIndex + 1).find(isRepositoryLikeToken);
  }

  return tokens.find(isRepositoryLikeToken);
}

function cleanRepositoryInputToken(token: string): string {
  return token
    .trim()
    .replace(/^[<("'`]+/g, "")
    .replace(/[>),."'`]+$/g, "");
}

function isRepositoryLikeToken(token: string): boolean {
  if (!token || token.startsWith("-")) {
    return false;
  }
  return (
    /^git@[^:]+:.+/.test(token) ||
    /^ssh:\/\/.+\/.+/i.test(token) ||
    /^https?:\/\/.+\/.+/i.test(token) ||
    /^[^/\s]+\.[^/\s]+\/.+/.test(token) ||
    /^[^/\s]+\/[^/\s]+/.test(token)
  );
}

function normalizeRepositoryPath(path: string): string {
  const beforeHash = path.split("#")[0] ?? "";
  const beforeQuery = beforeHash.split("?")[0] ?? "";
  const beforeGitSuffix = beforeQuery.replace(/\.git(?:\/.*)?$/i, ".git");
  const segments = beforeGitSuffix
    .split("/")
    .map((segment) => decodeRepositoryPathSegment(segment))
    .filter(Boolean);
  const stopIndex = segments.findIndex((segment) =>
    REPOSITORY_BROWSER_PATH_STOP_SEGMENTS.has(segment.toLowerCase()),
  );
  const repositorySegments = stopIndex >= 0 ? segments.slice(0, stopIndex) : segments;
  const normalizedPath = repositorySegments.join("/");
  return normalizedPath.endsWith(".git") ? normalizedPath : `${normalizedPath}.git`;
}

function decodeRepositoryPathSegment(segment: string): string {
  try {
    return decodeURIComponent(segment.trim());
  } catch {
    return segment.trim();
  }
}

function repositoryNameFromPath(path: string): string {
  const lastSegment = path.split("/").filter(Boolean).at(-1) ?? "repository";
  return lastSegment.replace(/\.git$/i, "") || "repository";
}

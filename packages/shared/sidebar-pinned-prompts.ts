export type SidebarPinnedPrompt = {
  content: string;
  createdAt: string;
  promptId: string;
  title: string;
  updatedAt: string;
};

export function normalizeSidebarPinnedPrompts(candidate: unknown): SidebarPinnedPrompt[] {
  if (!Array.isArray(candidate)) {
    return [];
  }

  const prompts: SidebarPinnedPrompt[] = [];
  for (const item of candidate) {
    if (!isObjectRecord(item)) {
      continue;
    }

    const promptId = typeof item.promptId === "string" ? item.promptId.trim() : "";
    const content = typeof item.content === "string" ? item.content : "";
    const createdAt = typeof item.createdAt === "string" ? item.createdAt : "";
    const titleCandidate = typeof item.title === "string" ? item.title : "";
    const updatedAt = typeof item.updatedAt === "string" ? item.updatedAt : "";
    const title = normalizePinnedPromptTitle(titleCandidate, content);
    if (!promptId || !content || !createdAt || !updatedAt) {
      continue;
    }

    prompts.push({
      content,
      createdAt,
      promptId,
      title,
      updatedAt,
    });
  }

  return prompts.sort((left, right) => comparePinnedPromptDate(right.updatedAt, left.updatedAt));
}

function comparePinnedPromptDate(left: string, right: string): number {
  const leftTime = Date.parse(left);
  const rightTime = Date.parse(right);
  if (!Number.isNaN(leftTime) && !Number.isNaN(rightTime)) {
    return leftTime - rightTime;
  }

  return left.localeCompare(right);
}

function isObjectRecord(candidate: unknown): candidate is Record<string, unknown> {
  return typeof candidate === "object" && candidate !== null;
}

function normalizePinnedPromptTitle(titleCandidate: string, content: string): string {
  const trimmedTitle = titleCandidate.trim();
  if (trimmedTitle) {
    return trimmedTitle;
  }

  const firstContentLine = content
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .find(Boolean);

  if (!firstContentLine) {
    return "Untitled Prompt";
  }

  return firstContentLine.slice(0, 80);
}

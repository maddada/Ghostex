// Empty/loading/error state copy (upstream chat spec §11.7 port + gxserver
// "starting").

export interface SessionChatEmptyStateCopy {
  title: string;
  detail: string;
}

export function sessionChatEmptyStateCopy(
  kind: "loading" | "empty" | "error" | "unsupported" | "starting",
  agentLabel?: string | null,
): SessionChatEmptyStateCopy {
  switch (kind) {
    case "loading":
      return {
        detail: "Reading the agent transcript.",
        title: "Loading conversation…",
      };
    case "empty": {
      const agent = agentLabel?.trim() || "the agent";
      return {
        detail: `Ask ${agent} to inspect code, explain output, or make a change.`,
        title: `Start a chat with ${agent}`,
      };
    }
    case "error":
      return {
        detail:
          "The transcript could not be read. Toggle back to the terminal to keep working.",
        title: "Could not load conversation",
      };
    case "unsupported":
      return {
        detail: "This terminal is not running a recognized coding agent.",
        title: "No conversation here",
      };
    case "starting":
      return sessionChatEmptyStateCopy("loading", agentLabel);
  }
}

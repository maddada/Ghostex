/*
 * Add Project dialog round trips.
 *
 * The real chain is apps/desktop/views/modal-host.tsx:786
 * requestAddProjectDialogOperation → apps/desktop/src/app/remote_conn.rs
 * handle_gpui_add_project_dialog_request_message → gxserver. The sandbox answers
 * the same `{type:"addProjectDialogResult", requestId, ok, result|error}`
 * envelope from the shared story mocks
 * (packages/core-ui/add-project-modal/add-project-modal-mocks.ts createAddProjectStoryMocks),
 * wrapped in the exact result containers the host's readers expect
 * (modal-host.tsx:828-920: `machines`, `project`, `discovery`, `repository`, `job`).
 */
import type { SidebarAddProjectDialogOperation } from "@/packages/shared/session-grid-contract";
import { createAddProjectStoryMocks } from "@/packages/core-ui/add-project-modal/add-project-modal-mocks";
import type { AddProjectProviderId } from "@/packages/core-ui/add-project-modal/types";
import type { ModalHostOutboundMessage } from "../state/types";

export type AddProjectAnswer = {
  ok: boolean;
  result?: unknown;
  error?: string;
  /** True when a project was really registered (the engine bumps projectCount). */
  addedProject: boolean;
};

const mocks = createAddProjectStoryMocks({ cloneRunningPolls: 2, latencyMs: 0 });

function readParams(message: ModalHostOutboundMessage): Record<string, unknown> {
  const params = message.params;
  return params && typeof params === "object" ? (params as Record<string, unknown>) : {};
}

function readString(source: Record<string, unknown>, key: string): string {
  const value = source[key];
  return typeof value === "string" ? value : "";
}

export function readAddProjectOperation(
  message: ModalHostOutboundMessage,
): SidebarAddProjectDialogOperation | undefined {
  const operation = message.operation;
  return typeof operation === "string"
    ? (operation as SidebarAddProjectDialogOperation)
    : undefined;
}

export async function answerAddProjectRequest(
  message: ModalHostOutboundMessage,
): Promise<AddProjectAnswer> {
  const operation = readAddProjectOperation(message);
  const params = readParams(message);
  const machineId = typeof message.machineId === "string" ? message.machineId : "local";
  try {
    switch (operation) {
      case "listMachines":
        return { ok: true, result: { machines: await mocks.listMachineOptions() }, addedProject: false };
      case "browse":
        return {
          ok: true,
          result: await mocks.browse({
            machineId,
            partialPath: readString(params, "partialPath"),
            ...(typeof params.cwd === "string" ? { cwd: params.cwd } : {}),
          }),
          addedProject: false,
        };
      case "createDirectory":
        return {
          ok: true,
          result: await mocks.createDirectory({
            machineId,
            name: readString(params, "name"),
            parentPath: readString(params, "parentPath"),
          }),
          addedProject: false,
        };
      case "add": {
        const project = await mocks.addProject({
          createIfMissing: params.createIfMissing === true,
          machineId,
          path: readString(params, "path"),
        });
        return { ok: true, result: { project }, addedProject: true };
      }
      case "discoverSourceControl":
        return {
          ok: true,
          result: { discovery: await mocks.discoverSourceControl({ machineId }) },
          addedProject: false,
        };
      case "lookupRepository":
        return {
          ok: true,
          result: {
            repository: await mocks.lookupRepository({
              machineId,
              provider: readString(params, "provider") as AddProjectProviderId,
              repository: readString(params, "repository"),
            }),
          },
          addedProject: false,
        };
      case "startClone":
        return {
          ok: true,
          result: {
            job: await mocks.startClone({
              destinationPath: readString(params, "destinationPath"),
              machineId,
              remoteUrl: readString(params, "remoteUrl"),
            }),
          },
          addedProject: false,
        };
      case "readCloneJob": {
        const job = await mocks.readCloneJob({ jobId: readString(params, "jobId"), machineId });
        return { ok: true, result: { job }, addedProject: job.state === "completed" };
      }
      case "cancelCloneJob":
        await mocks.cancelCloneJob?.({ jobId: readString(params, "jobId"), machineId });
        return { ok: true, result: {}, addedProject: false };
      default:
        return {
          ok: false,
          error: `The sandbox does not implement the "${String(operation)}" add-project operation.`,
          addedProject: false,
        };
    }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "The request failed.",
      addedProject: false,
    };
  }
}

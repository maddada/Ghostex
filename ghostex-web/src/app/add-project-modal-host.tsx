/*
 * CDXC:AddProject 2026-07-30:
 * ghostex-web hosts the shared add-project dialog IN PAGE (plan 014 Part D).
 * gpui renders the very same component inside a native child window; the only
 * difference is this file, which fulfils the dialog's callback props with
 * `rpcForMachine` calls against the connection registry.
 *
 * Routing rule: the dialog only ever hands back a bounded `machineId`. This host
 * turns that into a connection, so no base URL, auth token, or SSH detail ever
 * reaches the dialog's props, its state, or anything it logs.
 */

import { AddProjectModal } from "@/sidebar/add-project-modal/add-project-modal";
import type {
  AddProjectAddInput,
  AddProjectAddResult,
  AddProjectBrowseInput,
  AddProjectBrowseResult,
  AddProjectCloneJob,
  AddProjectCloneJobHandle,
  AddProjectCloneJobInput,
  AddProjectCloneStartInput,
  AddProjectCreateDirectoryInput,
  AddProjectCreateDirectoryResult,
  AddProjectMachineOption,
  AddProjectRepositoryInfo,
  AddProjectRepositoryLookupInput,
  AddProjectSourceControlDiscovery,
} from "@/sidebar/add-project-modal/types";
import type {
  GxserverCreateProjectDirectoryResult,
  GxserverDiscoverSourceControlResult,
  GxserverLookupRepositoryResult,
  GxserverProjectDirectoryBrowseResult,
  GxserverProjectDomainState,
  GxserverRepositoryCloneJobRpcResult,
} from "@/shared/gxserver-protocol";
import { useCallback, useEffect, useState } from "react";
import type { OpenAddProjectModalDetail } from "./action-events";
import {
  getConnectionStates,
  rpcForMachine,
} from "../connections/connection-registry";
import { getActiveSidebarProject } from "../sidebar-runtime/active-project-store";

/*
 * The dialog surfaces a "still working" notice at 8s but never gives up on its
 * own: the HOST owns the hard timeout (plan 014, Part B decision 6). A registration
 * that has not answered in a minute is a dead round trip, and the user needs the
 * error text instead of an input that spins forever.
 */
const ADD_PROJECT_TIMEOUT_MS = 60_000;

type AddProjectModalState = OpenAddProjectModalDetail & {
  activeProjectCwd?: string;
};

export function AddProjectModalHost() {
  const [modalState, setModalState] = useState<AddProjectModalState>();

  useEffect(() => {
    const openModal = (
      event: WindowEventMap["ghostex-web:openAddProjectModal"],
    ) => {
      const activeProjectCwd = resolveActiveProjectCwd();
      setModalState({
        ...event.detail,
        ...(activeProjectCwd ? { activeProjectCwd } : {}),
      });
    };
    const closeModal = () => setModalState(undefined);

    window.addEventListener("ghostex-web:openAddProjectModal", openModal);
    window.addEventListener("ghostex-web:closeAppModal", closeModal);
    return () => {
      window.removeEventListener("ghostex-web:openAddProjectModal", openModal);
      window.removeEventListener("ghostex-web:closeAppModal", closeModal);
    };
  }, []);

  const listMachineOptions = useCallback(
    (): Promise<readonly AddProjectMachineOption[]> =>
      Promise.resolve(listConnectedMachineOptions()),
    [],
  );

  const browse = useCallback(
    async (input: AddProjectBrowseInput): Promise<AddProjectBrowseResult> => {
      const result = await rpcForMachine<GxserverProjectDirectoryBrowseResult>(
        input.machineId,
        "/api/browseProjectDirectories",
        {
          ...(input.cwd ? { cwd: input.cwd } : {}),
          partialPath: input.partialPath,
        },
      );
      return { entries: result.entries, parentPath: result.parentPath };
    },
    [],
  );

  const createDirectory = useCallback(
    async (
      input: AddProjectCreateDirectoryInput,
    ): Promise<AddProjectCreateDirectoryResult> => {
      const result = await rpcForMachine<GxserverCreateProjectDirectoryResult>(
        input.machineId,
        "/api/createProjectDirectory",
        { name: input.name, parentPath: input.parentPath },
      );
      return { name: result.name, parentPath: result.parentPath, path: result.path };
    },
    [],
  );

  const addProject = useCallback(
    async (input: AddProjectAddInput): Promise<AddProjectAddResult> => {
      const { project } = await withTimeout(
        rpcForMachine<{ project: GxserverProjectDomainState }>(
          input.machineId,
          "/api/addProjectPath",
          { createIfMissing: input.createIfMissing, path: input.path },
        ),
        "Adding the project timed out. The machine may still be reconnecting.",
      );
      /*
       * `path` is optional on the shared domain type only because quick/chat
       * projects have no workspace root. A project registered through
       * `/api/addProjectPath` always has one, so an answer without it is a
       * broken contract and is surfaced instead of being papered over with the
       * unnormalized input path.
       */
      if (!project.path) {
        throw new Error("gxserver registered the project without a workspace path.");
      }
      return {
        machineId: input.machineId,
        path: project.path,
        projectId: project.projectId,
      };
    },
    [],
  );

  const discoverSourceControl = useCallback(
    async (input: {
      readonly machineId: string;
    }): Promise<AddProjectSourceControlDiscovery> => {
      const { discovery } = await rpcForMachine<GxserverDiscoverSourceControlResult>(
        input.machineId,
        "/api/discoverSourceControl",
      );
      return { providers: discovery.providers };
    },
    [],
  );

  const lookupRepository = useCallback(
    async (
      input: AddProjectRepositoryLookupInput,
    ): Promise<AddProjectRepositoryInfo> => {
      const { repository } = await rpcForMachine<GxserverLookupRepositoryResult>(
        input.machineId,
        "/api/lookupRepository",
        { provider: input.provider, repository: input.repository },
      );
      return repository;
    },
    [],
  );

  const startClone = useCallback(
    async (input: AddProjectCloneStartInput): Promise<AddProjectCloneJobHandle> => {
      const { job } = await withTimeout(
        rpcForMachine<GxserverRepositoryCloneJobRpcResult>(
          input.machineId,
          "/api/startRepositoryClone",
          {
            destinationPath: input.destinationPath,
            remoteUrl: input.remoteUrl,
          },
        ),
        "Starting the clone timed out. The machine may still be reconnecting.",
      );
      return { jobId: job.jobId };
    },
    [],
  );

  const readCloneJob = useCallback(
    async (input: AddProjectCloneJobInput): Promise<AddProjectCloneJob> => {
      const { job } = await rpcForMachine<GxserverRepositoryCloneJobRpcResult>(
        input.machineId,
        "/api/readRepositoryCloneJob",
        { jobId: input.jobId },
      );
      return job;
    },
    [],
  );

  const cancelCloneJob = useCallback(
    async (input: AddProjectCloneJobInput): Promise<void> => {
      await rpcForMachine(input.machineId, "/api/cancelRepositoryCloneJob", {
        jobId: input.jobId,
      });
    },
    [],
  );

  return (
    <AddProjectModal
      activeProjectCwd={modalState?.activeProjectCwd ?? null}
      addProject={addProject}
      browse={browse}
      cancelCloneJob={cancelCloneJob}
      createDirectory={createDirectory}
      discoverSourceControl={discoverSourceControl}
      initialMachineId={modalState?.machineId}
      isOpen={modalState !== undefined}
      listMachineOptions={listMachineOptions}
      lookupRepository={lookupRepository}
      onClose={() => setModalState(undefined)}
      readCloneJob={readCloneJob}
      startClone={startClone}
    />
  );
}

/*
 * Machine options come straight from the connection registry: the local
 * gxserver plus every remote machine whose connection is live. A machine that
 * is only "connecting" cannot answer a browse call, so offering it would just
 * produce a failed round trip inside the dialog.
 */
function listConnectedMachineOptions(): readonly AddProjectMachineOption[] {
  return getConnectionStates()
    .filter((state) => state.status === "connected")
    .map((state) => ({
      label: state.machine.label,
      machineId: state.machine.machineId,
    }))
    .sort((left, right) => {
      if (left.machineId === right.machineId) {
        return 0;
      }
      if (left.machineId === "local") {
        return -1;
      }
      if (right.machineId === "local") {
        return 1;
      }
      return left.label.localeCompare(right.label);
    });
}

/*
 * `./` and `../` queries resolve against the sidebar's active project. The
 * presentation snapshot already carries that project's path, so no extra round
 * trip is needed.
 */
function resolveActiveProjectCwd(): string | undefined {
  const active = getActiveSidebarProject();
  if (!active) {
    return undefined;
  }
  return getConnectionStates()
    .find((state) => state.machine.machineId === active.machineId)
    ?.presentation?.projects.find((project) => project.projectId === active.projectId)
    ?.path;
}

function withTimeout<TResult>(
  operation: Promise<TResult>,
  timeoutMessage: string,
): Promise<TResult> {
  return new Promise<TResult>((resolve, reject) => {
    const timer = window.setTimeout(
      () => reject(new Error(timeoutMessage)),
      ADD_PROJECT_TIMEOUT_MS,
    );
    operation.then(
      (result) => {
        window.clearTimeout(timer);
        resolve(result);
      },
      (error: unknown) => {
        window.clearTimeout(timer);
        reject(error instanceof Error ? error : new Error(String(error)));
      },
    );
  });
}

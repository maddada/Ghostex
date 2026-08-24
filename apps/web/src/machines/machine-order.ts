import type { GhostexWebMachine, MachineConnectionState } from '../connections/types';

/**
 * Apply a complete remote-machine id order without allowing the local machine
 * to move out of its pinned first position or replacing catalog credentials
 * with the sidebar's deliberately smaller settings projection.
 */
export function applyRemoteMachineOrder(
  machines: readonly GhostexWebMachine[],
  orderedRemoteMachineIds: readonly string[]
): readonly GhostexWebMachine[] {
  const localMachines = machines.filter((machine) => machine.machineId === 'local');
  const remoteMachines = machines.filter((machine) => machine.machineId !== 'local');
  if (orderedRemoteMachineIds.length !== remoteMachines.length) {
    return machines;
  }

  const remoteMachineById = new Map(remoteMachines.map((machine) => [machine.machineId, machine]));
  const orderedRemoteMachines: GhostexWebMachine[] = [];
  const seenMachineIds = new Set<string>();
  for (const machineId of orderedRemoteMachineIds) {
    const machine = remoteMachineById.get(machineId);
    if (!machine || seenMachineIds.has(machineId)) {
      return machines;
    }
    seenMachineIds.add(machineId);
    orderedRemoteMachines.push(machine);
  }

  const nextMachines = [...localMachines, ...orderedRemoteMachines];
  return nextMachines.every((machine, index) => machine === machines[index]) ? machines : nextMachines;
}

/** Keep runtime presentation order aligned with the durable machine catalog. */
export function orderMachineConnectionStates(
  states: readonly MachineConnectionState[],
  machines: readonly GhostexWebMachine[]
): readonly MachineConnectionState[] {
  const catalogIndexByMachineId = new Map(machines.map((machine, index) => [machine.machineId, index]));
  const originalIndexByMachineId = new Map(states.map((state, index) => [state.machine.machineId, index]));

  return [...states].sort((left, right) => {
    if (left.machine.machineId === 'local') {
      return right.machine.machineId === 'local' ? 0 : -1;
    }
    if (right.machine.machineId === 'local') {
      return 1;
    }
    const leftIndex = catalogIndexByMachineId.get(left.machine.machineId);
    const rightIndex = catalogIndexByMachineId.get(right.machine.machineId);
    if (leftIndex !== undefined && rightIndex !== undefined) {
      return leftIndex - rightIndex;
    }
    if (leftIndex !== undefined) {
      return -1;
    }
    if (rightIndex !== undefined) {
      return 1;
    }
    return (
      (originalIndexByMachineId.get(left.machine.machineId) ?? 0) -
      (originalIndexByMachineId.get(right.machine.machineId) ?? 0)
    );
  });
}

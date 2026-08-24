import { describe, expect, test } from 'vitest';
import type { GhostexWebMachine, MachineConnectionState } from '../connections/types';
import { applyRemoteMachineOrder, orderMachineConnectionStates } from './machine-order';

function machine(machineId: string): GhostexWebMachine {
  return {
    authToken: `${machineId}-token`,
    baseUrl: `https://${machineId}.example.com`,
    label: machineId,
    machineId,
  };
}

function connection(machineId: string): MachineConnectionState {
  return {
    machine: machine(machineId),
    status: 'connected',
  };
}

describe('web remote machine ordering', () => {
  test('pins local first and applies a complete remote order', () => {
    const local = machine('local');
    const alpha = machine('alpha');
    const beta = machine('beta');

    expect(applyRemoteMachineOrder([local, alpha, beta], ['beta', 'alpha'])).toEqual([local, beta, alpha]);
  });

  test('rejects incomplete or duplicate orders without changing the catalog', () => {
    const machines = [machine('local'), machine('alpha'), machine('beta')];

    expect(applyRemoteMachineOrder(machines, ['alpha'])).toBe(machines);
    expect(applyRemoteMachineOrder(machines, ['alpha', 'alpha'])).toBe(machines);
  });

  test('projects connection state in catalog order while keeping local first', () => {
    const local = machine('local');
    const alpha = machine('alpha');
    const beta = machine('beta');
    const states = [connection('alpha'), connection('local'), connection('beta')];

    expect(orderMachineConnectionStates(states, [local, beta, alpha]).map((state) => state.machine.machineId)).toEqual([
      'local',
      'beta',
      'alpha',
    ]);
  });
});

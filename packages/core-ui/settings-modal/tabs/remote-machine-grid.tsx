import { Switch } from '@/packages/components/ui/switch';
import { IconDeviceDesktop, IconPlus } from '@tabler/icons-react';
import { isRemoteMachineEnabledInSidebar, type RemoteMachineSettings } from '../../../shared/ghostex-settings';
import { formatRemoteMachineSshTarget } from './remote-machine-fields';

/**
 * CDXC:RemotePairing 2026-09-03 DECISION:
 * User: "simplify the machines area here like we simplified it in the mobile app exactly. It has too much info here. No need to show all available machines and a card for creating a new machine. just make it show 4 compact cards with ability to hide a machine from sidebar (disable toggle). and when i click on one of the machines then show that machine's details as a pop up in settings so i can edit it. First compact card needs to be 'Add a machine'".
 * This grid mirrors the mobile MachineCard: icon tile, name, one `user@host` (or "Easy Connect") line, and a Show-in-sidebar switch where the phone has its chevron.
 * The switch is a sibling of the card button, not a child, so flipping it never opens the editor; a hidden machine renders dimmed like the mobile Hidden state.
 * Every saved machine is listed; "4" is the column count of the compact grid, not a cap.
 */
export function RemoteMachineGrid({
  machines,
  onAdd,
  onOpen,
  onSetVisible,
}: {
  machines: RemoteMachineSettings[];
  onAdd: () => void;
  onOpen: (machineId: string) => void;
  onSetVisible: (machineId: string, visible: boolean) => void;
}) {
  return (
    <div className='settings-remote-machine-grid'>
      <button className='settings-remote-machine-tile settings-remote-machine-add-tile' onClick={onAdd} type='button'>
        <span aria-hidden='true' className='settings-remote-machine-tile-icon settings-remote-machine-add-icon'>
          <IconPlus size={18} />
        </span>
        <span className='settings-remote-machine-tile-text'>
          <span className='settings-remote-machine-tile-title'>Add a machine</span>
          <span className='settings-remote-machine-tile-detail'>SSH details or an Easy Connect code</span>
        </span>
      </button>
      {machines.map((machine) => {
        const visible = isRemoteMachineEnabledInSidebar(machine);
        return (
          <div
            className='settings-remote-machine-tile'
            data-hidden={visible ? undefined : ''}
            data-settings-remote-machine-id={machine.id}
            key={machine.id}
          >
            <button
              aria-label={`Edit ${machine.name}`}
              className='settings-remote-machine-tile-main'
              onClick={() => onOpen(machine.id)}
              type='button'
            >
              <span aria-hidden='true' className='settings-remote-machine-tile-icon'>
                <IconDeviceDesktop size={18} />
              </span>
              <span className='settings-remote-machine-tile-text'>
                <span className='settings-remote-machine-tile-title'>{machine.name}</span>
                <span className='settings-remote-machine-tile-detail'>{formatRemoteMachineSshTarget(machine)}</span>
              </span>
            </button>
            <Switch
              aria-label={`Show ${machine.name} in the sidebar`}
              checked={visible}
              className='settings-remote-machine-tile-switch'
              onCheckedChange={(checked) => onSetVisible(machine.id, checked)}
              onClick={(event) => event.stopPropagation()}
              size='sm'
            />
          </div>
        );
      })}
    </div>
  );
}

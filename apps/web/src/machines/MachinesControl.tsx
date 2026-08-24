import { useEffect, useRef, useState, useSyncExternalStore, type FormEvent, type MouseEvent } from 'react';
import { AppTooltip } from '@/packages/core-ui/app-tooltip';
import { getConnectionStates, subscribeConnectionStates } from '../connections/connection-registry';
import type { MachineConnectionStatus } from '../connections/types';
import {
  addMachine,
  getMachineCatalogState,
  initializeMachineCatalog,
  removeMachine,
  subscribeMachineCatalog,
} from './machine-catalog';

export function MachinesControl() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    void initializeMachineCatalog();
  }, []);

  return (
    <>
      <AppTooltip content='Machines'>
        <button
          aria-label='Machines'
          className='web-titlebar__icon-button web-titlebar__machines'
          onClick={() => setOpen(true)}
          type='button'
        >
          <MachinesIcon />
        </button>
      </AppTooltip>
      {open && <MachinesModal onClose={() => setOpen(false)} />}
    </>
  );
}

function MachinesModal({ onClose }: { onClose(): void }) {
  const catalog = useSyncExternalStore(subscribeMachineCatalog, getMachineCatalogState, getMachineCatalogState);
  const connections = useSyncExternalStore(subscribeConnectionStates, getConnectionStates, getConnectionStates);
  const [label, setLabel] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [authToken, setAuthToken] = useState('');
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);
  const labelInput = useRef<HTMLInputElement>(null);
  const connectionByMachineId = new Map(connections.map((connection) => [connection.machine.machineId, connection]));

  useEffect(() => {
    labelInput.current?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setError(undefined);
    setSaving(true);
    try {
      await addMachine({ authToken, baseUrl, label });
      setAuthToken('');
      setBaseUrl('');
      setLabel('');
      labelInput.current?.focus();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const stopPropagation = (event: MouseEvent) => event.stopPropagation();

  return (
    <div className='machines-modal-backdrop' onMouseDown={onClose} role='presentation'>
      <section
        aria-labelledby='machines-modal-title'
        aria-modal='true'
        className='machines-modal'
        onMouseDown={stopPropagation}
        role='dialog'
      >
        <header className='machines-modal__header'>
          <div>
            <h2 id='machines-modal-title'>Machines</h2>
            <p>Connect this browser to additional gxservers.</p>
          </div>
          <AppTooltip content='Close machines'>
            <button aria-label='Close machines' className='machines-modal__close' onClick={onClose} type='button'>
              ×
            </button>
          </AppTooltip>
        </header>

        <div className='machines-list'>
          {catalog.initializing && !catalog.machines.some((machine) => machine.machineId === 'local') && (
            <MachineRow baseUrl='Bootstrapping local gxserver…' label='Local machine' status='connecting' />
          )}
          {catalog.bootstrapError && !catalog.machines.some((machine) => machine.machineId === 'local') && (
            <MachineRow baseUrl={catalog.bootstrapError} label='Local machine' status='disconnected' />
          )}
          {catalog.machines.map((machine) => {
            const connection = connectionByMachineId.get(machine.machineId);
            return (
              <MachineRow
                baseUrl={connection?.error ?? machine.baseUrl}
                key={machine.machineId}
                label={machine.label}
                onRemove={machine.machineId === 'local' ? undefined : () => removeMachine(machine.machineId)}
                status={connection?.status ?? 'connecting'}
              />
            );
          })}
        </div>

        <form className='machines-form' onSubmit={submit}>
          <h3>Add machine</h3>
          <label>
            Name
            <input
              autoComplete='off'
              onChange={(event) => setLabel(event.target.value)}
              placeholder='Build server'
              ref={labelInput}
              value={label}
            />
          </label>
          <label>
            Base URL
            <input
              autoCapitalize='none'
              autoComplete='url'
              onChange={(event) => setBaseUrl(event.target.value)}
              placeholder='http://127.0.0.1:58746'
              spellCheck={false}
              type='url'
              value={baseUrl}
            />
          </label>
          <label>
            Auth token
            <input
              autoCapitalize='none'
              autoComplete='off'
              onChange={(event) => setAuthToken(event.target.value)}
              spellCheck={false}
              type='password'
              value={authToken}
            />
          </label>
          {error && (
            <p className='machines-form__error' role='alert'>
              {error}
            </p>
          )}
          <button className='machines-form__submit' disabled={saving} type='submit'>
            {saving ? 'Checking…' : 'Add machine'}
          </button>
        </form>
      </section>
    </div>
  );
}

function MachineRow({
  baseUrl,
  label,
  onRemove,
  status,
}: {
  baseUrl: string;
  label: string;
  onRemove?: () => void;
  status: MachineConnectionStatus;
}) {
  return (
    <div className='machines-list__row'>
      <AppTooltip content={status}>
        <span aria-label={status} className={`machines-status machines-status--${status}`} />
      </AppTooltip>
      <div className='machines-list__identity'>
        <strong>{label}</strong>
        <AppTooltip content={baseUrl}>
          <span>{baseUrl}</span>
        </AppTooltip>
      </div>
      {onRemove && (
        <button className='machines-list__remove' onClick={onRemove} type='button'>
          Remove
        </button>
      )}
    </div>
  );
}

function MachinesIcon() {
  return (
    <svg aria-hidden='true' viewBox='0 0 24 24'>
      <rect height='6' rx='1' width='16' x='4' y='4' />
      <rect height='6' rx='1' width='16' x='4' y='14' />
      <path d='M8 7h.01M8 17h.01M12 7h5M12 17h5' />
    </svg>
  );
}

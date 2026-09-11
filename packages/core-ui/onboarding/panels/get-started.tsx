import { useState } from 'react';
import type { PreferredAgentInterface } from '@/packages/shared/ghostex-settings';
import {
  defaultAgentId as resolveDefaultAgentId,
  finishOnboarding,
  installedAgents,
  type PanelProps,
} from '../onboarding-state';
import { Cta, Eyebrow, Heading, Icon, Spinner, Sub } from '../primitives';
import { box } from '../stage';

const SESSION_VIEWS: readonly (readonly [PreferredAgentInterface, string, string])[] = [
  ['chat', 'Chat', 'Cleaner agent conversation'],
  ['terminal', 'Terminal', 'Raw CLI interface'],
];
const CARD_LEFT = 506;
const CARD_WIDTH = 660;
const TILE_GAP = 8;

export function GetStartedPanel({ props, flow, setFlow }: PanelProps) {
  const { settings, agents, pickedProjectFolder, hasProjects } = props;
  const defaultAgent = resolveDefaultAgentId(settings, agents);
  const installed = installedAgents(agents);
  const ordered = defaultAgent
    ? [
        installed.find((agent) => agent.agentId === defaultAgent)!,
        ...installed.filter((agent) => agent.agentId !== defaultAgent),
      ]
    : installed;
  const tiles: readonly { id: string; name: string; detail: string }[] = [
    ...ordered.map((agent) => ({
      id: agent.agentId,
      name: agent.name,
      detail: agent.agentId === defaultAgent ? 'Default agent' : 'Switch anytime',
    })),
    { id: 'terminal', name: 'Terminal', detail: 'No agent yet' },
  ];
  const startWith = flow.startWith ?? defaultAgent ?? 'terminal';
  const sessionView = settings?.preferredAgentInterface ?? 'chat';
  const folder = pickedProjectFolder?.trim() ?? '';
  const canOpen = folder.length > 0 || hasProjects === true;
  /** "Open Ghostex" is a host round trip: busy until the project and session exist, error stays on the panel. */
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<string>();

  const tilePitch = (CARD_WIDTH + TILE_GAP) / tiles.length;
  const viewPitch = (CARD_WIDTH + TILE_GAP) / SESSION_VIEWS.length;

  const setSessionView = (view: PreferredAgentInterface) => {
    if (!settings) return;
    props.onChange({ ...settings, preferredAgentInterface: view });
  };
  const openGhostex = () => {
    if (folder) {
      if (opening) return;
      setOpening(true);
      setOpenError(undefined);
      props.onFinishFirstLaunch({ agentId: startWith, path: folder }).then(
        () => setFlow({ finishedPath: folder, finished: true }),
        (error: unknown) => {
          setOpening(false);
          setOpenError(error instanceof Error && error.message ? error.message : 'Ghostex could not open the project.');
        }
      );
      return;
    }
    if (hasProjects) finishOnboarding(props, flow);
  };
  const advancedLater = () => {
    props.onOpenSettings?.();
    props.onClose();
  };

  return (
    <>
      <Eyebrow x={486} y={150} w={700}>
        Get started
      </Eyebrow>
      <Heading x={336} y={182} w={1000} size={48} center l1='Open your first project in Ghostex.' />
      <Sub x={486} y={252} w={700} size={16.5} center>
        One folder, one agent, one default view. Everything else can change later.
      </Sub>
      <div className='glass pcard' style={box(486, 310, 700, 350)} />
      <div className='label' style={{ position: 'absolute', left: CARD_LEFT, top: 328 }}>
        Project folder
      </div>
      <div className='pfield' style={box(CARD_LEFT, 354, CARD_WIDTH, 50)}>
        <Icon n='folder' size={22} className='dimc2' />
        <span className={'path' + (folder ? '' : ' dim')}>{folder || 'Choose a folder to start in'}</span>
        <button type='button' className='choose' onClick={props.onPickProjectFolder}>
          Choose folder
        </button>
      </div>
      <div className='label' style={{ position: 'absolute', left: CARD_LEFT, top: 428 }}>
        Start with
      </div>
      {tiles.map((tile, index) => (
        <button
          key={tile.id}
          type='button'
          className={'opt' + (startWith === tile.id ? ' sel' : '')}
          style={box(CARD_LEFT + index * tilePitch, 452, tilePitch - TILE_GAP, 72)}
          onClick={() => setFlow({ startWith: tile.id })}
        >
          <span className='nm'>{tile.name}</span>
          <span className='ss'>{tile.detail}</span>
        </button>
      ))}
      <div className='label' style={{ position: 'absolute', left: CARD_LEFT, top: 544 }}>
        Default session view
      </div>
      {SESSION_VIEWS.map(([id, name, detail], index) => (
        <button
          key={id}
          type='button'
          className={'opt' + (sessionView === id ? ' sel' : '')}
          style={box(CARD_LEFT + index * viewPitch, 568, viewPitch - TILE_GAP, 72)}
          onClick={() => setSessionView(id)}
        >
          <span className='nm'>{name}</span>
          <span className='ss'>{detail}</span>
        </button>
      ))}
      <div className='actions center' style={{ position: 'absolute', left: 486, top: 692, width: 700 }}>
        <Cta
          filled
          style={{ height: 53, padding: '0 22px', fontSize: 17 }}
          onClick={openGhostex}
          disabled={!canOpen || opening}
          arrow={!opening}
        >
          {opening ? (
            <>
              <Spinner /> Opening…
            </>
          ) : (
            'Open Ghostex'
          )}
        </Cta>
        <button
          type='button'
          className='ghost'
          style={{ marginLeft: 22, fontSize: 17 }}
          onClick={advancedLater}
          disabled={opening}
        >
          Advanced settings later
        </button>
      </div>
      <Sub x={486} y={772} w={700} size={15} center>
        {openError ? (
          <span style={{ color: '#ff6b62' }} role='alert'>
            {openError}
          </span>
        ) : opening ? (
          'Adding the project and opening its first session…'
        ) : canOpen ? (
          "That's it. The workspace teaches the deeper features once you are inside."
        ) : (
          'Choose a folder above to open your first project.'
        )}
      </Sub>
    </>
  );
}

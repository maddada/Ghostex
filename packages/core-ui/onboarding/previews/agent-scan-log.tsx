import { useEffect, useRef, useState } from 'react';
import type { OnboardingDetectedAgent } from '../contract';
import { installedAgents } from '../onboarding-state';
import { Icon, Spinner } from '../primitives';
import { box, clockStamp } from '../stage';

export type ScanExtraLine = { id: string; text: string; cls: 'acc' | 'ok' | 'wait'; at: Date };

type ScanLine = { id: string; text: string; at: Date; ok?: boolean; done?: boolean; agentId?: string };

const LINE_STAGGER_MS = 360;

/**
 * CDXC:Onboarding 2026-09-11 WHY:
 * The prototype replayed a scripted scan with fake timestamps. Here every line is stamped with the real clock at the
 * moment it is appended, the "Found" lines come from the host's detection result, and the stagger only paces the
 * reveal; nothing is shown as found before the host reported it. The host probes providers one at a time and
 * re-sends `agents` after each, so "Found" lines are appended as agents arrive and "Scan complete." waits for
 * `loading` to end (the walk's final payload), not for the first partial result. On a rescan the previous result
 * is still in `agents`; nothing from it is printed until the host sends a new array.
 */
export function AgentScanLog({
  agents,
  loading,
  defaultAgentId,
  extras,
  onRescan,
  rescanDisabled,
}: {
  agents: readonly OnboardingDetectedAgent[];
  loading: boolean;
  defaultAgentId?: string;
  extras: readonly ScanExtraLine[];
  onRescan: () => void;
  rescanDisabled?: boolean;
}) {
  const [lines, setLines] = useState<ScanLine[]>([]);
  const [revealing, setRevealing] = useState(false);
  const timers = useRef<number[]>([]);
  /** Wall-clock time at which the next queued line may appear (keeps the stagger across separate enqueues). */
  const nextAt = useRef(0);
  const foundIds = useRef(new Set<string>());
  const hasStarted = useRef(false);
  const wasLoading = useRef(false);
  /** The `agents` array present when the current scan started; stale until the host replaces it. */
  const agentsAtStart = useRef<readonly OnboardingDetectedAgent[] | null>(null);

  useEffect(() => () => timers.current.forEach((timer) => window.clearTimeout(timer)), []);

  const push = (line: Omit<ScanLine, 'at'>) => setLines((current) => [...current, { ...line, at: new Date() }]);
  const enqueue = (steps: readonly (() => void)[]) => {
    if (steps.length === 0) return;
    const base = Math.max(Date.now(), nextAt.current);
    steps.forEach((step, index) => {
      timers.current.push(window.setTimeout(step, base + index * LINE_STAGGER_MS - Date.now()));
    });
    nextAt.current = base + steps.length * LINE_STAGGER_MS;
    const revealEnd = nextAt.current;
    setRevealing(true);
    timers.current.push(
      window.setTimeout(() => {
        if (nextAt.current <= revealEnd) setRevealing(false);
      }, revealEnd - Date.now())
    );
  };
  const reset = () => {
    timers.current.forEach((timer) => window.clearTimeout(timer));
    timers.current = [];
    nextAt.current = 0;
    foundIds.current.clear();
    setRevealing(false);
    setLines([]);
  };
  const startSteps = (): (() => void)[] => [
    () => push({ id: 'start', text: 'Starting the scan...' }),
    () => push({ id: 'check', text: 'Checking installed agent CLIs...' }),
  ];
  const foundSteps = (): (() => void)[] =>
    installedAgents(agents)
      .filter((agent) => !foundIds.current.has(agent.agentId))
      .map((agent) => {
        foundIds.current.add(agent.agentId);
        return () =>
          push({ id: 'found-' + agent.agentId, text: `Found ${agent.name}`, ok: true, agentId: agent.agentId });
      });
  const doneStep = () => push({ id: 'done', text: 'Scan complete.', done: true });

  useEffect(() => {
    const started = loading && !wasLoading.current;
    const ended = !loading && wasLoading.current;
    wasLoading.current = loading;
    if (started) {
      hasStarted.current = true;
      agentsAtStart.current = agents;
      reset();
      enqueue(startSteps());
      return;
    }
    if (loading) return;
    if (!hasStarted.current) {
      hasStarted.current = true;
      reset();
      enqueue([...startSteps(), ...foundSteps(), doneStep]);
      return;
    }
    if (ended) enqueue([...foundSteps(), doneStep]);
  }, [loading]);

  useEffect(() => {
    if (!loading || !hasStarted.current || agents === agentsAtStart.current) return;
    enqueue(foundSteps());
  }, [agents, loading]);

  const scanning = loading || revealing;
  return (
    <div className='glass term' style={box(885, 174, 700, 586)}>
      <div className='term-head'>
        <span>Looking for agents</span>
        <span className={'term-state' + (scanning ? '' : ' done')}>
          {scanning ? <Spinner /> : <Icon n='checkCircle' size={16} />}
          {scanning ? 'Scanning...' : 'Complete'}
        </span>
        <button type='button' className='rescan' onClick={onRescan} disabled={scanning || rescanDisabled}>
          <Icon n='refresh' size={15} className={loading ? 'spin' : ''} />
          Rescan
        </button>
      </div>
      <div className='term-lines'>
        {lines.map((line) => (
          <div
            key={line.id}
            className={
              'tl' + (line.done ? ' done' : '') + (line.agentId && line.agentId === defaultAgentId ? ' hot' : '')
            }
          >
            <span className='ts'>[{clockStamp(line.at)}]</span>
            <span className='tt'>{line.text}</span>
            {line.ok && <Icon n='check' size={17} className='tick' sw={2} />}
          </div>
        ))}
        {!scanning &&
          extras.map((line) => (
            <div key={line.id + line.at.getTime()} className={'tl extra ' + line.cls}>
              <span className='ts'>[{clockStamp(line.at)}]</span>
              <span className='tt'>{line.text}</span>
            </div>
          ))}
        {scanning && <span className='caret' />}
      </div>
    </div>
  );
}

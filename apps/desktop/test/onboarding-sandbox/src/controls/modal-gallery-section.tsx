/*
 * Force-open gallery: one button per SandboxModalKind, grouped. Force-opens
 * bypass the single app-modal slot the real app enforces, so the engine emits a
 * note whenever one is used.
 *
 * Payloads mirror the required fields of OpenAppModalMessage
 * (packages/core-ui/app-modal-host-bridge.ts) — modals whose payload is mandatory would
 * otherwise never render.
 */
import { useSandboxStore } from '../state/store';
import type { SandboxModalKind } from '../state/types';
import { Note, Section } from './control-primitives';

interface GalleryEntry<K extends SandboxModalKind = SandboxModalKind> {
  kind: K;
  label: string;
  note?: string;
  payload?: Record<string, unknown>;
}

function entry<K extends SandboxModalKind>(value: GalleryEntry<K>): GalleryEntry<K> {
  return value;
}

const SAMPLE_PROJECT = {
  projectId: 'sandbox-project',
  projectName: 'Ghostex',
  projectPath: '/Users/sandbox/dev/ghostex',
};

const MODAL_GALLERY_GROUPS = [
  {
    title: 'Onboarding',
    entries: [
      entry({ kind: 'firstLaunchSetup', label: 'firstLaunchSetup', note: '1120×850 · “Ghostex Tips”' }),
      entry({ kind: 'discoverGhostex', label: 'discoverGhostex', note: 'tour only' }),
      entry({
        kind: 'discoverGhostex',
        label: 'discoverGhostex → setup',
        note: 'showFirstLaunchSetupOnClose',
        payload: { showFirstLaunchSetupOnClose: true },
      }),
      entry({ kind: 'watchGhostexVideo', label: 'watchGhostexVideo', note: 'auto-opened on first run' }),
      entry({ kind: 'tipsAndTricks', label: 'tipsAndTricks' }),
    ],
  },
  {
    title: 'Projects',
    entries: [
      entry({ kind: 'addProject', label: 'addProject' }),
      entry({ kind: 'addRepository', label: 'addRepository' }),
      entry({ kind: 'missingProjectFolder', label: 'missingProjectFolder', payload: SAMPLE_PROJECT }),
      entry({
        kind: 'remoteProjectPicker',
        label: 'remoteProjectPicker',
        payload: { remoteMachineId: 'sandbox-remote', remoteMachineName: 'sandbox-remote' },
      }),
      entry({
        kind: 'remoteGxserverInstall',
        label: 'remoteGxserverInstall',
        payload: { remoteMachineId: 'sandbox-remote', remoteMachineName: 'sandbox-remote' },
      }),
      entry({
        kind: 'worktree',
        label: 'worktree',
        payload: {
          projectId: SAMPLE_PROJECT.projectId,
          projectName: SAMPLE_PROJECT.projectName,
          projectPath: SAMPLE_PROJECT.projectPath,
        },
      }),
      entry({ kind: 'deleteWorktree', label: 'deleteWorktree' }),
      entry({ kind: 'recentProjects', label: 'recentProjects' }),
    ],
  },
  {
    title: 'Settings & config',
    entries: [
      entry({ kind: 'settings', label: 'settings' }),
      entry({ kind: 'settings', label: 'settings · agents tab', payload: { initialTab: 'agents' } }),
      entry({ kind: 'configureAgents', label: 'configureAgents' }),
      entry({ kind: 'configureActions', label: 'configureActions' }),
      entry({ kind: 'agentConfig', label: 'agentConfig', note: 'needs an agentDraft' }),
      entry({ kind: 'agentsHub', label: 'agentsHub', note: 'loads Monaco' }),
      entry({ kind: 'hotkeys', label: 'hotkeys' }),
    ],
  },
  {
    title: 'Misc',
    entries: [
      entry({
        kind: 'portlessSetup',
        label: 'portlessSetup · firstSetup',
        note: 'compile-time disabled in the real app',
        payload: { mode: 'firstSetup', protocol: 'https' },
      }),
      entry({
        kind: 'portlessSetup',
        label: 'portlessSetup · reconfigure',
        payload: { mode: 'standaloneReconfigure', protocol: 'http' },
      }),
      entry({ kind: 'updateAvailable', label: 'updateAvailable' }),
      entry({ kind: 'commandPalette', label: 'commandPalette' }),
      entry({ kind: 'daemonSessions', label: 'daemonSessions' }),
      entry({ kind: 'previousSessions', label: 'previousSessions' }),
      entry({ kind: 'openTargets', label: 'openTargets' }),
      entry({ kind: 'pinnedPrompts', label: 'pinnedPrompts' }),
      entry({ kind: 'stashedPrompts', label: 'stashedPrompts' }),
      entry({ kind: 'scratchPad', label: 'scratchPad' }),
      entry({ kind: 'gitCommit', label: 'gitCommit', note: 'needs a gitCommitDraft' }),
      entry({ kind: 'gitFileDiff', label: 'gitFileDiff' }),
      entry({
        kind: 'renameSession',
        label: 'renameSession',
        payload: { initialTitle: 'sandbox session', sessionId: 'sandbox-session' },
      }),
      entry({
        kind: 'delayedSend',
        label: 'delayedSend',
        payload: { sessionId: 'sandbox-session' },
      }),
      entry({
        kind: 'firstUserMessage',
        label: 'firstUserMessage',
        payload: { message: 'Hello from the onboarding sandbox.' },
      }),
    ],
  },
];

/* Compile-time guard: every SandboxModalKind must appear in the gallery. */
type CoveredModalKind = (typeof MODAL_GALLERY_GROUPS)[number]['entries'][number]['kind'];
type UncoveredModalKind = Exclude<SandboxModalKind, CoveredModalKind>;
const galleryCoversEveryModalKind: [UncoveredModalKind] extends [never] ? true : never = true;
void galleryCoversEveryModalKind;

export function ModalGallerySection() {
  const forceOpenModal = useSandboxStore((s) => s.forceOpenModal);
  const modalWindows = useSandboxStore((s) => s.modalWindows);

  return (
    <Section
      badge={modalWindows.length > 0 ? `${modalWindows.length} open` : undefined}
      defaultOpen={false}
      id='modal-gallery'
      title='Modal gallery'
    >
      <Note>Force-opens bypass the single app-modal slot; the real app can only ever have one window.</Note>
      {MODAL_GALLERY_GROUPS.map((group) => (
        <div className='cp-gallery-group' key={group.title}>
          <div className='cp-group-label'>{group.title}</div>
          <div className='cp-gallery-grid'>
            {group.entries.map((galleryEntry) => (
              <button
                className='cp-gallery-btn'
                key={`${galleryEntry.kind}:${galleryEntry.label}`}
                onClick={() =>
                  forceOpenModal(
                    galleryEntry.kind,
                    galleryEntry.payload === undefined ? undefined : { ...galleryEntry.payload }
                  )
                }
                title={galleryEntry.note ?? galleryEntry.kind}
                type='button'
              >
                <span className='cp-gallery-label'>{galleryEntry.label}</span>
                {galleryEntry.note === undefined ? null : <span className='cp-gallery-note'>{galleryEntry.note}</span>}
              </button>
            ))}
          </div>
        </div>
      ))}
    </Section>
  );
}

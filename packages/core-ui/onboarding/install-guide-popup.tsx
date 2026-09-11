import { ONBOARDING_INSTALL_GUIDE_URL } from './onboarding-state';
import { AgentLogo, Cta, Icon, Popup } from './primitives';

const INSTALL_ROWS: readonly (readonly [string, string, string, string])[] = [
  ['Gemini CLI', 'gemini', 'npm install -g @google/gemini-cli', 'https://github.com/google-gemini/gemini-cli'],
  ['OpenCode', 'opencode', 'curl -fsSL https://opencode.ai/install | bash', 'https://opencode.ai/docs'],
  ['Pi', 'pi', 'npm install -g @mariozechner/pi-coding-agent', 'https://github.com/badlogic/pi-mono'],
];

export function InstallGuidePopup({
  onClose,
  onLater,
  toast,
  onOpenUrl,
  onOpenGuide,
}: {
  onClose: () => void;
  /** Queue the guide for after onboarding; omitted on the finished screen, where the guide opens right away. */
  onLater?: () => void;
  toast: (message: string) => void;
  onOpenUrl: (url: string) => void;
  onOpenGuide: (url: string) => void;
}) {
  const copy = (command: string) => {
    navigator.clipboard?.writeText(command).catch(() => undefined);
    toast(`Copied ${command}`);
  };
  return (
    <Popup title='Install another agent' onClose={onClose} width={580}>
      <p className='modal-p'>
        Ghostex runs any agent CLI already installed on your computer. Install one, then{' '}
        {onLater ? 'press Rescan' : 'rescan in Settings → Agents'}.
      </p>
      <div className='install-list'>
        {INSTALL_ROWS.map(([name, id, command, url]) => (
          <div key={name} className='install-row'>
            <AgentLogo agentId={id} size={18} />
            <button type='button' className='nm link' onClick={() => onOpenUrl(url)} title={url}>
              {name}
            </button>
            <code>{command}</code>
            <button
              type='button'
              className='icon-btn'
              onClick={() => copy(command)}
              aria-label={`Copy ${name} install command`}
            >
              <Icon n='copy' size={15} />
            </button>
          </div>
        ))}
      </div>
      <p className='modal-p dim'>20+ more are listed in Settings → Agents.</p>
      <div className='modal-actions'>
        <button type='button' className='ghost guide-link' onClick={() => onOpenGuide(ONBOARDING_INSTALL_GUIDE_URL)}>
          <Icon n='external' size={14} />
          Full install guide
        </button>
        {onLater ? (
          <>
            <button type='button' className='ghost' onClick={onClose}>
              Close
            </button>
            <Cta filled arrow={false} className='sm' onClick={onLater}>
              Open after onboarding
            </Cta>
          </>
        ) : (
          <Cta filled arrow={false} className='sm' onClick={onClose}>
            Done
          </Cta>
        )}
      </div>
    </Popup>
  );
}

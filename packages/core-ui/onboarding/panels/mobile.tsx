import { GHOSTEX_ANDROID_APK_URL } from '@/packages/shared/sidebar-commands';
import type { PanelProps } from '../onboarding-state';
import { PairingPreview } from '../previews/phone-pairing';
import { Cta, Eyebrow, Heading, Icon, Sub, Toggle, buttonProps } from '../primitives';
import { box } from '../stage';

const STEPS: readonly (readonly [string, string])[] = [
  ['Install Ghostex Mobile', 'Get Ghostex Mobile for Android or iPhone.'],
  ['Pair this computer', 'Opens Settings → Remote after setup: turn on Easy Connect and scan the QR code.'],
  ['Keep working remotely', 'Reply, steer and keep working from your phone.'],
];

export function MobilePanel({ props, flow, setFlow, go, toast }: PanelProps) {
  const { settings } = props;
  const notify = settings?.showMacOSAttentionNotifications ?? true;
  const done = [flow.mobileInstallOpened, flow.mobilePairingOpened, false];
  const currentStep = done.findIndex((value) => !value);
  const stepActions: readonly (() => void)[] = [
    () => {
      setFlow({ mobileInstallOpened: true });
      props.onOpenExternalUrl(GHOSTEX_ANDROID_APK_URL);
    },
    () => {
      // Queued, not opened: Settings replaces this window, so Remote opens when the flow finishes.
      setFlow({ mobilePairingOpened: true, phoneQueued: true });
      toast("We'll open Remote settings when you finish");
    },
    () => toast('Your phone shows every session once it is paired'),
  ];
  const toggleNotify = () => {
    if (!settings) return;
    props.onChange({ ...settings, showMacOSAttentionNotifications: !notify });
  };
  return (
    <>
      <Eyebrow x={46} y={146}>
        Optional · Mobile
      </Eyebrow>
      <Heading x={46} y={174} w={700} size={52} l1='Take the session with you.' />
      <Sub x={46} y={250} w={660} size={16}>
        The agents keep running on your computer while you reply, steer and keep working from your phone.
      </Sub>
      {STEPS.map(([title, detail], index) => (
        <div
          key={title}
          className={'glass mstep' + (done[index] ? ' done' : currentStep === index ? ' on' : '')}
          style={box(46, 326 + index * 92, 660, 84)}
          aria-current={currentStep === index ? 'step' : undefined}
          {...buttonProps(stepActions[index])}
        >
          <span className='mnum'>{done[index] ? <Icon n='check' size={18} sw={2.2} /> : index + 1}</span>
          <div>
            <div className='nm'>{title}</div>
            <div className='ss'>{detail}</div>
          </div>
        </div>
      ))}
      <div
        className={'glass vrow' + (notify ? ' on' : '')}
        style={box(46, 608, 660, 72)}
        {...buttonProps(toggleNotify)}
      >
        <Icon n='bell' size={22} className='vicon' />
        <div>
          <div className='nm lg'>Ping me when an agent needs me</div>
          <div className='ss'>
            {notify
              ? 'This computer and your phone alert you when an agent needs an answer.'
              : 'No alerts on this computer or your phone: you find out when you look.'}
          </div>
        </div>
        <Toggle on={notify} onClick={toggleNotify} label='Ping me when an agent needs me' />
      </div>
      <div className='actions' style={{ position: 'absolute', left: 46, top: 704 }}>
        <Cta
          filled
          style={{ height: 52, padding: '0 24px', fontSize: 17 }}
          onClick={flow.phoneQueued ? () => go(5) : stepActions[1]}
        >
          {flow.phoneQueued ? 'Continue' : 'Connect my phone'}
        </Cta>
        <button type='button' className='ghost dimmer' style={{ marginLeft: 26, fontSize: 17 }} onClick={() => go(5)}>
          Not now
        </button>
      </div>
      <p className='note' style={{ position: 'absolute', left: 46, top: 786 }}>
        {flow.phoneQueued ? (
          <>
            <b>Settings → Remote</b> opens when you finish setup.
          </>
        ) : (
          <>
            You can set this up anytime from <b>Settings → Remote</b>.
          </>
        )}
      </p>
      <PairingPreview notify={notify} toast={toast} />
    </>
  );
}

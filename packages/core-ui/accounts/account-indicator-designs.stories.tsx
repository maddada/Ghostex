import { useState } from 'react';
import { ProjectAgentLauncherIcon } from '../project-agent-launcher-icon';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { IconChevronDown, IconDots, IconSettings } from '@tabler/icons-react';
import { Switch } from '@/packages/components/ui/switch';
import { SettingsInput } from '../settings-modal/fields';
import { AGENT_LOGOS, getBrandAgentLogoStyle } from '../agent-logos';
import { normalizeAccountIndicatorInput, type AccountProvider } from '@/packages/shared/agent-accounts';
import './account-indicator-designs.css';

type Design = 'paired' | 'capsule' | 'monogram' | 'stacked' | 'tab' | 'keycap' | 'wordmark' | 'rail' | 'usage' | 'type';
type Account = { id: string; provider: AccountProvider; label: string; name: string; first: string; second: string };
type DesignSpec = { id: Design; name: string; description: string; width: string };
const designs: DesignSpec[] = [
  {
    id: 'paired',
    name: 'Paired tiles',
    description: 'Separate the provider and account into two small tiles. No overlap.',
    width: '86 px',
  },
  {
    id: 'capsule',
    name: 'Quiet capsule',
    description: 'A single soft capsule holds the logo and label beside the usage figures.',
    width: '90 px',
  },
  {
    id: 'monogram',
    name: 'Account first',
    description: 'Give the account label the strongest shape. Keep the provider small and separate.',
    width: '78 px',
  },
  {
    id: 'stacked',
    name: 'Label underneath',
    description: 'Stack the logo above its label, with both usage figures beside it.',
    width: '64 px',
  },
  {
    id: 'tab',
    name: 'Account tabs',
    description: 'Use a short label and a bright underline to identify the selected account.',
    width: '76 px',
  },
  {
    id: 'keycap',
    name: 'Split key',
    description: 'Join provider and label in one compact, lightly raised key.',
    width: '90 px',
  },
  {
    id: 'wordmark',
    name: 'Provider + label',
    description: 'Spell out the provider with the account label. More width, less decoding.',
    width: '110 px',
  },
  {
    id: 'rail',
    name: 'Side label',
    description: 'A narrow account rail anchors the logo and usage without floating elements.',
    width: '76 px',
  },
  {
    id: 'usage',
    name: 'Usage first',
    description: 'Lead with the two figures, followed by a restrained provider and account signature.',
    width: '80 px',
  },
  {
    id: 'type',
    name: 'Type only',
    description: 'Replace the logo with a clear account label and a small provider name.',
    width: '74 px',
  },
];
const samples: Account[] = [
  { id: 'claude-personal', provider: 'claude', label: '0', name: 'Personal', first: '0%', second: '0%' },
  { id: 'claude-work', provider: 'claude', label: 'cw', name: 'Work', first: '7%', second: '13%' },
  { id: 'claude-research', provider: 'claude', label: 'r', name: 'Research', first: '0%', second: '0%' },
  { id: 'claude-backup', provider: 'claude', label: '2', name: 'Backup', first: '17%', second: '47%' },
  { id: 'codex-work', provider: 'codex', label: 'xw', name: 'Work', first: '9%', second: '2rs' },
  { id: 'codex-personal', provider: 'codex', label: '2', name: 'Personal', first: '0%', second: '2rs' },
];
const providerName = (provider: AccountProvider) => (provider === 'claude' ? 'Claude' : 'Codex');

function ProviderIcon({ provider, brand }: { provider: AccountProvider; brand: boolean }) {
  return (
    <span
      aria-hidden='true'
      className='aid-logo'
      style={
        brand
          ? getBrandAgentLogoStyle(provider)
          : {
              backgroundColor: 'currentColor',
              maskImage: 'url("' + AGENT_LOGOS[provider] + '")',
              WebkitMaskImage: 'url("' + AGENT_LOGOS[provider] + '")',
            }
      }
    />
  );
}

function AccountChip({
  account,
  design,
  selected,
  brand,
  onSelect,
}: {
  account: Account;
  design: Design;
  selected: boolean;
  brand: boolean;
  onSelect: () => void;
}) {
  const label = account.label === '-' ? '' : account.label;
  const logo = <ProviderIcon provider={account.provider} brand={brand} />;
  const mark = label ? <span className='aid-mark'>{label}</span> : null;
  const usage = (
    <span className='aid-usage'>
      <span>{account.first}</span>
      <span>{account.second}</span>
    </span>
  );
  const word = <span className='aid-provider-name'>{providerName(account.provider)}</span>;
  const identity =
    design === 'type' ? (
      <span className='aid-identity'>
        {mark}
        {word}
      </span>
    ) : design === 'wordmark' ? (
      <span className='aid-identity'>
        {logo}
        <span>
          {word}
          {mark}
        </span>
      </span>
    ) : (
      <span className='aid-identity'>
        {logo}
        {mark}
      </span>
    );
  return (
    <button
      type='button'
      className={'aid-chip aid-variant-' + design}
      aria-pressed={selected}
      aria-label={providerName(account.provider) + ' ' + account.name + ', ' + account.first + ', ' + account.second}
      title={providerName(account.provider) + ' / ' + account.name + ' / ' + (label || 'indicator hidden')}
      onClick={onSelect}
    >
      {design === 'usage' ? (
        <>
          {usage}
          {identity}
        </>
      ) : (
        <>
          {identity}
          {usage}
        </>
      )}
    </button>
  );
}

function Strip({
  spec,
  accounts,
  selected,
  brand,
  onSelect,
}: {
  spec: DesignSpec;
  accounts: Account[];
  selected: string;
  brand: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <div className='aid-strip-scroll'>
      <div className='aid-strip' role='group' aria-label={spec.name + ' accounts'}>
        {accounts.map((account) => (
          <AccountChip
            key={account.id}
            account={account}
            design={spec.id}
            selected={selected === account.id}
            brand={brand}
            onSelect={() => onSelect(account.id)}
          />
        ))}
        <span className='aid-toolbar-end' aria-hidden='true'>
          <IconSettings size={15} />
          <IconDots size={16} />
        </span>
      </div>
    </div>
  );
}

function DesignStudy({
  design,
  label = 'cw',
  brandIcons = false,
}: {
  design?: Design;
  label?: string;
  brandIcons?: boolean;
}) {
  const [editedLabel, setEditedLabel] = useState<string | undefined>();
  const [editedBrand, setEditedBrand] = useState<boolean | undefined>();
  const [selected, setSelected] = useState('claude-work');
  const currentLabel = editedLabel ?? label;
  const currentBrand = editedBrand ?? brandIcons;
  const accounts = samples.map((account) =>
    account.id === 'claude-work'
      ? {
          ...account,
          label: normalizeAccountIndicatorInput(currentLabel) || '1',
        }
      : account
  );
  const shown = design ? designs.filter((spec) => spec.id === design) : designs;
  const selectedAccount = accounts.find((account) => account.id === selected)!;
  return (
    <main className='aid-study ghostex-root ghostex-settings-shadcn' data-sidebar-theme='dark-2'>
      <header className='aid-header'>
        <div>
          <p className='aid-eyebrow'>GHOSTEX / ACCOUNT IDENTITY</p>
          <h1>{design ? shown[0].name : '10 ways to identify an account'}</h1>
          <p>
            The titlebar strip from your screenshot, with room for labels like <strong>cw</strong>.
          </p>
        </div>
        <span className='aid-study-tag'>Design study</span>
      </header>
      <div className='aid-controls'>
        <label htmlFor='aid-label'>
          Claude Work label
          <SettingsInput
            id='aid-label'
            maxLength={4}
            value={currentLabel}
            placeholder='1'
            onChange={(event) => setEditedLabel(normalizeAccountIndicatorInput(event.target.value))}
          />
        </label>
        <div className='aid-color-control'>
          <label htmlFor='aid-brand'>Original provider colors</label>
          <Switch id='aid-brand' checked={currentBrand} onCheckedChange={setEditedBrand} />
        </div>
        <p>
          Try <strong>cw</strong>, <strong>W</strong>, or <strong>-</strong> to hide the label.
          <br />
          Click an account to compare its selected state.
        </p>
      </div>
      <div className='aid-design-list'>
        {shown.map((spec) => (
          <section className='aid-design' key={spec.id}>
            <div className='aid-design-heading'>
              <span className='aid-design-number'>{String(designs.indexOf(spec) + 1).padStart(2, '0')}</span>
              <div>
                <h2>{spec.name}</h2>
                <p>{spec.description}</p>
              </div>
              <small>{spec.width} / account</small>
            </div>
            <Strip spec={spec} accounts={accounts} selected={selected} brand={currentBrand} onSelect={setSelected} />
            {design && (
              <div className='aid-session-preview'>
                <div className='aid-session-heading'>
                  <span>Ghostex</span>
                  <IconChevronDown size={12} />
                  <span>Accounts management</span>
                </div>
                <div className='aid-session-body'>
                  <p>Keep the account identity readable at a glance.</p>
                  <span>Provider, account, and usage each have their own space.</span>
                </div>
                <div className='aid-composer'>
                  <p>Ask a follow-up…</p>
                  <footer>
                    <AccountChip
                      account={selectedAccount}
                      design={spec.id}
                      selected
                      brand={currentBrand}
                      onSelect={() => {}}
                    />
                    <span>
                      {selectedAccount.provider === 'claude' ? 'Opus' : 'GPT-5.6'} <IconChevronDown size={12} />
                    </span>
                    <IconDots size={16} />
                  </footer>
                </div>
              </div>
            )}
          </section>
        ))}
      </div>
      <footer className='aid-footnote'>
        Selected: {providerName(selectedAccount.provider)} / {selectedAccount.name}. Example usage data only. These
        variations do not change the app’s selected design.
      </footer>
    </main>
  );
}

const meta = {
  title: 'Accounts/Indicator designs',
  component: DesignStudy,
  parameters: { layout: 'fullscreen' },
  args: { label: 'cw', brandIcons: false },
  argTypes: {
    label: { control: 'text', description: 'Up to two letters or digits for Claude Work. A hyphen hides it.' },
    brandIcons: { control: 'boolean' },
    design: { table: { disable: true } },
  },
} satisfies Meta<typeof DesignStudy>;
export default meta;
type Story = StoryObj<typeof meta>;
export const AllDesigns: Story = { name: '00 Compare all 10' };
export const PairedTiles: Story = { name: '01 Paired tiles', args: { design: 'paired' } };
export const QuietCapsule: Story = { name: '02 Quiet capsule', args: { design: 'capsule' } };
export const AccountFirst: Story = { name: '03 Account first', args: { design: 'monogram' } };
export const LabelUnderneath: Story = {
  name: '04 Text over icon',
  argTypes: { brandIcons: { table: { disable: true } } },
  args: { design: 'stacked' },
  render: ({ label = 'cw' }) => (
    <main className='aid-study aid-chosen ghostex-root' data-sidebar-theme='dark-2'>
      <div
        className='aid-chosen-icons'
        role='group'
        aria-label='Account labels over agent icons with usage beside them'
      >
        {samples.map((account) => (
          <span
            key={account.id}
            role='img'
            aria-label={
              providerName(account.provider) + ' ' + account.name + ', ' + account.first + ', ' + account.second
            }
          >
            <ProjectAgentLauncherIcon
              agent={{
                agentId: account.provider,
                name: providerName(account.provider),
                icon: account.provider,
                isDefault: false,
              }}
              accountIndicator={
                account.id === 'claude-work' ? normalizeAccountIndicatorInput(label) || '1' : account.label
              }
              colorMode='brand'
            />
            <span className='aid-usage'>
              <span>{account.first}</span>
              <span>{account.second}</span>
            </span>
          </span>
        ))}
      </div>
    </main>
  ),
};
export const AccountTabs: Story = { name: '05 Account tabs', args: { design: 'tab' } };
export const SplitKey: Story = { name: '06 Split key', args: { design: 'keycap' } };
export const ProviderAndLabel: Story = { name: '07 Provider + label', args: { design: 'wordmark' } };
export const SideLabel: Story = { name: '08 Side label', args: { design: 'rail' } };
export const UsageFirst: Story = { name: '09 Usage first', args: { design: 'usage' } };
export const TypeOnly: Story = { name: '10 Type only', args: { design: 'type' } };

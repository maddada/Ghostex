import './accounts.css';

/** CDXC:AgentProviders 2026-09-09 DECISION: User wants the same account-label font centered over a larger provider icon as its background. Account icons always use original provider colors; labels and adjacent usage figures share the chat indicator’s monospace font. Claude keeps its label color; Codex uses #7db8fb. The background icon is 19.2px (20% smaller) and the label is 9.9px (10% bigger). This replaces the label-underneath design. Sidebar icons and account menus have no indicator. Labels accept up to two letters or digits; - hides the label and an empty setting uses the slot number. */
export function AccountIndicator({ value }: { value?: string }) {
  return value && value !== '-' ? <span aria-hidden='true' className='gx-account-indicator'>{value}</span> : null;
}

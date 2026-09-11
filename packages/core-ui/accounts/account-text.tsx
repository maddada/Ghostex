import { createContext, useContext } from 'react';
import { maskAccountText } from '@/packages/shared/account-display';
import { useSidebarStore } from '../sidebar-store';

export const AccountPrivacyContext = createContext<boolean | undefined>(undefined);
export function useHideAccountEmails() {
  const override = useContext(AccountPrivacyContext);
  const saved = useSidebarStore((state) => state.hud.settings?.hideAccountEmails === true);
  return override ?? saved;
}
export function useAccountText() {
  const hidden = useHideAccountEmails();
  return (text: string) => (hidden ? maskAccountText(text) : text);
}
export function AccountText({ text }: { text: string }) {
  const format = useAccountText();
  return <span className='gx-account-text'>{format(text)}</span>;
}

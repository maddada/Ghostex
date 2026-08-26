import { useEffect, type ReactNode } from 'react';

type ModalStorySurfaceProps = {
  children: ReactNode;
  theme?: 'dark-1' | 'dark-2' | 'light-orange';
};

/**
 * Matches the app-modal host contract for portalled content while keeping the
 * Storybook canvas large enough to inspect the dialog as an overlay.
 */
export function ModalStorySurface({ children, theme = 'dark-2' }: ModalStorySurfaceProps) {
  useEffect(() => {
    const previousTheme = document.body.dataset.sidebarTheme;
    document.body.classList.add('app-modal-host-body');
    document.body.dataset.sidebarTheme = theme;

    return () => {
      document.body.classList.remove('app-modal-host-body');
      if (previousTheme === undefined) {
        delete document.body.dataset.sidebarTheme;
      } else {
        document.body.dataset.sidebarTheme = previousTheme;
      }
    };
  }, [theme]);

  return (
    <div className='ghostex-root min-h-screen bg-[#050505]' data-sidebar-theme={theme}>
      {children}
    </div>
  );
}

export const modalStoryParameters = {
  layout: 'fullscreen' as const,
};

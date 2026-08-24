import { SidebarApp } from '@/packages/core-ui/sidebar-app';
import '@/packages/core-ui/styles.css';
import type { WebSidebarRuntime } from './sidebar-runtime';

export function WebSidebar({ runtime }: { runtime: WebSidebarRuntime }) {
  return (
    <div className='native-sidebar-shell web-sidebar__shell' data-sidebar-host='web' data-sidebar-mode='combined'>
      <main className='native-sidebar-main'>
        <SidebarApp
          enableProjectCollections
          messageSource={runtime.messageSource}
          nativeHostEventSource={null}
          vscode={runtime.vscode}
          windowScopeId='web-main'
        />
      </main>
    </div>
  );
}

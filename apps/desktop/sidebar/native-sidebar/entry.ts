import { bootClientStorage } from '@/packages/client-storage/bootstrap';
import { sidebarStore } from '@/packages/core-ui/sidebar-store-model';
import { createGpuiSidebarRuntime } from '../gxserver-runtime';
import { currentGpuiRuntimeSettings } from '../gxserver-runtime/helpers/bootstrap';
import { createGpuiSidebarHudState } from '../gxserver-runtime/helpers/command-pane';
import type { GpuiSidebarRuntimeSettings } from '../gxserver-runtime/types-and-protocol';
import { installSessionChatRuntimeBroker } from '../session-chat-runtime/broker';
import { connectNativeSidebar } from './controller';
import { connectNativeQuickAccess } from '../native-quick-access/controller';

const initialSettings = new Promise<GpuiSidebarRuntimeSettings>((resolve) => {
  const installed = currentGpuiRuntimeSettings();
  if (installed) resolve(installed);
  else (window.ghostexGpui ??= {}).onRuntimeSettingsChanged = resolve;
});

bootClientStorage(async () => {
  const runtimeSettings = await initialSettings;
  sidebarStore.getState().applyHudChangedMessage({
    type: 'sidebarHudChanged',
    revision: 0,
    hud: createGpuiSidebarHudState({ runtimeSettings }),
  });
  installSessionChatRuntimeBroker();
  const runtime = createGpuiSidebarRuntime();
  connectNativeSidebar(runtime);
  connectNativeQuickAccess(runtime);
  runtime.start();
});

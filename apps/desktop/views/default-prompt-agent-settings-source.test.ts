import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const modalHostSource = readFileSync(new URL('./modal-host.tsx', import.meta.url), 'utf8');

describe('default prompt agent settings source', () => {
  test('does not save modal default settings before native hydrate', () => {
    /*
     * CDXC:AgentProviders 2026-06-19-08:58:
     * The modal store initializes with DEFAULT_ghostex_SETTINGS. Settings and
     * First Launch should not render as writable until a native hydrate replaces
     * that placeholder with the gxserver-backed settings snapshot.
     *
     * CDXC:Settings 2026-06-21-04:18:
     * Settings renderability now separates the Settings-family modal check from
     * the hydrate check so native child windows can block `presented` until the
     * actual Settings UI is renderable.
     *
     * CDXC:Onboarding 2026-06-29-13:46:
     * First Launch uses the same hydrated Settings store gate, so it must also
     * block native presentation until the modal host has applied native state.
     */
    expect(modalHostSource).toContain('const revision = useSidebarStore((state) => state.revision);');
    expect(modalHostSource).toContain('const hasNativeSettingsHydrated = revision > 0;');
    expect(modalHostSource).toContain('const isSettingsModal = isSettingsModalKind(activeModal);');
    expect(modalHostSource).toContain('const isSettingsRenderable = isSettingsModal && hasNativeSettingsHydrated;');
    expect(modalHostSource).toContain(
      'const isFirstLaunchSetupRenderable = isFirstLaunchSetupModal && hasNativeSettingsHydrated;'
    );
    expect(modalHostSource).toContain('(!isFirstLaunchSetupModal || isFirstLaunchSetupRenderable)');
  });
});

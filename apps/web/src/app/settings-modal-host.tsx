import { lazy, Suspense, useEffect, useState } from "react";
import type { ghostexSettings } from "@/packages/shared/ghostex-settings";
import type { WebSidebarRuntime } from "../sidebar-runtime/sidebar-runtime";
import { readWebSettings, writeWebSettings } from "./web-settings";

const SettingsModal = lazy(() =>
  import("@/packages/core-ui/settings-modal").then((module) => ({ default: module.SettingsModal })),
);

export function SettingsModalHost({ runtime }: { runtime: WebSidebarRuntime }) {
  const [isOpen, setIsOpen] = useState(false);
  const [settings, setSettings] = useState(readWebSettings);

  useEffect(() => {
    const open = () => setIsOpen(true);
    const close = () => setIsOpen(false);
    window.addEventListener("ghostex-web:openSettingsModal", open);
    window.addEventListener("ghostex-web:closeAppModal", close);
    return () => {
      window.removeEventListener("ghostex-web:openSettingsModal", open);
      window.removeEventListener("ghostex-web:closeAppModal", close);
    };
  }, []);

  const save = (nextSettings: ghostexSettings) => {
    const normalized = writeWebSettings(nextSettings);
    setSettings(normalized);
    runtime.updateSettings(normalized);
  };

  if (!isOpen) {
    return null;
  }

  return (
    <Suspense fallback={null}>
      <SettingsModal
        appIconPickerUnavailable
        isOpen
        onChange={save}
        onClose={() => setIsOpen(false)}
        settings={settings}
        theme="dark-blue"
      />
    </Suspense>
  );
}

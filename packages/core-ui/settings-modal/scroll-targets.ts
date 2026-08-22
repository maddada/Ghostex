/*
 * CDXC:SettingsModalSplit 2026-08-23:
 * Scroll-target helpers shared by the SettingsModal component and the extracted
 * settings-modal modules. They live here so a module can resolve a section
 * anchor without importing settings-modal.tsx back.
 */
import { type RefObject } from "react";
import { type MainSettingsScrollTargetId, type MainSettingsSectionRefs } from "./types";

export function getActiveSettingsModalScrollViewport(dialogElement: HTMLElement | null): HTMLElement | null {
  return (
    dialogElement
      ?.querySelector<HTMLElement>("[role='tabpanel'][data-state='active']")
      ?.querySelector<HTMLElement>("[data-slot='scroll-area-viewport']") ?? null
  );
}

export function getMainSettingsSectionRef(
  sectionId: MainSettingsScrollTargetId,
  refs: MainSettingsSectionRefs,
): RefObject<HTMLDivElement | null> {
  return refs[sectionId];
}

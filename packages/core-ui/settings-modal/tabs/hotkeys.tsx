import {
  useEffect,
  useMemo,
  useRef,
  type UIEvent as ReactUIEvent,
} from "react";
import { Button } from "@/packages/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/packages/components/ui/field";
import {
  DEFAULT_ghostex_HOTKEYS,
  GHOSTEX_HOTKEY_DEFINITIONS,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
  type ghostexHotkeyActionId,
  type ghostexHotkeySettings,
} from "../../../shared/ghostex-hotkeys";
import { HotkeyRecorderField } from "../../hotkey-recorder-field";
import {
  SettingsNativeScrollArea,
  SettingsSection,
  ToggleField,
} from "../fields";
import {
  getMostlyVisibleSettingsSectionId,
  shouldShowSetting,
} from "../search";
import {
  HotkeySettingsDefinitionById,
  HotkeySettingsSectionDefinition,
  HotkeySettingsSectionId,
  HotkeySettingsSectionRefs,
  HotkeySettingsSectionSearches,
  SettingModificationProps,
  SettingsSectionMeasurementItem,
  SettingsSectionNavigationItem,
} from "../types";

export function HotkeysSettingsTab({
  definitionsById,
  expandCollapsedProjectsOnJump,
  expandCollapsedProjectsOnJumpModification,
  hotkeys,
  onActiveSectionChange,
  onChange,
  onExpandCollapsedProjectsOnJumpChange,
  onShowLessForExpandedProjectJumpsChange,
  searchQuery,
  sectionRefs,
  sectionSearches,
  showLessForExpandedProjectJumps,
  showLessForExpandedProjectJumpsModification,
  visibleSections,
}: {
  definitionsById: HotkeySettingsDefinitionById;
  expandCollapsedProjectsOnJump: boolean;
  expandCollapsedProjectsOnJumpModification: Required<SettingModificationProps>;
  hotkeys?: ghostexHotkeySettings;
  onActiveSectionChange: (sectionId: HotkeySettingsSectionId) => void;
  onChange: (hotkeys: ghostexHotkeySettings) => void;
  onExpandCollapsedProjectsOnJumpChange: (checked: boolean) => void;
  onShowLessForExpandedProjectJumpsChange: (checked: boolean) => void;
  searchQuery: string;
  sectionRefs: HotkeySettingsSectionRefs;
  sectionSearches: HotkeySettingsSectionSearches;
  showLessForExpandedProjectJumps: boolean;
  showLessForExpandedProjectJumpsModification: Required<SettingModificationProps>;
  visibleSections: readonly HotkeySettingsSectionDefinition[];
}) {
  const normalizedHotkeys = normalizeghostexHotkeySettings(hotkeys);
  const defaultHotkeys = normalizeghostexHotkeySettings(DEFAULT_ghostex_HOTKEYS);
  const duplicateIds = useMemo(
    () => getDuplicateHotkeyIds(normalizedHotkeys),
    [normalizedHotkeys],
  );
  const pendingHotkeySectionViewportRef = useRef<HTMLElement | null>(null);
  const hotkeySectionFrameRef = useRef<number | undefined>(undefined);
  /**
   * CDXC:Hotkeys 2026-05-13-16:05
   * Superseded by CDXC:SettingsNavigation 2026-06-24-22:16.
   *
   * CDXC:SettingsNavigation 2026-06-24-22:16:
   * Hotkey section refs and search results are owned by SettingsModal so the
   * shared sidebar can expand Hotkeys and jump into its internal sections.
   * The same top search query filters General and Hotkeys instead of keeping a
   * hidden tab-specific search state.
   */
  const visibleHotkeySectionNavigation: SettingsSectionNavigationItem<HotkeySettingsSectionId>[] =
    visibleSections.map((section) => ({
      id: section.id,
      title: section.title,
    }));
  const visibleHotkeySectionMeasurementItems: SettingsSectionMeasurementItem<HotkeySettingsSectionId>[] =
    visibleSections.map((section) => ({
      id: section.id,
      ref: sectionRefs[section.id],
    }));
  const visibleHotkeySectionIds = visibleHotkeySectionNavigation
    .map((section) => section.id)
    .join("|");
  const hasVisibleHotkeys = visibleSections.length > 0;

  const updateHotkey = (id: ghostexHotkeyActionId, value: string) => {
    onChange(
      normalizeghostexHotkeySettings({
        ...normalizedHotkeys,
        [id]: normalizeHotkeyText(value),
      }),
    );
  };

  const resetHotkeys = () => {
    onChange(defaultHotkeys);
  };

  const scheduleHotkeySectionMeasurement = (viewport: HTMLElement) => {
    /*
     * CDXC:SettingsPerformance 2026-06-29-00:40:
     * Hotkeys uses the same active-section measurement as General Settings.
     * Keep scroll handlers cheap by measuring section rects once per animation
     * frame instead of on every scroll event.
     */
    pendingHotkeySectionViewportRef.current = viewport;
    if (hotkeySectionFrameRef.current !== undefined) {
      return;
    }
    hotkeySectionFrameRef.current = requestAnimationFrame(() => {
      hotkeySectionFrameRef.current = undefined;
      const pendingViewport = pendingHotkeySectionViewportRef.current;
      pendingHotkeySectionViewportRef.current = null;
      if (!pendingViewport?.isConnected) {
        return;
      }
      const mostlyVisibleSectionId = getMostlyVisibleSettingsSectionId(
        pendingViewport,
        visibleHotkeySectionMeasurementItems,
      );
      if (mostlyVisibleSectionId) {
        onActiveSectionChange(mostlyVisibleSectionId);
      }
    });
  };

  const handleHotkeySettingsScrollCapture = (event: ReactUIEvent<HTMLDivElement>) => {
    if (!(event.target instanceof HTMLElement) || event.target.dataset.slot !== "scroll-area-viewport") {
      return;
    }
    scheduleHotkeySectionMeasurement(event.target);
  };

  useEffect(() => {
    return () => {
      if (hotkeySectionFrameRef.current !== undefined) {
        cancelAnimationFrame(hotkeySectionFrameRef.current);
      }
    };
  }, []);

  useEffect(() => {
    const animationFrame = requestAnimationFrame(() => {
      const firstSection = visibleHotkeySectionMeasurementItems[0];
      const viewport = firstSection?.ref.current?.closest<HTMLElement>("[data-slot='scroll-area-viewport']");
      if (!viewport) {
        return;
      }
      const mostlyVisibleSectionId = getMostlyVisibleSettingsSectionId(
        viewport,
        visibleHotkeySectionMeasurementItems,
      );
      if (mostlyVisibleSectionId) {
        onActiveSectionChange(mostlyVisibleSectionId);
      }
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [onActiveSectionChange, searchQuery, visibleHotkeySectionIds]);

  return (
    <div className="settings-main-tab-layout">
      <SettingsNativeScrollArea
        className="settings-main-scroll h-full min-h-0"
        onScrollCapture={handleHotkeySettingsScrollCapture}
      >
        <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
          {visibleSections.map((section) => (
            <SettingsSection
              key={section.id}
              sectionRef={sectionRefs[section.id]}
              title={section.title}
            >
              {section.id === "projects" &&
              shouldShowSetting(sectionSearches.projects, "expandCollapsedProjectsOnJump") ? (
                <ToggleField
                  checked={expandCollapsedProjectsOnJump}
                  description="Reveal a collapsed project row before focusing it from Jump to Project hotkeys."
                  label="Expand Collapsed Projects on Jump"
                  {...expandCollapsedProjectsOnJumpModification}
                  onChange={onExpandCollapsedProjectsOnJumpChange}
                />
              ) : null}
              {section.id === "projects" &&
              expandCollapsedProjectsOnJump &&
              shouldShowSetting(sectionSearches.projects, "showLessForExpandedProjectJumps") ? (
                <ToggleField
                  checked={showLessForExpandedProjectJumps}
                  description="After a project jump expands a collapsed project, switch that project session list to Show less."
                  label="Use Show less After Jump Expand"
                  {...showLessForExpandedProjectJumpsModification}
                  onChange={onShowLessForExpandedProjectJumpsChange}
                />
              ) : null}
              {section.ids.flatMap((id) => {
                const definition = definitionsById.get(id);
                if (
                  !definition ||
                  !shouldShowSetting(sectionSearches[section.id], definition.id)
                ) {
                  return [];
                }
                const value = normalizedHotkeys[definition.id] ?? definition.defaultKey;
                const isDuplicate = duplicateIds.has(definition.id);
                return [
                  <Field className="gap-2.5" data-invalid={isDuplicate} key={definition.id}>
                    <FieldContent>
                      <FieldLabel className="text-sm" htmlFor={`hotkey-${definition.id}`}>
                        {definition.title}
                      </FieldLabel>
                      <FieldDescription className="text-sm">
                        {definition.description}
                      </FieldDescription>
                    </FieldContent>
                    <HotkeyRecorderField
                      ariaInvalid={isDuplicate}
                      id={`hotkey-${definition.id}`}
                      hotkey={value}
                      onChange={(nextHotkey) => updateHotkey(definition.id, nextHotkey)}
                      originalHotkey={defaultHotkeys[definition.id] ?? ""}
                    />
                  </Field>,
                ];
              })}
            </SettingsSection>
          ))}
          {!hasVisibleHotkeys ? (
            <div className="rounded-none border border-border bg-muted/30 px-4 py-6 text-center text-sm text-muted-foreground">
              No hotkeys match your search.
            </div>
          ) : null}
          <div className="flex justify-end">
            <Button onClick={resetHotkeys} type="button" variant="outline">
              Reset Hotkeys
            </Button>
          </div>
        </div>
      </SettingsNativeScrollArea>
    </div>
  );
}

export function getDuplicateHotkeyIds(hotkeys: ghostexHotkeySettings): Set<ghostexHotkeyActionId> {
  const idsByHotkey = new Map<string, ghostexHotkeyActionId[]>();
  for (const definition of GHOSTEX_HOTKEY_DEFINITIONS) {
    const hotkey = normalizeHotkeyText(hotkeys[definition.id] ?? definition.defaultKey);
    if (!hotkey) {
      continue;
    }
    idsByHotkey.set(hotkey, [...(idsByHotkey.get(hotkey) ?? []), definition.id]);
  }

  return new Set(
    Array.from(idsByHotkey.values())
      .filter((ids) => ids.length > 1)
      .flat(),
  );
}

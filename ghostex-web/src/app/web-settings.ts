import {
  DEFAULT_ghostex_SETTINGS,
  normalizeghostexSettings,
  type ghostexSettings,
} from "@/shared/ghostex-settings";

const WEB_SETTINGS_STORAGE_KEY = "ghostexWeb.settings.v1";

export function readWebSettings(): ghostexSettings {
  try {
    const stored = window.localStorage.getItem(WEB_SETTINGS_STORAGE_KEY);
    return stored ? normalizeghostexSettings(JSON.parse(stored)) : DEFAULT_ghostex_SETTINGS;
  } catch {
    return DEFAULT_ghostex_SETTINGS;
  }
}

export function writeWebSettings(settings: ghostexSettings): ghostexSettings {
  const normalized = normalizeghostexSettings(settings);
  window.localStorage.setItem(WEB_SETTINGS_STORAGE_KEY, JSON.stringify(normalized));
  return normalized;
}

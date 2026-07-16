import { useEffect, useState } from "react";

export type ThemeMode = "system" | "light" | "dark";
export type InterfaceDensity = "comfortable" | "compact";
export type ModelProvider = "openai" | "anthropic" | "local";
export type ApprovalPolicy = "always" | "risky" | "trusted";
export type HistoryRetention = "30-days" | "90-days" | "forever";

export interface SettingsPreferences {
  theme: ThemeMode;
  density: InterfaceDensity;
  provider: ModelProvider;
  model: string;
  approvalPolicy: ApprovalPolicy;
  terminalAccess: boolean;
  fileWriteAccess: boolean;
  networkAccess: boolean;
  commandTimeout: string;
  historyRetention: HistoryRetention;
  diagnostics: boolean;
}

const SETTINGS_STORAGE_KEY = "ora.settings.v1";
const DEFAULT_SETTINGS: SettingsPreferences = {
  theme: "system",
  density: "comfortable",
  provider: "openai",
  model: "gpt-5.1-codex",
  approvalPolicy: "risky",
  terminalAccess: true,
  fileWriteAccess: true,
  networkAccess: false,
  commandTimeout: "120",
  historyRetention: "90-days",
  diagnostics: false,
};

/** Reads persisted prototype preferences while tolerating unavailable or stale browser storage. */
function readSettings(): SettingsPreferences {
  if (typeof window === "undefined") return DEFAULT_SETTINGS;
  try {
    const raw = window.localStorage.getItem(SETTINGS_STORAGE_KEY);
    return raw ? { ...DEFAULT_SETTINGS, ...JSON.parse(raw) as Partial<SettingsPreferences> } : DEFAULT_SETTINGS;
  } catch {
    return DEFAULT_SETTINGS;
  }
}

/** Owns local settings until persisted settings contracts are available from the host runtime. */
export function useSettingsPreferences() {
  const [settings, setSettings] = useState<SettingsPreferences>(readSettings);

  useEffect(() => {
    try {
      window.localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
    } catch {
      // Preferences still work for the current runtime when storage is unavailable.
    }
  }, [settings]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      const dark = settings.theme === "dark" || (settings.theme === "system" && media.matches);
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.dataset.theme = settings.theme;
      document.documentElement.dataset.density = settings.density;
    };

    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [settings.density, settings.theme]);

  const updateSettings = (patch: Partial<SettingsPreferences>) => {
    setSettings((current) => ({ ...current, ...patch }));
  };

  return { settings, updateSettings };
}

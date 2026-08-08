import { create } from "zustand";

interface PluginInstallState {
  /** Catalog plugin ids the user has manually installed (the detection-driven CLI runtimes track their own state instead). */
  installedIds: string[];
  /** Installed plugin ids the user has manually disabled without uninstalling them. */
  disabledIds: string[];
  /** Installs or uninstalls a catalog plugin. */
  toggleInstalled: (id: string) => void;
  /** Enables or disables an installed catalog plugin in place. */
  toggleEnabled: (id: string) => void;
}

/**
 * Shared install/enable state for the hard-coded plugin catalog. Kept in one store instead of
 * component state so the Settings marketplace and the chat composer's plugin picker agree on
 * which plugins are actually available to use.
 */
export const usePluginInstallStore = create<PluginInstallState>((set) => ({
  installedIds: [],
  disabledIds: [],
  toggleInstalled: (id) =>
    set((state) => ({
      installedIds: state.installedIds.includes(id)
        ? state.installedIds.filter((current) => current !== id)
        : [...state.installedIds, id],
    })),
  toggleEnabled: (id) =>
    set((state) => ({
      disabledIds: state.disabledIds.includes(id)
        ? state.disabledIds.filter((current) => current !== id)
        : [...state.disabledIds, id],
    })),
}));

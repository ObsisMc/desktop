import type { PluginInstallProgress } from "../../platform";
import { create } from "zustand";

export type PluginOperationKind =
  "install" | "update" | "activate" | "stop" | "uninstall";

export type PluginOperationActivity =
  | {
      state: "pending";
      kind: PluginOperationKind;
      progress: PluginInstallProgress | null;
    }
  | { state: "install_completed"; completionId: number };

interface PluginOperationState {
  activities: Record<string, PluginOperationActivity>;
  /** Starts an operation only when the same plugin has no conflicting work in flight. */
  begin: (pluginId: string, kind: PluginOperationKind) => boolean;
  /** Retains native install transfer progress across settings-page navigation. */
  reportInstallProgress: (progress: PluginInstallProgress) => void;
  /** Briefly records install success so the completion glyph can animate after query refresh. */
  completeInstall: (pluginId: string) => void;
  /** Clears a settled operation so another lifecycle action can begin. */
  clear: (pluginId: string) => void;
}

const COMPLETION_ANIMATION_MS = 700;
let nextCompletionId = 1;

/** Owns plugin lifecycle activity independently of any settings row or card lifecycle. */
export const usePluginOperationStore = create<PluginOperationState>(
  (set, get) => ({
    activities: {},
    begin: (pluginId, kind) => {
      if (get().activities[pluginId]?.state === "pending") return false;
      set((state) => ({
        activities: {
          ...state.activities,
          [pluginId]: { state: "pending", kind, progress: null },
        },
      }));
      return true;
    },
    reportInstallProgress: (progress) => {
      const activity = get().activities[progress.pluginId];
      if (activity?.state !== "pending" || activity.kind !== "install") return;
      set((state) => ({
        activities: {
          ...state.activities,
          [progress.pluginId]: {
            state: "pending",
            kind: "install",
            progress,
          },
        },
      }));
    },
    completeInstall: (pluginId) => {
      const completionId = nextCompletionId;
      nextCompletionId += 1;
      set((state) => ({
        activities: {
          ...state.activities,
          [pluginId]: { state: "install_completed", completionId },
        },
      }));
      // Completion is transitional UI state, not durable installation truth; queries own that.
      setTimeout(() => {
        const activity = get().activities[pluginId];
        if (
          activity?.state === "install_completed" &&
          activity.completionId === completionId
        ) {
          get().clear(pluginId);
        }
      }, COMPLETION_ANIMATION_MS);
    },
    clear: (pluginId) => {
      set((state) => {
        const activities = { ...state.activities };
        delete activities[pluginId];
        return { activities };
      });
    },
  }),
);

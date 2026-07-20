import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  PathSelectionInProgressError,
  type PlatformAdapter,
  type SelectPathOptions,
} from "../types";

/** Delegates path selection to the desktop operating system's native open dialog. */
export class TauriPlatformAdapter implements PlatformAdapter {
  private selectionInProgress = false;

  readonly worktreeStorage = {
    kind: "configurable" as const,
    getRoot: async (): Promise<string> => {
      const config = await invoke<{ worktreeRoot: string }>("get_desktop_config", {
        request: {},
      });
      return config.worktreeRoot;
    },
    setRoot: async (path: string): Promise<void> => {
      await invoke("set_worktree_root", {
        request: { worktreeRoot: path },
      });
    },
  };

  /** Opens one native single-selection dialog configured for a file or directory. */
  async selectPath(options: SelectPathOptions): Promise<string | null> {
    if (this.selectionInProgress) {
      throw new PathSelectionInProgressError();
    }

    this.selectionInProgress = true;
    try {
      return await open({
        directory: options.kind === "directory",
        multiple: false,
        defaultPath: options.initialPath,
      });
    } finally {
      this.selectionInProgress = false;
    }
  }
}

/** Creates the desktop adapter without runtime platform auto-detection. */
export function createTauriPlatformAdapter(): TauriPlatformAdapter {
  return new TauriPlatformAdapter();
}

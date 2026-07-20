export type PathSelectionKind = "file" | "directory";

export interface SelectPathOptions {
  kind: PathSelectionKind;
  initialPath?: string;
}

export type WorktreeStorageCapability =
  | { kind: "unsupported" }
  | {
      kind: "configurable";
      getRoot(): Promise<string>;
      setRoot(path: string): Promise<void>;
    };

/** Abstracts one single-path selection interaction across Web and Tauri hosts. */
export interface PlatformAdapter {
  readonly worktreeStorage: WorktreeStorageCapability;
  selectPath(options: SelectPathOptions): Promise<string | null>;
}

export type PlatformLocale = "zh-CN" | "en-US";

/** Reports a caller bug that attempts to open two selectors on one adapter concurrently. */
export class PathSelectionInProgressError extends Error {
  constructor() {
    super("a path selection request is already in progress");
    this.name = "PathSelectionInProgressError";
  }
}

import type { ComposerFileAttrs } from "@ora/editor/composer";
import type { TaskChangesNavigation } from "./diff/task-changes-navigation-context";

/**
 * Routes a user-authored file reference chip (composer `@` mention, file-preview
 * quote, or git-diff-gutter quote) to the matching Files/Changes navigation call.
 *
 * Unlike `chat-link`'s text classification, these attrs are already structured
 * and trustworthy — no index lookup needed. Paths are workspace-relative
 * already (they come from the same explorer/search listings and diff
 * `oldPath`/`newPath` the rest of the app treats as relative), so no cwd
 * stripping is needed here either.
 *
 * Returns whether navigation happened, so callers can decide whether to
 * suppress the click's default behaviour.
 */
export function navigateToFileRef(
  attrs: ComposerFileAttrs,
  navigation: TaskChangesNavigation | null,
): boolean {
  if (navigation === null || attrs.path === "") return false;
  if (attrs.kind === "directory") {
    if (navigation.openWorkspaceDirectory === undefined) return false;
    navigation.openWorkspaceDirectory(attrs.path);
    return true;
  }
  if (attrs.origin === "diff") {
    navigation.openDiff(attrs.path, attrs.startLine);
    return true;
  }
  navigation.openWorkspaceFile(attrs.path, attrs.startLine);
  return true;
}

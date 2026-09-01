import { createElement, type ReactNode } from "react";
import { render, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { AppI18nProvider } from "../../i18n/i18n";
import { ContractsClientContext } from "../../contracts-client-context";
import {
  createMockClient,
  createMockClientState,
} from "../../test/mock-client";
import { queryKeys } from "../../state/hooks/query-keys";
import { TaskDiffView } from "./task-diff-view";

/** Builds a new-file patch whose single hunk carries `lines` added lines. */
function largeFilePatch(path: string, lines: number): string {
  const body = Array.from({ length: lines }, (_line, index) => `+${index + 1}`);
  return [
    `diff --git a/${path} b/${path}`,
    "new file mode 100644",
    "index 0000000..1111111",
    "--- /dev/null",
    `+++ b/${path}`,
    `@@ -0,0 +1,${lines} @@`,
    ...body,
    "",
  ].join("\n");
}

/**
 * Renders Changes with the patch pre-seeded into the query cache, so the very
 * first commit mounts the file (the defer decision happens there) — mirroring
 * a session switch back onto an already-loaded workspace diff.
 */
function renderDiff(patch: string) {
  const client = createMockClient(createMockClientState());
  client.workspace.getDiff = async () => ({
    baseCommitId: "base",
    headCommitId: "head",
    patch,
  });
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
  queryClient.setQueryData(queryKeys.workspaceDiff("task-1", "branch"), {
    baseCommitId: "base",
    headCommitId: "head",
    patch,
  });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(
        ContractsClientContext.Provider,
        { value: client },
        createElement(AppI18nProvider, null, children),
      ),
    );
  return render(
    <TaskDiffView
      workspaceId="task-1"
      hasBaseline
      viewType="unified"
      fileTreeOpen
      onFileTreeOpenChange={() => undefined}
    />,
    { wrapper },
  );
}

describe("TaskDiffFile deferred mount", () => {
  it("shows a placeholder for oversized diffs and brings the rows in after a paint", async () => {
    const { container } = renderDiff(largeFilePatch("big/generated.ts", 700));

    // First commit: placeholder only — no diff rows are created yet.
    expect(
      container.querySelector("[data-diff-deferred-loading]"),
    ).not.toBeNull();
    expect(container.querySelector(".diff-line")).toBeNull();

    await waitFor(() =>
      expect(container.querySelector(".diff-line")).not.toBeNull(),
    );
    expect(container.querySelector("[data-diff-deferred-loading]")).toBeNull();
  });

  it("renders small diffs immediately without the placeholder", async () => {
    const { container } = renderDiff(largeFilePatch("src/small.ts", 5));

    await waitFor(() =>
      expect(container.querySelector(".diff-line")).not.toBeNull(),
    );
    expect(container.querySelector("[data-diff-deferred-loading]")).toBeNull();
  });
});

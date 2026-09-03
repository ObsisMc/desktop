import { createElement, type ReactNode } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { appI18n } from "../../i18n/i18n-instance";
import { AppI18nProvider } from "../../i18n/i18n";
import { ContractsClientContext } from "../../contracts-client-context";
import {
  createMockClient,
  createMockClientState,
} from "../../test/mock-client";
import { queryKeys } from "../../state/hooks/query-keys";
import { TaskDiffView } from "./task-diff-view";

/** Gives the row virtualizer a viewport size in jsdom (no layout is performed). */
const mockViewportSize = (height: number) => {
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(
    height,
  );
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
};

/** Builds a new-file patch whose single hunk carries `lines` added lines. */
function filePatch(path: string, lines: number): string {
  const body = Array.from({ length: lines }, (_l, i) => `+${i + 1}`);
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

/** Builds a patch of many small new files. */
function manyFilesPatch(count: number): string {
  const patches: string[] = [];
  for (let i = 0; i < count; i += 1) {
    patches.push(
      `diff --git a/f${i}.ts b/f${i}.ts\nnew file mode 100644\nindex 0000000..1111111\n--- /dev/null\n+++ b/f${i}.ts\n@@ -0,0 +1,2 @@\n+a${i}\n+b${i}\n`,
    );
  }
  return patches.join("\n");
}

afterEach(() => vi.restoreAllMocks());

function renderDiff(
  patch: string,
  options?: {
    viewType?: "unified" | "split";
    fileRequest?: { path: string; requestId: number; line?: number };
    onFileNotFound?: () => void;
  },
) {
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
      viewType={options?.viewType ?? "unified"}
      fileTreeOpen
      fileRequest={options?.fileRequest}
      onFileNotFound={options?.onFileNotFound}
      onFileTreeOpenChange={() => undefined}
    />,
    { wrapper },
  );
}

describe("TaskDiffView performance modes", () => {
  it("enters focus mode automatically for a large number of files", async () => {
    mockViewportSize(600);
    const { container } = renderDiff(manyFilesPatch(50));

    await waitFor(() =>
      expect(container.querySelector(".ora-diff-scroll-region")).not.toBeNull(),
    );
    // A single-file focus body renders exactly one file article.
    await waitFor(() =>
      expect(container.querySelectorAll("article").length).toBeGreaterThan(0),
    );
    expect(container.querySelectorAll("article").length).toBe(1);
  });

  it("switches back to scroll mode via the toolbar toggle", async () => {
    mockViewportSize(600);
    const { container } = renderDiff(manyFilesPatch(50));

    await waitFor(() =>
      expect(container.querySelectorAll("article").length).toBe(1),
    );
    const toggle = screen.getByRole("button", {
      name: appI18n.t("diff.viewModeScroll"),
    });
    fireEvent.click(toggle);

    await waitFor(() =>
      expect(container.querySelectorAll("article").length).toBeGreaterThan(1),
    );
  });

  it("keeps react-diff-view's tint classes on a row-windowed large file", async () => {
    // A single file large enough to row-window keeps the native tint classes
    // on the windowed rows — the virtualized body must not drop them.
    mockViewportSize(600);
    const { container } = renderDiff(filePatch("big/generated.ts", 700));

    await waitFor(() =>
      expect(container.querySelector(".diff-line")).not.toBeNull(),
    );
    // Insert tint classes present on the windowed rows.
    expect(container.querySelector(".diff-code-insert")).not.toBeNull();
    expect(container.querySelector(".diff-gutter-insert")).not.toBeNull();
    // A single huge file is chunk-windowed into bounded native tables.
    const mounted = container.querySelectorAll("table.diff").length;
    expect(mounted).toBeGreaterThan(0);
    expect(mounted).toBeLessThan(6);
    // The summary still shows a loading state was never leaked.
    expect(container.querySelector("[data-diff-deferred-loading]")).toBeNull();
  });
});

/** A patch over 512k chars of added lines, enough to trip the parse gate. */
function deferredPatch(): string {
  const padded = "x".repeat(520);
  const body = Array.from({ length: 1000 }, (_l, i) => `+${i}-${padded}`);
  return [
    `diff --git a/big/api.ts b/big/api.ts`,
    "new file mode 100644",
    "index 0000000..1111111",
    "--- /dev/null",
    "+++ b/big/api.ts",
    `@@ -0,0 +1,1000 @@`,
    ...body,
    "",
  ].join("\n");
}

describe("TaskDiffView deferred parse", () => {
  it("defers parsing of an oversized patch past first paint", async () => {
    const { container } = renderDiff(deferredPatch());

    // First commit: parse is deferred, so no file rows exist and the toolbar
    // shows zero files while a loading state is shown instead of "no changes".
    expect(container.querySelector("[data-diff-deferred-loading]")).toBeNull();
    // The parse gate defers the whole file list, so nothing is rendered yet.
    expect(container.querySelector("article")).toBeNull();

    await waitFor(
      () => expect(container.querySelector("article")).not.toBeNull(),
      { timeout: 8000 },
    );
  });

  it("does not report a restored file as missing while the parse is deferred", async () => {
    // Session switch replays a stored preview path into a still-deferred patch.
    // The layout must not be yanked into the Files panel (via onFileNotFound)
    // just because `filePaths` is empty for the deferred first render.
    let missing = false;
    const { container } = renderDiff(deferredPatch(), {
      fileRequest: {
        path: "big/api.ts",
        requestId: 1,
        line: 3,
      },
      onFileNotFound: () => {
        missing = true;
      },
    });

    await waitFor(
      () => expect(container.querySelector("article")).not.toBeNull(),
      { timeout: 8000 },
    );
    expect(missing).toBe(false);
  });
});

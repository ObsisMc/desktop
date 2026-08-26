import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ComposerFileAttrs } from "@ora/editor/composer";
import { AppI18nProvider } from "../i18n/i18n";
import { TaskChangesNavigationProvider } from "./diff/task-changes-navigation";
import { FileRefChip } from "./file-ref-chip";

function renderChip(
  attrs: ComposerFileAttrs,
  navigation?: {
    openDiff?: (path: string, line?: number) => void;
    openWorkspaceFile?: (path: string, line?: number, column?: number) => void;
    openWorkspaceDirectory?: (path: string) => void;
  },
) {
  const openDiff = navigation?.openDiff ?? vi.fn();
  const openWorkspaceFile = navigation?.openWorkspaceFile ?? vi.fn();
  const openWorkspaceDirectory = navigation?.openWorkspaceDirectory ?? vi.fn();
  render(
    <AppI18nProvider>
      <TaskChangesNavigationProvider
        onOpenDiff={openDiff}
        onOpenWorkspaceFile={openWorkspaceFile}
        onOpenWorkspaceDirectory={openWorkspaceDirectory}
      >
        <FileRefChip attrs={attrs} />
      </TaskChangesNavigationProvider>
    </AppI18nProvider>,
  );
  return { openDiff, openWorkspaceFile, openWorkspaceDirectory };
}

describe("FileRefChip", () => {
  it("opens a plain file mention in Files at its start line", async () => {
    const user = userEvent.setup();
    const { openWorkspaceFile } = renderChip({
      path: "src/main.ts",
      startLine: 12,
      kind: "file",
    });

    const chip = screen.getByRole("button", { name: /main\.ts/ });
    await user.click(chip);

    expect(openWorkspaceFile).toHaveBeenCalledWith("src/main.ts", 12);
  });

  it("opens a diff-origin quote in Changes at its start line", async () => {
    const user = userEvent.setup();
    const { openDiff } = renderChip({
      path: "src/example.ts",
      startLine: 2,
      endLine: 40,
      snippet: " keep\n+added",
      origin: "diff",
      diffSide: "new",
    });

    await user.click(screen.getByRole("button", { name: /example\.ts/ }));

    expect(openDiff).toHaveBeenCalledWith("src/example.ts", 2);
  });

  it("opens a directory mention as a folder, never through readFile", async () => {
    const user = userEvent.setup();
    const { openWorkspaceDirectory, openWorkspaceFile } = renderChip({
      path: "src/features",
      kind: "directory",
    });

    await user.click(screen.getByRole("button", { name: /features/ }));

    expect(openWorkspaceDirectory).toHaveBeenCalledWith("src/features");
    expect(openWorkspaceFile).not.toHaveBeenCalled();
  });

  it("renders as an inert, non-navigable span without a navigation context", () => {
    render(
      <AppI18nProvider>
        <FileRefChip attrs={{ path: "src/main.ts", kind: "file" }} />
      </AppI18nProvider>,
    );

    expect(screen.queryByRole("button")).toBeNull();
    const chip = document.querySelector("[data-composer-file='src/main.ts']");
    expect(chip?.tagName).toBe("SPAN");
    expect(chip).not.toHaveAttribute("data-navigable");
  });

  it("marks a clickable chip as navigable for the hover/focus affordance", () => {
    renderChip({ path: "src/main.ts", kind: "file" });
    const chip = screen.getByRole("button", { name: /main\.ts/ });
    expect(chip).toHaveAttribute("data-navigable", "true");
  });
});

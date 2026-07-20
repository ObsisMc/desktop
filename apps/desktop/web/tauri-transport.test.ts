import { describe, expect, it, vi } from "vitest";
import { ContractTransportError } from "@ora/contracts";
import { createTauriTransport } from "./tauri-transport";

describe("createTauriTransport", () => {
  it("maps supported operations and forwards the complete request", async () => {
    const invoke = vi.fn().mockResolvedValue({ projects: [] });
    const transport = createTauriTransport(invoke);
    const request = {
      operationName: "listProjects",
      request: {},
      method: "GET" as const,
      path: "/api/projects",
      body: undefined,
      headers: {},
    };

    await expect(transport.send(request)).resolves.toEqual({ projects: [] });
    expect(invoke).toHaveBeenCalledWith("list_projects", { request: {} });
  });

  it("rejects explicitly unsupported operations before invoking Rust", async () => {
    const invoke = vi.fn();
    const transport = createTauriTransport(invoke);

    await expect(
      transport.send({
        operationName: "listDirectory",
        request: { path: "/tmp" },
        method: "GET",
        path: "/api/file-system/directory?path=%2Ftmp",
        body: undefined,
        headers: {},
      }),
    ).rejects.toMatchObject({ code: "unsupported_operation", status: null });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("normalizes structured command errors", async () => {
    const invoke = vi.fn().mockRejectedValue({
      code: "project_not_found",
      message: "project not found: project-1",
    });
    const transport = createTauriTransport(invoke);

    try {
      await transport.send({
        operationName: "getProject",
        request: { projectId: "project-1" },
        method: "GET",
        path: "/api/projects/project-1",
        body: undefined,
        headers: {},
      });
      throw new Error("expected transport to reject");
    } catch (error) {
      expect(error).toBeInstanceOf(ContractTransportError);
      expect(error).toMatchObject({
        code: "project_not_found",
        status: null,
        responseBody: {
          code: "project_not_found",
          message: "project not found: project-1",
        },
      });
    }
  });
});

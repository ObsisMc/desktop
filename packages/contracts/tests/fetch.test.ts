import assert from "node:assert/strict";
import test from "node:test";

import { createFetchTransport, decodeErrorEnvelope, resolveUrl } from "../src/fetch.js";
import { ContractTransportError, type ContractTransportRequest } from "../src/transport.js";

test("resolves paths relative to the current origin when baseUrl is empty", () => {
  assert.equal(resolveUrl("", "/api/projects"), "/api/projects");
});

test("resolves paths against an absolute server base", () => {
  assert.equal(
    resolveUrl("http://localhost:32578", "/api/projects"),
    "http://localhost:32578/api/projects",
  );
});

test("decodes the shared HTTP error envelope", () => {
  assert.deepEqual(
    decodeErrorEnvelope({
      error: {
        code: "project_not_found",
        message: "project not found: project-1",
      },
    }),
    {
      code: "project_not_found",
      message: "project not found: project-1",
    },
  );
});

test("normalizes structured server errors from fetch responses", async () => {
  const requests: Array<{
    url: string;
    init: RequestInit | undefined;
  }> = [];
  const transport = createFetchTransport({
    baseUrl: "http://localhost:32578",
    fetch: async (input, init) => {
      requests.push({
        url: String(input),
        init,
      });

      return new Response(
        JSON.stringify({
          error: {
            code: "project_not_found",
            message: "project not found: project-1",
          },
        }),
        {
          status: 404,
          headers: {
            "content-type": "application/json",
          },
        },
      );
    },
  });
  const request: ContractTransportRequest = {
    operationName: "getProject",
    request: { projectId: "project-1" },
    method: "GET",
    path: "/api/projects/project-1",
    body: undefined,
    headers: {},
  };

  await assert.rejects(
    transport.send(request),
    (error: unknown) => {
      assert.ok(error instanceof ContractTransportError);
      const transportError = error as ContractTransportError;

      assert.equal(transportError.code, "project_not_found");
      assert.equal(transportError.status, 404);
      assert.deepEqual(transportError.responseBody, {
        error: {
          code: "project_not_found",
          message: "project not found: project-1",
        },
      });

      return true;
    },
  );
  assert.deepEqual(requests, [
    {
      url: "http://localhost:32578/api/projects/project-1",
      init: {
        method: "GET",
        headers: {},
        body: undefined,
      },
    },
  ]);
});

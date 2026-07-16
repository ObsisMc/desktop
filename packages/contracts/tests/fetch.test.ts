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
        signal: undefined,
      },
    },
  ]);
});

/**
 * Serves one fixed set of raw response chunks as a streaming fetch response.
 */
function eventStreamTransport(chunks: string[]) {
  return createFetchTransport({
    fetch: async () =>
      new Response(
        new ReadableStream<Uint8Array>({
          start(controller) {
            const encoder = new TextEncoder();

            for (const chunk of chunks) {
              controller.enqueue(encoder.encode(chunk));
            }

            controller.close();
          },
        }),
        {
          status: 200,
          headers: {
            "content-type": "text/event-stream",
          },
        },
      ),
  });
}

const subscribeRequest: ContractTransportRequest = {
  operationName: "subscribeSessionEvents",
  method: "POST",
  path: "/api/sessions/session-1/events",
  body: { afterEventId: null },
  headers: {
    "content-type": "application/json",
    accept: "text/event-stream",
  },
};

test("decodes server-sent events split across chunk boundaries", async () => {
  const transport = eventStreamTransport([
    "data: {\"kind\":\"agentMessageChunk\",\"id\":\"event-1\",\"te",
    "xt\":\"hel\"}\n\n: keep-alive\n\ndata: {\"kind\":\"statusChanged\",",
    "\"id\":\"event-2\",\"status\":\"stopped\"}\n\n",
  ]);
  const received: unknown[] = [];

  for await (const event of transport.stream(subscribeRequest)) {
    received.push(event);
  }

  assert.deepEqual(received, [
    { kind: "agentMessageChunk", id: "event-1", text: "hel" },
    { kind: "statusChanged", id: "event-2", status: "stopped" },
  ]);
});

test("stops reading a server-sent event stream once the consumer breaks", async () => {
  const transport = eventStreamTransport([
    "data: {\"kind\":\"agentMessageChunk\",\"id\":\"event-1\",\"text\":\"hel\"}\n\n",
    "data: {\"kind\":\"agentMessageChunk\",\"id\":\"event-2\",\"text\":\"lo\"}\n\n",
  ]);
  const received: unknown[] = [];

  for await (const event of transport.stream(subscribeRequest)) {
    received.push(event);
    break;
  }

  assert.deepEqual(received, [{ kind: "agentMessageChunk", id: "event-1", text: "hel" }]);
});

test("surfaces error frames sent inside a server-sent event stream", async () => {
  const transport = eventStreamTransport([
    "data: {\"error\":{\"code\":\"session_stopped\",\"message\":\"session stopped: session-1\"}}\n\n",
  ]);

  await assert.rejects(
    (async () => {
      for await (const event of transport.stream(subscribeRequest)) {
        void event;
      }
    })(),
    (error: unknown) => {
      assert.ok(error instanceof ContractTransportError);
      assert.equal((error as ContractTransportError).code, "session_stopped");
      assert.equal((error as ContractTransportError).status, null);

      return true;
    },
  );
});

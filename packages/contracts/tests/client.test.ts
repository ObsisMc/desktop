import assert from "node:assert/strict";
import test from "node:test";

import { createContractsClient } from "../src/client.js";
import { endpoints } from "../src/endpoints.js";
import type { SessionEvent } from "../src/session.js";
import type { ContractTransport, ContractTransportRequest } from "../src/transport.js";

/**
 * Builds a transport double that records requests and returns a fixed response.
 *
 * Streaming operations replay `events` so the client's stream wiring can be
 * asserted without an HTTP server.
 */
function recordingTransport<TResponse>(
  requests: ContractTransportRequest[],
  response: TResponse,
  events: unknown[] = [],
): ContractTransport {
  return {
    async send<TTransportResponse>(
      request: ContractTransportRequest,
    ): Promise<TTransportResponse> {
      requests.push(request);

      return response as unknown as TTransportResponse;
    },

    async *stream<TEvent>(request: ContractTransportRequest): AsyncGenerator<TEvent> {
      requests.push(request);

      for (const event of events) {
        yield event as TEvent;
      }
    },
  };
}

test("builds update URLs from path params and JSON bodies", async () => {
  const requests: ContractTransportRequest[] = [];
  const client = createContractsClient(
    recordingTransport(requests, {
      task: {
        id: "task-1",
        projectId: "project-1",
        title: "Ship SDK",
        status: "doing",
      },
    }),
  );
  const response = await client.task.update({
    taskId: "task-1",
    projectId: "project-1",
    title: "Ship SDK",
    status: "doing",
  });

  assert.deepEqual(requests, [
    {
      operationName: "updateTask",
      method: "PUT",
      path: "/api/tasks/task-1",
      body: {
        projectId: "project-1",
        title: "Ship SDK",
        status: "doing",
      },
      headers: {
        "content-type": "application/json",
      },
    },
  ]);
  assert.deepEqual(response, {
    task: {
      id: "task-1",
      projectId: "project-1",
      title: "Ship SDK",
      status: "doing",
    },
  });
});

test("omits JSON bodies for path-only operations", async () => {
  const requests: ContractTransportRequest[] = [];
  const client = createContractsClient(
    recordingTransport(requests, {
      project: {
        id: "project-1",
        name: "Ora",
        rootPath: "/workspace/ora",
      },
    }),
  );

  await client.project.get({
    projectId: "project-1",
  });

  assert.deepEqual(requests, [
    {
      operationName: "getProject",
      method: "GET",
      path: "/api/projects/project-1",
      body: undefined,
      headers: {},
    },
  ]);
});

test("uses a skill id in PUT paths while leaving editable fields in JSON", async () => {
  const requests: ContractTransportRequest[] = [];
  const client = createContractsClient(
    recordingTransport(requests, {
      skill: {
        id: "skill-1",
        name: "code-review",
        description: "Reviews code",
      },
    }),
  );

  await client.skill.update({
    skillId: "skill-1",
    name: "code-review",
    description: "Reviews code",
  });

  assert.deepEqual(requests, [
    {
      operationName: "updateSkill",
      method: "PUT",
      path: "/api/skills/skill-1",
      body: {
        name: "code-review",
        description: "Reviews code",
      },
      headers: {
        "content-type": "application/json",
      },
    },
  ]);
});

test("subscribes to session events over a streaming operation", async () => {
  const requests: ContractTransportRequest[] = [];
  const client = createContractsClient(
    recordingTransport(requests, null, [
      { kind: "agentMessageChunk", id: "event-2", text: "hello" },
      { kind: "statusChanged", id: "event-3", status: "stopped" },
    ]),
  );
  const received: SessionEvent[] = [];

  for await (const event of client.session.subscribe({
    sessionId: "session-1",
    afterEventId: "event-1",
  })) {
    received.push(event);
  }

  assert.deepEqual(requests, [
    {
      operationName: "subscribeSessionEvents",
      method: "POST",
      path: "/api/sessions/session-1/events",
      body: {
        afterEventId: "event-1",
      },
      headers: {
        "content-type": "application/json",
        accept: "text/event-stream",
      },
    },
  ]);
  assert.deepEqual(received, [
    { kind: "agentMessageChunk", id: "event-2", text: "hello" },
    { kind: "statusChanged", id: "event-3", status: "stopped" },
  ]);
});

test("omits standalone worktree operations from generated contracts", () => {
  assert.equal("createWorktree" in endpoints, false);
  assert.equal("getWorktree" in endpoints, false);
  assert.equal("listWorktrees" in endpoints, false);
  assert.equal("updateWorktree" in endpoints, false);
  assert.equal("deleteWorktree" in endpoints, false);

  const client = createContractsClient(
    recordingTransport([], {
      task: {
        id: "task-1",
        projectId: "project-1",
        title: "Ship SDK",
        status: "doing",
      },
    }),
  );

  assert.equal("createWorktree" in client, false);
  assert.equal("getWorktree" in client, false);
  assert.equal("listWorktrees" in client, false);
  assert.equal("updateWorktree" in client, false);
  assert.equal("deleteWorktree" in client, false);
});

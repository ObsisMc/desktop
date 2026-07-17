import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  useConversationsStore,
  setReplyScheduler,
  setNowProvider,
  resetConversationsDeps,
  disposeConversationsStore,
  CONVERSATIONS_STORAGE_KEY,
  type ReplyScheduler,
  type ReplyHandle,
} from "./conversations-store";
import type { Conversation } from "../../lib/types";

/** Scheduler that queues replies for deterministic manual flushing in tests. */
class ControllableScheduler implements ReplyScheduler {
  private queue: Array<{ fn: () => void; cleared: boolean }> = [];

  schedule(_delayMs: number, fn: () => void): ReplyHandle {
    const slot = { fn, cleared: false };
    this.queue.push(slot);
    return {
      clear: () => {
        slot.cleared = true;
      },
    };
  }

  flush(): void {
    const pending = this.queue;
    this.queue = [];
    for (const slot of pending) {
      if (!slot.cleared) slot.fn();
    }
  }

  get pendingCount(): number {
    return this.queue.filter((slot) => !slot.cleared).length;
  }
}

const scheduler = new ControllableScheduler();
const FIXED_NOW = 1_700_000_000_000;

beforeEach(() => {
  // Flush any stray reply from a previous test before swapping deps.
  scheduler.flush();
  window.localStorage.clear();
  setReplyScheduler(scheduler);
  setNowProvider(() => FIXED_NOW);
  useConversationsStore.setState({
    conversations: [],
    activeId: null,
    isResponding: false,
  });
});

afterEach(() => {
  scheduler.flush();
  disposeConversationsStore();
  resetConversationsDeps();
  window.localStorage.clear();
  useConversationsStore.setState({
    conversations: [],
    activeId: null,
    isResponding: false,
  });
});

describe("useConversationsStore.sendMessage", () => {
  it("creates a new conversation when none is active", () => {
    useConversationsStore.getState().sendMessage("Hello world", "en-US");

    const state = useConversationsStore.getState();
    expect(state.isResponding).toBe(true);
    expect(state.conversations).toHaveLength(1);
    expect(state.activeId).toBe(state.conversations[0]!.id);

    const convo = state.conversations[0]!;
    expect(convo.title).toBe("Hello world");
    expect(convo.messages).toHaveLength(1);
    expect(convo.messages[0]).toEqual({
      id: convo.messages[0]!.id,
      role: "user",
      content: "Hello world",
      createdAt: FIXED_NOW,
    });
  });

  it("appends the assistant reply when the scheduler flushes", () => {
    useConversationsStore.getState().sendMessage("Hello world", "en-US");
    scheduler.flush();

    const state = useConversationsStore.getState();
    expect(state.isResponding).toBe(false);
    expect(state.conversations[0]!.messages).toHaveLength(2);
    expect(state.conversations[0]!.messages[1]!.role).toBe("assistant");
    expect(state.conversations[0]!.messages[1]!.createdAt).toBe(FIXED_NOW);
  });

  it("appends to the active conversation instead of creating a new one", () => {
    useConversationsStore.getState().sendMessage("First", "en-US");
    scheduler.flush();
    const initialId = useConversationsStore.getState().activeId;
    expect(initialId).not.toBeNull();

    useConversationsStore.getState().sendMessage("Second", "en-US");
    const state = useConversationsStore.getState();
    expect(state.conversations).toHaveLength(1);
    expect(state.activeId).toBe(initialId);
    expect(state.conversations[0]!.messages).toHaveLength(3);
    expect(state.conversations[0]!.messages[2]!.content).toBe("Second");

    scheduler.flush();
    expect(useConversationsStore.getState().conversations[0]!.messages).toHaveLength(4);
    expect(useConversationsStore.getState().conversations[0]!.messages[3]!.role).toBe("assistant");
  });

  it("ignores empty/whitespace messages", () => {
    useConversationsStore.getState().sendMessage("   ", "en-US");
    expect(useConversationsStore.getState().conversations).toEqual([]);
    expect(useConversationsStore.getState().isResponding).toBe(false);
  });

  it("ignores a second send while a reply is pending (isResponding guard)", () => {
    useConversationsStore.getState().sendMessage("First", "en-US");
    useConversationsStore.getState().sendMessage("Second", "en-US");

    const state = useConversationsStore.getState();
    expect(state.conversations).toHaveLength(1);
    expect(state.conversations[0]!.messages).toHaveLength(1);
    expect(state.conversations[0]!.messages[0]!.content).toBe("First");

    scheduler.flush();
    expect(useConversationsStore.getState().conversations[0]!.messages).toHaveLength(2);
  });
});

describe("useConversationsStore navigation", () => {
  it("newChat clears activeId but preserves conversations", () => {
    useConversationsStore.getState().sendMessage("Hello", "en-US");
    scheduler.flush();
    expect(useConversationsStore.getState().activeId).not.toBeNull();

    useConversationsStore.getState().newChat();
    expect(useConversationsStore.getState().activeId).toBeNull();
    expect(useConversationsStore.getState().conversations).toHaveLength(1);
  });

  it("selectConversation sets activeId", () => {
    useConversationsStore.getState().selectConversation("conv-1");
    expect(useConversationsStore.getState().activeId).toBe("conv-1");
  });
});

describe("useConversationsStore.renameConversation", () => {
  it("renames the matching conversation", () => {
    useConversationsStore.setState({
      conversations: [
        { id: "c1", title: "Old", messages: [], createdAt: 0, updatedAt: 0 },
      ],
    });
    useConversationsStore.getState().renameConversation("c1", "New title");
    expect(useConversationsStore.getState().conversations[0]!.title).toBe("New title");
  });

  it("ignores blank titles", () => {
    useConversationsStore.setState({
      conversations: [
        { id: "c1", title: "Old", messages: [], createdAt: 0, updatedAt: 0 },
      ],
    });
    useConversationsStore.getState().renameConversation("c1", "   ");
    expect(useConversationsStore.getState().conversations[0]!.title).toBe("Old");
  });
});

describe("useConversationsStore.removeConversation", () => {
  it("removes the conversation and clears activeId when it was active", () => {
    useConversationsStore.setState({
      conversations: [
        { id: "c1", title: "A", messages: [], createdAt: 0, updatedAt: 0 },
        { id: "c2", title: "B", messages: [], createdAt: 0, updatedAt: 0 },
      ],
      activeId: "c1",
    });
    useConversationsStore.getState().removeConversation("c1");
    expect(useConversationsStore.getState().conversations.map((c) => c.id)).toEqual(["c2"]);
    expect(useConversationsStore.getState().activeId).toBeNull();
  });

  it("leaves activeId alone when a non-active conversation is removed", () => {
    useConversationsStore.setState({
      conversations: [
        { id: "c1", title: "A", messages: [], createdAt: 0, updatedAt: 0 },
        { id: "c2", title: "B", messages: [], createdAt: 0, updatedAt: 0 },
      ],
      activeId: "c1",
    });
    useConversationsStore.getState().removeConversation("c2");
    expect(useConversationsStore.getState().activeId).toBe("c1");
  });
});

describe("useConversationsStore.clearConversations", () => {
  it("empties everything and cancels the pending reply", () => {
    useConversationsStore.getState().sendMessage("Hello", "en-US");
    expect(scheduler.pendingCount).toBe(1);

    useConversationsStore.getState().clearConversations();
    expect(useConversationsStore.getState().conversations).toEqual([]);
    expect(useConversationsStore.getState().activeId).toBeNull();
    expect(useConversationsStore.getState().isResponding).toBe(false);

    // Flushing after clear must not resurrect any reply.
    scheduler.flush();
    expect(useConversationsStore.getState().conversations).toEqual([]);
    expect(useConversationsStore.getState().isResponding).toBe(false);
  });
});

describe("useConversationsStore persistence", () => {
  it("persists conversations and activeId under the v1 key", () => {
    useConversationsStore.setState({
      conversations: [
        { id: "c1", title: "Persisted", messages: [], createdAt: 0, updatedAt: 0 } satisfies Conversation,
      ],
      activeId: "c1",
    });
    const raw = window.localStorage.getItem(CONVERSATIONS_STORAGE_KEY);
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw!) as { state: { conversations: Conversation[]; activeId: string | null } };
    expect(parsed.state.conversations).toHaveLength(1);
    expect(parsed.state.conversations[0]!.title).toBe("Persisted");
    expect(parsed.state.activeId).toBe("c1");
  });

  it("does not persist isResponding", () => {
    useConversationsStore.setState({ isResponding: true });
    const raw = window.localStorage.getItem(CONVERSATIONS_STORAGE_KEY);
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw!) as { state: Record<string, unknown> };
    expect(parsed.state).not.toHaveProperty("isResponding");
  });
});

import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";
import type { ChatMessage, Conversation } from "../../lib/types";
import type { Locale } from "../../i18n/i18n";
import {
  createAssistantReply,
  createId,
  createSeedConversations,
  deriveTitle,
} from "../../lib/mock-data";

/** localStorage key under which the prototype persists its conversation state. */
export const CONVERSATIONS_STORAGE_KEY = "ora.web-client.conversations.v1";
const REPLY_DELAY_MS = 650;

/** Handle returned by a scheduler so a pending reply can be cancelled. */
export interface ReplyHandle {
  clear(): void;
}

/** Abstraction over `setTimeout` so tests can drive assistant replies synchronously. */
export interface ReplyScheduler {
  schedule(delayMs: number, fn: () => void): ReplyHandle;
}

const windowScheduler: ReplyScheduler = {
  schedule(delayMs, fn) {
    const handle = window.setTimeout(fn, delayMs);
    return { clear: () => window.clearTimeout(handle) };
  },
};

let replyScheduler: ReplyScheduler = windowScheduler;
let nowProvider: () => number = () => Date.now();

/** Test-only: replaces the reply scheduler so tests can flush replies deterministically. */
export function setReplyScheduler(scheduler: ReplyScheduler): void {
  replyScheduler = scheduler;
}

/** Test-only: replaces the clock so seeded timestamps are deterministic. */
export function setNowProvider(provider: () => number): void {
  nowProvider = provider;
}

/** Resets test-only overrides back to the browser defaults. */
export function resetConversationsDeps(): void {
  replyScheduler = windowScheduler;
  nowProvider = () => Date.now();
}

interface ConversationsState {
  conversations: Conversation[];
  activeId: string | null;
  isResponding: boolean;
  newChat: () => void;
  selectConversation: (id: string) => void;
  sendMessage: (text: string, locale: Locale) => void;
  renameConversation: (id: string, title: string) => void;
  removeConversation: (id: string) => void;
  clearConversations: () => void;
}

/** Tracks the pending assistant reply so cancel/clear/unmount can stop it mid-flight. */
let pendingReply: ReplyHandle | null = null;

/** Cancels the pending reply, if any, and forgets it. */
function clearPendingReply(): void {
  if (pendingReply) {
    pendingReply.clear();
    pendingReply = null;
  }
}

/**
 * Owns the conversation list and active selection, mirroring state to localStorage
 * and simulating an assistant reply through the injected scheduler. Replaces the
 * legacy `useConversations` hook with a single source of truth consumable from
 * any component without props drilling.
 */
export const useConversationsStore = create<ConversationsState>()(
  persist(
    (set, get) => ({
      // Seed fresh conversations for first load; persist overwrites this if storage has data.
      conversations: createSeedConversations(nowProvider()),
      activeId: null,
      isResponding: false,

      newChat: () => set({ activeId: null }),

      selectConversation: (id) => set({ activeId: id }),

      sendMessage: (text, locale) => {
        const content = text.trim();
        const state = get();
        if (!content || state.isResponding) return;

        const now = nowProvider();
        const userMessage: ChatMessage = { id: createId(), role: "user", content, createdAt: now };
        const currentActiveId = state.activeId;
        const targetId = currentActiveId ?? createId();

        if (currentActiveId) {
          set((s) => ({
            conversations: s.conversations.map((c) =>
              c.id === currentActiveId
                ? { ...c, messages: [...c.messages, userMessage], updatedAt: now }
                : c,
            ),
          }));
        } else {
          const conversation: Conversation = {
            id: targetId,
            title: deriveTitle(content),
            messages: [userMessage],
            createdAt: now,
            updatedAt: now,
          };
          set((s) => ({
            conversations: [conversation, ...s.conversations],
            activeId: targetId,
          }));
        }

        set({ isResponding: true });

        // Guard against a leaked prior handle even though isResponding should prevent re-entry.
        clearPendingReply();
        pendingReply = replyScheduler.schedule(REPLY_DELAY_MS, () => {
          const replyAt = nowProvider();
          const assistantMessage: ChatMessage = {
            id: createId(),
            role: "assistant",
            content: createAssistantReply(content, locale),
            createdAt: replyAt,
          };
          set((s) => ({
            conversations: s.conversations.map((c) =>
              c.id === targetId
                ? { ...c, messages: [...c.messages, assistantMessage], updatedAt: replyAt }
                : c,
            ),
            isResponding: false,
          }));
          pendingReply = null;
        });
      },

      renameConversation: (id, title) => {
        const next = title.trim();
        if (!next) return;
        set((s) => ({
          conversations: s.conversations.map((c) => (c.id === id ? { ...c, title: next } : c)),
        }));
      },

      removeConversation: (id) =>
        set((s) => ({
          conversations: s.conversations.filter((c) => c.id !== id),
          activeId: s.activeId === id ? null : s.activeId,
        })),

      clearConversations: () => {
        clearPendingReply();
        set({ conversations: [], activeId: null, isResponding: false });
      },
    }),
    {
      name: CONVERSATIONS_STORAGE_KEY,
      storage: createJSONStorage(() => window.localStorage),
      // Only persist data, never the transient responding flag.
      partialize: (state) => ({ conversations: state.conversations, activeId: state.activeId }),
    },
  ),
);

/** Tears down any pending reply; call on shell unmount to avoid stray state updates. */
export function disposeConversationsStore(): void {
  clearPendingReply();
}

/**
 * Names the conversation a selection is looking at, for frontend-only state that
 * belongs to one conversation rather than one task.
 *
 * A live session keys on its own id, so sibling sessions under the same task stay
 * independent. A selection with no session yet — a conversation being composed
 * before its session exists — falls back to its task, and is rekeyed onto the real
 * session id once the send mints one.
 */
export function conversationKeyFor(selection: {
  sessionId: string | null;
  taskId: string | null;
}): string {
  if (selection.sessionId !== null) return selection.sessionId;
  if (selection.taskId !== null) return `task:${selection.taskId}`;
  return "__none__";
}

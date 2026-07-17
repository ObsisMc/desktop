/**
 * Centralised react-query cache keys for the app shell.
 *
 * Keeping keys in one place lets mutations invalidate exactly the queries they
 * share data with, without scattering string literals across hook files.
 */
export const queryKeys = {
  projects: ["projects"] as const,
  tasks: ["tasks"] as const,
  sessions: ["sessions"] as const,
  agents: ["agents"] as const,
  skills: ["skills"] as const,
};

export type WorkspaceQueryKey = readonly ["projects"] | readonly ["tasks"] | readonly ["sessions"];

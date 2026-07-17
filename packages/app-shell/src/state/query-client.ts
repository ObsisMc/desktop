import { QueryClient, type QueryClientConfig } from "@tanstack/react-query";

/**
 * Default cache + retry policy for the Ora app shell.
 *
 * - `retry: 1` softens transient transport errors without masking contract
 *   violations behind long retry storms.
 * - `refetchOnWindowFocus: false` keeps the desktop/web prototype from
 *   re-fetching the entire workspace every time focus returns, which would
 *   reset selection-derived UI under the mock transport.
 */
const DEFAULT_CONFIG: QueryClientConfig = {
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
      staleTime: 30_000,
    },
    mutations: {
      retry: 0,
    },
  },
};

/** Creates a fresh, isolated QueryClient for one AppShell instance. */
export function createAppQueryClient(): QueryClient {
  return new QueryClient(DEFAULT_CONFIG);
}

import { createElement, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, type RenderHookResult } from "@testing-library/react";
import type { ContractsClient } from "@ora/contracts";
import { ContractsClientContext } from "../contracts-client-context";

/** Builds a QueryClient with retries disabled so tests fail fast on transport errors. */
export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0 },
      mutations: { retry: false },
    },
  });
}

/** Wraps children with QueryClient + ContractsClient providers for hook tests. */
export function createHookWrapper(client: ContractsClient, queryClient: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(ContractsClientContext.Provider, { value: client }, children),
    );
  };
}

/** Renders a hook with both providers set up and returns the result + QueryClient. */
export function renderHookWithClient<TResult>(
  hook: () => TResult,
  client: ContractsClient,
  queryClient: QueryClient = createTestQueryClient(),
): RenderHookResult<TResult, TResult> & { queryClient: QueryClient } {
  const result = renderHook(hook, { wrapper: createHookWrapper(client, queryClient) });
  return { ...result, queryClient };
}

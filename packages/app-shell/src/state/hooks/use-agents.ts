import { useQuery } from "@tanstack/react-query";
import { useContractsClient } from "../../contracts-client-context";
import { queryKeys } from "./query-keys";

/** Loads configurable agents through the contracts client and caches them. */
export function useAgents() {
  const client = useContractsClient();
  return useQuery({
    queryKey: queryKeys.agents,
    queryFn: () => client.agent.list({}).then((response) => response.agents),
  });
}

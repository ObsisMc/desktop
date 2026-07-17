import { useQuery } from "@tanstack/react-query";
import { useContractsClient } from "../../contracts-client-context";
import { queryKeys } from "./query-keys";

/** Loads the visible task list through the contracts client and caches it. */
export function useTasks() {
  const client = useContractsClient();
  return useQuery({
    queryKey: queryKeys.tasks,
    queryFn: () => client.task.list({}).then((response) => response.tasks),
  });
}

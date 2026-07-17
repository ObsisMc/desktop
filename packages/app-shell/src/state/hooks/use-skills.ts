import { useQuery } from "@tanstack/react-query";
import { useContractsClient } from "../../contracts-client-context";
import { queryKeys } from "./query-keys";

/** Loads configurable skills through the contracts client and caches them. */
export function useSkills() {
  const client = useContractsClient();
  return useQuery({
    queryKey: queryKeys.skills,
    queryFn: () => client.skill.list({}).then((response) => response.skills),
  });
}

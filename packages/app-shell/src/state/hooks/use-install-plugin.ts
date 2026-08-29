import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useContractsClient } from "../../contracts-client-context";
import {
  useOptionalPlatform,
  type PluginInstallProgress,
} from "../../platform";
import { queryKeys } from "./query-keys";

/**
 * Installs one marketplace plugin and refreshes the installed and available
 * surfaces once the backend settles. The optional `signal` lets the caller
 * cancel the pending request; the installed lookup is refreshed either way.
 */
export function useInstallPlugin(pluginId: string) {
  const client = useContractsClient();
  const platform = useOptionalPlatform();
  const queryClient = useQueryClient();
  const [progress, setProgress] = useState<PluginInstallProgress | null>(null);
  const invalidate = () =>
    Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.installedPlugins }),
      queryClient.invalidateQueries({ queryKey: queryKeys.availablePlugins }),
    ]);

  const mutation = useMutation({
    mutationFn: ({ signal }: { signal?: AbortSignal } = {}) =>
      client.plugin.install({ pluginId }, { signal }),
    onSettled: invalidate,
  });

  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;
    void platform?.pluginMarketplace
      ?.onInstallProgress((next) => {
        if (active && next.pluginId === pluginId) setProgress(next);
      })
      .then((stop) => {
        if (active) unsubscribe = stop;
        else stop();
      });
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [platform?.pluginMarketplace, pluginId]);

  const mutate = (...args: Parameters<typeof mutation.mutate>) => {
    setProgress(null);
    mutation.mutate(...args);
  };
  const mutateAsync = (...args: Parameters<typeof mutation.mutateAsync>) => {
    setProgress(null);
    return mutation.mutateAsync(...args);
  };

  return { ...mutation, mutate, mutateAsync, progress };
}

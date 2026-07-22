import { useTranslation } from "react-i18next";
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@ora/ui";
import { IconCheck, IconChevronDown } from "@tabler/icons-react";
import type { ModelProvider } from "../../state/stores/settings-store";
import { useSettingsStore } from "../../state/stores/settings-store";
import { PROVIDER_LABELS, PROVIDER_MODELS, PROVIDERS } from "./model-catalog";
import { ProviderLogo } from "./provider-logos";

/**
 * The composer's model picker. Collapsed, it shows only the provider logo and
 * the active model name; on hover (or while its menu is open) it expands to slot
 * the provider name between the two. The trigger carries no chrome of its own —
 * a muted fill appears only on hover — so it sits quietly beside the send button.
 *
 * Provider and model are persisted in the settings store; there is no backend
 * contract for switching them yet, so this drives the local prototype state only.
 */
export function ModelSelector({ disabled = false }: { disabled?: boolean }) {
  const { t } = useTranslation();
  const provider = useSettingsStore((state) => state.settings.provider);
  const model = useSettingsStore((state) => state.settings.model);
  const updateSettings = useSettingsStore((state) => state.updateSettings);

  const selectModel = (nextProvider: ModelProvider, nextModel: string) =>
    updateSettings({ provider: nextProvider, model: nextModel });

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={disabled}
            aria-label={t("chat.modelSelector.label")}
            className="group/model h-7 gap-1.5 rounded-md px-2 text-xs font-normal text-muted-foreground hover:text-foreground"
          />
        }
      >
        <ProviderLogo provider={provider} className="size-3.5 shrink-0" />
        {/* The provider name is width-animated in via a 0fr → 1fr grid so the
            button grows smoothly on hover instead of snapping wider. */}
        <span className="grid grid-cols-[0fr] opacity-0 transition-all duration-200 group-hover/model:grid-cols-[1fr] group-hover/model:opacity-100 group-aria-expanded/model:grid-cols-[1fr] group-aria-expanded/model:opacity-100">
          <span className="min-w-0 overflow-hidden whitespace-nowrap">{PROVIDER_LABELS[provider]}</span>
        </span>
        <span className="whitespace-nowrap">{model}</span>
        <IconChevronDown className="size-3 shrink-0 opacity-50" aria-hidden="true" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" side="top" className="w-56">
        {PROVIDERS.map((candidate) => (
          <DropdownMenuGroup key={candidate} className="p-1">
            <DropdownMenuLabel className="flex items-center gap-1.5 px-2 py-1.5 text-xs font-normal text-muted-foreground">
              <ProviderLogo provider={candidate} className="size-3.5" />
              {PROVIDER_LABELS[candidate]}
            </DropdownMenuLabel>
            {PROVIDER_MODELS[candidate].map((candidateModel) => (
              <DropdownMenuItem
                key={candidateModel}
                className="gap-1.5 rounded-sm px-2 py-1.5 text-xs"
                onClick={() => selectModel(candidate, candidateModel)}
              >
                {candidateModel}
                {candidate === provider && candidateModel === model && <IconCheck className="ml-auto size-4" />}
              </DropdownMenuItem>
            ))}
          </DropdownMenuGroup>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

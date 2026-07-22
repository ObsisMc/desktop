import type { ModelProvider } from "../../state/stores/settings-store";

/**
 * The models offered per provider. This is a hard-coded prototype catalog: the
 * backend contract does not expose a model list yet, so both the composer's
 * model selector and the settings dialog read from here to stay in sync.
 */
export const PROVIDER_MODELS: Record<ModelProvider, string[]> = {
  openai: ["gpt-5.1-codex", "gpt-5.1", "gpt-4.1"],
  anthropic: ["claude-sonnet-4.5", "claude-opus-4.1"],
  local: ["qwen3-coder", "deepseek-r1"],
};

/** Human-facing provider names shown next to the logo when the selector expands. */
export const PROVIDER_LABELS: Record<ModelProvider, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  local: "Local",
};

/** The provider order used when listing every provider's models. */
export const PROVIDERS = Object.keys(PROVIDER_MODELS) as ModelProvider[];

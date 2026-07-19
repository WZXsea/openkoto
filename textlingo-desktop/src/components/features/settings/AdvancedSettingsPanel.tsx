import { useTranslation } from "react-i18next";
import type { AppConfig } from "../../../lib/tauri";
import { Input } from "../../ui/input";

export const DEFAULT_BATCH_TRANSLATION_CONCURRENCY = 3;
export const MIN_BATCH_TRANSLATION_CONCURRENCY = 1;
export const MAX_BATCH_TRANSLATION_CONCURRENCY = 10;

export function normalizeBatchTranslationConcurrency(value: unknown): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return DEFAULT_BATCH_TRANSLATION_CONCURRENCY;
  }
  return Math.min(
    MAX_BATCH_TRANSLATION_CONCURRENCY,
    Math.max(MIN_BATCH_TRANSLATION_CONCURRENCY, Math.trunc(parsed)),
  );
}

export interface AdvancedSettingsPanelProps {
  config: AppConfig;
  onConfigChange: (config: AppConfig) => void;
}

export function AdvancedSettingsPanel({ config, onConfigChange }: AdvancedSettingsPanelProps) {
  const { t } = useTranslation();

  return (
    <section className="space-y-4" aria-label={t("settings.nav.advanced")}>
      <div>
        <label htmlFor="batch-translation-concurrency" className="block text-sm font-medium text-foreground mb-2">
          {t("settings.batchTranslationConcurrency", "Batch explanation concurrency")}
        </label>
        <Input
          id="batch-translation-concurrency"
          type="number"
          min={MIN_BATCH_TRANSLATION_CONCURRENCY}
          max={MAX_BATCH_TRANSLATION_CONCURRENCY}
          step={1}
          value={config.batch_translation_concurrency ?? DEFAULT_BATCH_TRANSLATION_CONCURRENCY}
          onChange={(event) =>
            onConfigChange({
              ...config,
              batch_translation_concurrency: normalizeBatchTranslationConcurrency(event.target.value),
            })
          }
        />
        <p className="mt-1 text-xs text-muted-foreground">
          {t(
            "settings.batchTranslationConcurrencyHelp",
            "Controls how many segments are explained at the same time. Higher values are faster but may hit model rate limits.",
          )}
        </p>
      </div>
    </section>
  );
}

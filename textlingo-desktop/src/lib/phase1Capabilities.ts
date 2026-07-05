import type { AppConfig } from "./tauri";

export const phase1Capabilities = {
  youtubeImport: false,
  webImport: false,
  pdfTranslation: false,
  ktvExport: false,
  autoSubtitleExtraction: false,
  updateCheck: false,
} as const;

export type Phase1Capability = keyof typeof phase1Capabilities;

export function isPhase1CapabilityEnabled(capability: Phase1Capability): boolean {
  return phase1Capabilities[capability];
}

export function hasActiveModelConfig(config: AppConfig | null | undefined): boolean {
  if (!config?.active_model_id || !config.model_configs?.length) {
    return false;
  }

  return config.model_configs.some((modelConfig) => modelConfig.id === config.active_model_id);
}

import type { FeatureBoundary } from "../types";

export const mediaFeature = {
  id: "media",
  label: "Media",
  entry: "src/features/media",
  status: "shell-only",
  phase1Scope: "Local audio, local video, subtitle import, playback, and export boundaries.",
  legacyComponents: [
    "src/components/features/LocalAudioImportForm.tsx",
    "src/components/features/LocalVideoImportForm.tsx",
    "src/components/features/LocalSubtitleImportForm.tsx",
    "src/components/features/VideoSubtitlePlayer.tsx",
    "src/components/features/KtvExportPage.tsx",
  ],
  notes: [
    "YouTube and automatic subtitle extraction stay disabled by Phase 1 gates.",
    "PR-2 does not introduce new media sidecars or external processors.",
  ],
} as const satisfies FeatureBoundary;

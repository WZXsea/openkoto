import type { FeatureBoundary } from "../types";

export const materialsFeature = {
  id: "materials",
  label: "Materials",
  entry: "src/features/materials",
  status: "shell-only",
  phase1Scope: "Material list, manual article creation, import dialog, and drop import boundaries.",
  legacyComponents: [
    "src/components/features/ArticleList.tsx",
    "src/components/features/NewArticleForm.tsx",
    "src/components/features/NewMaterialDialog.tsx",
    "src/components/features/DropImportOverlay.tsx",
  ],
  notes: [
    "PR-2 does not split the existing material dialogs or forms.",
    "External source import surfaces remain governed by Phase 1 capability gates.",
  ],
} as const satisfies FeatureBoundary;

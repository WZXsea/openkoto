import type { FeatureBoundary } from "../types";

export const localLearningFeature = {
  id: "local-learning",
  label: "Local Learning",
  entry: "src/features/local-learning",
  status: "shell-only",
  phase1Scope: "Local word pack management, recitation panel, and pack selection boundaries.",
  legacyComponents: [
    "src/components/features/WordPackManager.tsx",
    "src/components/features/WordRecitePanel.tsx",
    "src/components/features/SelectPackDialog.tsx",
  ],
  notes: [
    "Memory scheduling remains local to existing app behavior in PR-2.",
    "Anki and FSRS handoff belongs to a later phase.",
  ],
} as const satisfies FeatureBoundary;

import type { FeatureBoundary } from "../types";

export const localLearningFeature = {
  id: "local-learning",
  label: "Local Learning",
  entry: "src/features/local-learning",
  status: "active",
  phase1Scope: "Reader selection learning items, candidate inbox, local word pack handoff, recitation panel, and pack selection boundaries.",
  legacyComponents: [
    "src/components/features/LearningCandidateBox.tsx",
    "src/components/features/WordPackManager.tsx",
    "src/components/features/WordRecitePanel.tsx",
    "src/components/features/SelectPackDialog.tsx",
  ],
  notes: [
    "PR-5 stores local learning item candidates in the backend and accepts them into existing word packs.",
    "Memory scheduling remains local to existing app behavior in PR-5.",
    "Anki and FSRS handoff belongs to a later phase.",
  ],
} as const satisfies FeatureBoundary;

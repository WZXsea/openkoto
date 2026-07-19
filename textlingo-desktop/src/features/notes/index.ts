import type { FeatureBoundary } from "../types";

export const notesFeature = {
  id: "notes",
  label: "Notes",
  entry: "src/features/notes",
  status: "shell-only",
  phase1Scope: "Vocabulary, grammar, favorite cards, and source-context note boundaries.",
  legacyComponents: [
    "src/components/features/FavoritesCards.tsx",
    "src/components/features/FavoritesPage.tsx",
  ],
  notes: [
    "Vocabulary items must continue to preserve source context.",
    "PR-2 does not add unattended bulk card generation.",
  ],
} as const satisfies FeatureBoundary;

import type { FeatureBoundary } from "../types";

export const readerFeature = {
  id: "reader",
  label: "Reader",
  entry: "src/features/reader",
  status: "shell-only",
  phase1Scope: "Article reader shell, explanation panel, bookmarks, and reading interaction boundaries.",
  legacyComponents: [
    "src/components/features/ArticleReader.tsx",
    "src/components/features/ArticleExplanationPanel.tsx",
    "src/components/features/BookmarkSidebar.tsx",
  ],
  notes: [
    "Detailed reader component decomposition is outside PR-2.",
    "Book-specific readers are tracked by the books feature boundary.",
  ],
} as const satisfies FeatureBoundary;

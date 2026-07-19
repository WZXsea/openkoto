import type { FeatureBoundary } from "../types";

export const booksFeature = {
  id: "books",
  label: "Books",
  entry: "src/features/books",
  status: "shell-only",
  phase1Scope: "Book import and EPUB/TXT/PDF reading boundaries.",
  legacyComponents: [
    "src/components/features/BookImportForm.tsx",
    "src/components/features/BookReader.tsx",
    "src/components/features/EpubReader.tsx",
    "src/components/features/TxtReader.tsx",
    "src/components/features/PdfReader.tsx",
  ],
  notes: [
    "Book-specific readers stay in legacy components during PR-2.",
    "Future migrations should keep file parsing local to the app shell unless a later phase approves integration.",
  ],
} as const satisfies FeatureBoundary;

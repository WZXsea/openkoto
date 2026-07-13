import type { FeatureBoundary } from "../types";

export {
  READING_PROGRESS_COMPLETION_THRESHOLD,
  clampProgressRatio,
  createEpubCfiLocator,
  createMediaTimeLocator,
  createPageLocator,
  createReadingProgressUpdate,
  createSegmentLocator,
  getEpubCfiFromLocator,
  getInitialProgressForReader,
  getMediaTimeFromLocator,
  getPageNumberFromLocator,
  getPageNumberFromProgress,
  getReadingProgressStatus,
  getSegmentPositionFromLocator,
  useReadingProgressReporter,
} from "./readingProgress";
export type {
  ReaderKind,
  ReadingProgressChangeHandler,
  ReadingProgressLocator,
  ReadingProgressStatus,
  ReadingProgressUpdate,
} from "./readingProgress";
export {
  SOURCE_LOCATOR_VERSION,
  assertValidSourceLocator,
  createTextRangeLocator,
  parseSourceLocator,
  withLocatorVersion,
} from "./sourceLocator";
export type { SourceLocatorV1, TextQuoteSelector } from "./sourceLocator";

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

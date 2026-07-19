import { assistantFeature } from "./assistant";
import { booksFeature } from "./books";
import { localLearningFeature } from "./local-learning";
import { materialsFeature } from "./materials";
import { mediaFeature } from "./media";
import { notesFeature } from "./notes";
import { readerFeature } from "./reader";
import { settingsFeature } from "./settings";

export { assistantFeature } from "./assistant";
export { booksFeature } from "./books";
export { localLearningFeature } from "./local-learning";
export { materialsFeature } from "./materials";
export { mediaFeature } from "./media";
export { notesFeature } from "./notes";
export { readerFeature } from "./reader";
export { settingsFeature } from "./settings";
export type { FeatureBoundary, FeatureBoundaryStatus, FeatureId } from "./types";

export const featureBoundaries = [
  materialsFeature,
  readerFeature,
  notesFeature,
  localLearningFeature,
  assistantFeature,
  settingsFeature,
  mediaFeature,
  booksFeature,
] as const;

export type FeatureId =
  | "assistant"
  | "books"
  | "local-learning"
  | "materials"
  | "media"
  | "notes"
  | "reader"
  | "settings";

export type FeatureBoundaryStatus = "shell-only" | "active";

export interface FeatureBoundary {
  id: FeatureId;
  label: string;
  entry: `src/features/${string}`;
  status: FeatureBoundaryStatus;
  phase1Scope: string;
  legacyComponents: readonly `src/components/features/${string}`[];
  notes: readonly string[];
}

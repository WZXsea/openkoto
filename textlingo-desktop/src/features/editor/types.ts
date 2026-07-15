import type { ArticleSegment, SegmentExplanation } from "../../types";

export type MaterialBlockType =
  | "paragraph"
  | "heading"
  | "list_item"
  | "quote"
  | "divider";

export interface MaterialDocumentSegment extends Omit<ArticleSegment, "article_id" | "created_at"> {
  block_segment_order?: number;
  text_sha256?: string;
  reading_status?: "current" | "stale" | "missing" | (string & {});
  translation_status?: "current" | "stale" | "missing" | (string & {});
  explanation_status?: "current" | "stale" | "missing" | (string & {});
  explanation?: SegmentExplanation;
}

export interface MaterialEditorBlock {
  id?: string;
  block_type: MaterialBlockType;
  block_order: number;
  text: string;
  attrs: Record<string, unknown>;
  segments?: MaterialDocumentSegment[];
}

export interface MaterialDerivedSummary {
  readings?: number;
  translations?: number;
  explanations?: number;
  stale_readings?: number;
  stale_translations?: number;
  stale_explanations?: number;
  [key: string]: unknown;
}

export interface MaterialDraft {
  material_id: string;
  base_revision: number;
  blocks: MaterialEditorBlock[];
  updated_at: string;
  is_stale: boolean;
}

export interface MaterialDocument {
  material_id: string;
  title: string;
  current_revision: number;
  content_sha256?: string | null;
  blocks: MaterialEditorBlock[];
  segments?: MaterialDocumentSegment[];
  draft?: MaterialDraft | null;
  derived_summary?: MaterialDerivedSummary;
}

export interface MaterialEditImpact {
  inserted_blocks: number;
  updated_blocks: number;
  deleted_blocks: number;
  moved_blocks: number;
  changed_segments: number;
  deleted_segments: number;
  stale_readings: number;
  stale_translations: number;
  stale_explanations: number;
  affected_annotations: number;
  affected_learning_items: number;
  annotation_reanchors?: {
    exact: number;
    text: number;
    ambiguous: number;
    orphaned: number;
  };
}

export interface MaterialEditPreview {
  base_revision: number;
  next_revision: number;
  content_sha256?: string | null;
  impact: MaterialEditImpact;
  preview_token: string;
}

export interface MaterialEditCommitResult {
  document: MaterialDocument;
  impact: MaterialEditImpact;
}

export interface MaterialRevisionSummary {
  revision: number;
  parent_revision?: number | null;
  action: string;
  content_sha256?: string | null;
  change_summary?: MaterialEditImpact | Record<string, unknown> | null;
  created_at: string;
}

export interface MaterialRevisionDetail extends MaterialRevisionSummary {
  snapshot: MaterialDocument | { blocks: MaterialEditorBlock[]; [key: string]: unknown };
}

export interface MaterialEditableDerivativeResult {
  source_material_id: string;
  derivative_material_id: string;
  created: boolean;
}

export type MaterialEditorPhase =
  | "loading"
  | "editing"
  | "previewing"
  | "confirming"
  | "saving"
  | "conflict"
  | "error";

export function emptyMaterialEditImpact(): MaterialEditImpact {
  return {
    inserted_blocks: 0,
    updated_blocks: 0,
    deleted_blocks: 0,
    moved_blocks: 0,
    changed_segments: 0,
    deleted_segments: 0,
    stale_readings: 0,
    stale_translations: 0,
    stale_explanations: 0,
    affected_annotations: 0,
    affected_learning_items: 0,
  };
}

export function requiresMaterialEditConfirmation(impact: MaterialEditImpact): boolean {
  return impact.deleted_blocks > 0
    || impact.deleted_segments > 0
    || impact.stale_readings > 0
    || impact.stale_translations > 0
    || impact.stale_explanations > 0
    || impact.affected_annotations > 0
    || impact.affected_learning_items > 0;
}

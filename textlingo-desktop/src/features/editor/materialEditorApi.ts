import { invoke } from "@tauri-apps/api/core";

import type {
  MaterialDocument,
  MaterialDraft,
  MaterialEditCommitResult,
  MaterialEditorBlock,
  MaterialEditableDerivativeResult,
  MaterialEditPreview,
  MaterialRevisionDetail,
  MaterialRevisionSummary,
} from "./types";

const commands = {
  getDocument: "get_material_document_cmd",
  previewEdit: "preview_material_edit_cmd",
  commitEdit: "commit_material_edit_cmd",
  getDraft: "get_material_draft_cmd",
  saveDraft: "save_material_draft_cmd",
  deleteDraft: "delete_material_draft_cmd",
  listRevisions: "list_material_revisions_cmd",
  getRevision: "get_material_revision_cmd",
  restoreRevision: "restore_material_revision_cmd",
  updateDerived: "update_segment_derived_cmd",
  createDerivative: "create_editable_derivative_cmd",
} as const;

export interface MaterialEditRequest {
  base_revision: number;
  blocks: MaterialEditorBlock[];
}

export interface MaterialEditCommitRequest extends MaterialEditRequest {
  client_request_id: string;
  preview_token: string;
}

export const materialEditorApi = {
  getDocument(materialId: string) {
    return invoke<MaterialDocument>(commands.getDocument, { materialId });
  },

  previewEdit(materialId: string, request: MaterialEditRequest) {
    return invoke<MaterialEditPreview>(commands.previewEdit, { materialId, payload: request });
  },

  commitEdit(materialId: string, request: MaterialEditCommitRequest) {
    return invoke<MaterialEditCommitResult>(commands.commitEdit, { materialId, payload: request });
  },

  getDraft(materialId: string) {
    return invoke<MaterialDraft | null>(commands.getDraft, { materialId });
  },

  saveDraft(materialId: string, request: MaterialEditRequest) {
    return invoke<MaterialDraft>(commands.saveDraft, { materialId, payload: request });
  },

  deleteDraft(materialId: string) {
    return invoke<void>(commands.deleteDraft, { materialId });
  },

  listRevisions(materialId: string) {
    return invoke<MaterialRevisionSummary[]>(commands.listRevisions, { materialId });
  },

  getRevision(materialId: string, revision: number) {
    return invoke<MaterialRevisionDetail>(commands.getRevision, { materialId, revision });
  },

  restoreRevision(
    materialId: string,
    revision: number,
    baseRevision: number,
    clientRequestId: string,
    preserveDraft = false,
  ) {
    return invoke<MaterialEditCommitResult>(commands.restoreRevision, {
      materialId,
      revision,
      payload: {
        base_revision: baseRevision,
        client_request_id: clientRequestId,
        preserve_draft: preserveDraft,
      },
    });
  },

  updateSegmentDerived(materialId: string, payload: Record<string, unknown>) {
    return invoke(commands.updateDerived, { materialId, payload });
  },

  createEditableDerivative(materialId: string) {
    return invoke<MaterialEditableDerivativeResult>(commands.createDerivative, { materialId, payload: {} });
  },
};

export function isMaterialRevisionConflict(error: unknown): boolean {
  if (typeof error === "object" && error !== null && "code" in error) {
    return (error as { code?: unknown }).code === "material_revision_conflict";
  }
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("material_revision_conflict");
}

export function isMaterialPreviewChanged(error: unknown): boolean {
  if (typeof error === "object" && error !== null && "code" in error) {
    return (error as { code?: unknown }).code === "document_preview_changed";
  }
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("document_preview_changed");
}

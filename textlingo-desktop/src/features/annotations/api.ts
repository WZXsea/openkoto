import { invoke } from "@tauri-apps/api/core";

import type {
  Annotation,
  ConvertAnnotationResponse,
  CreateAnnotationInput,
  ListAnnotationsQuery,
  UpdateAnnotationInput,
} from "../../types";

export interface AnnotationsApi {
  list(query?: ListAnnotationsQuery): Promise<Annotation[]>;
  create(payload: CreateAnnotationInput): Promise<Annotation>;
  update(id: string, payload: UpdateAnnotationInput): Promise<Annotation>;
  remove(id: string): Promise<void>;
  convertToLearningItem(id: string): Promise<ConvertAnnotationResponse>;
}

/** Tauri command adapter for the PR-9 annotation workbench. */
export function createAnnotationsApi(): AnnotationsApi {
  return {
    list: (query) => invoke<Annotation[]>("list_annotations_cmd", { query: query ?? null }),
    create: (payload) => invoke<Annotation>("create_annotation_cmd", { payload }),
    update: (id, payload) => invoke<Annotation>("update_annotation_cmd", { id, payload }),
    remove: async (id) => {
      await invoke("delete_annotation_cmd", { id });
    },
    convertToLearningItem: (id) => invoke<ConvertAnnotationResponse>("convert_annotation_to_learning_item_cmd", { id }),
  };
}

export const annotationsApi = createAnnotationsApi();

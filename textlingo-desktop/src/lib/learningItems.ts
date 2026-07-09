import { invoke } from "@tauri-apps/api/core";
import type {
  CreateLearningItemFromSelectionInput,
  CreateLearningItemInput,
  LearningItem,
  ListLearningItemsQuery,
  UpdateLearningItemInput,
} from "../types";

export function listLearningItems(query?: ListLearningItemsQuery): Promise<LearningItem[]> {
  return invoke<LearningItem[]>("list_learning_items_cmd", { query: query ?? null });
}

export function createLearningItem(payload: CreateLearningItemInput): Promise<LearningItem> {
  return invoke<LearningItem>("create_learning_item_cmd", { payload });
}

export function createLearningItemFromSelection(
  payload: CreateLearningItemFromSelectionInput,
): Promise<LearningItem> {
  return invoke<LearningItem>("create_learning_item_from_selection_cmd", { payload });
}

export function updateLearningItem(
  id: string,
  payload: UpdateLearningItemInput,
): Promise<LearningItem> {
  return invoke<LearningItem>("update_learning_item_cmd", { id, payload });
}

export function deleteLearningItem(id: string): Promise<void> {
  return invoke<void>("delete_learning_item_cmd", { id });
}

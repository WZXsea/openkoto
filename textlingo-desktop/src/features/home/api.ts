import { invoke } from "@tauri-apps/api/core";

import type {
  HomeActivityApi,
  LearningActivityHeatmap,
  LearningActivityHeatmapDay,
  LearningActivityHeatmapQuery,
} from "./types";

function normalizeCount(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? Math.max(0, Math.trunc(value)) : 0;
}

function normalizeDay(value: Partial<LearningActivityHeatmapDay>): LearningActivityHeatmapDay {
  return {
    date: typeof value.date === "string" ? value.date : "",
    read_materials: normalizeCount(value.read_materials),
    learning_actions: normalizeCount(value.learning_actions),
    activity_score: normalizeCount(value.activity_score),
  };
}

export function createHomeActivityApi(): HomeActivityApi {
  return {
    getActivityHeatmap: async (query: LearningActivityHeatmapQuery) => {
      const response = await invoke<LearningActivityHeatmap>("get_learning_activity_heatmap_cmd", { query });
      return {
        start_date: response.start_date || query.start_date,
        end_date: response.end_date || query.end_date,
        days: Array.isArray(response.days) ? response.days.map(normalizeDay).filter((day) => day.date) : [],
      };
    },
  };
}

export const homeActivityApi = createHomeActivityApi();

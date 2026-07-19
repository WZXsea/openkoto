export interface LearningActivityHeatmapDay {
  date: string;
  read_materials: number;
  learning_actions: number;
  activity_score: number;
}

export interface LearningActivityHeatmap {
  start_date: string;
  end_date: string;
  days: LearningActivityHeatmapDay[];
}

export interface LearningActivityHeatmapQuery {
  start_date: string;
  end_date: string;
  timezone_offset_minutes: number;
}

export interface HomeActivityApi {
  getActivityHeatmap(query: LearningActivityHeatmapQuery): Promise<LearningActivityHeatmap>;
}

import type { FeatureBoundary } from "../types";

export const assistantFeature = {
  id: "assistant",
  label: "Assistant",
  entry: "src/features/assistant",
  status: "active",
  phase1Scope: "Agent sidebar, article chat, mind map panel, task observability, and artifact boundaries.",
  legacyComponents: [
    "src/components/features/AgentPanel.tsx",
    "src/components/features/ArticleChatAssistant.tsx",
    "src/components/features/AssistantSidebarShell.tsx",
    "src/components/features/ArticleMindMapPanel.tsx",
    "src/components/features/LogsPanel.tsx",
  ],
  notes: [
    "PR-12 adds a global task center and current-material task history without introducing an external Assistant runtime.",
    "First phase does not connect browser extensions, MCP, Zotero, Anki, or MinerU.",
  ],
} as const satisfies FeatureBoundary;

export { assistantTasksApi, createAssistantTasksApi } from "./api";
export { collectAssistantSourceReferences, useAssistantTaskCenter } from "./state";
export { ASSISTANT_TASK_STATUSES } from "./types";
export type {
  AssistantArtifact,
  AssistantSourceReference,
  AssistantTask,
  AssistantTaskDetail,
  AssistantTaskLineageEntry,
  AssistantTaskListQuery,
  AssistantTaskListResponse,
  AssistantTaskStatus,
  AssistantTaskTimelineEvent,
  AssistantTasksApi,
} from "./types";

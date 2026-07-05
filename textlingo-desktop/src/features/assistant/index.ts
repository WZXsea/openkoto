import type { FeatureBoundary } from "../types";

export const assistantFeature = {
  id: "assistant",
  label: "Assistant",
  entry: "src/features/assistant",
  status: "shell-only",
  phase1Scope: "Agent sidebar, article chat, mind map panel, and task log boundaries.",
  legacyComponents: [
    "src/components/features/AgentPanel.tsx",
    "src/components/features/ArticleChatAssistant.tsx",
    "src/components/features/AssistantSidebarShell.tsx",
    "src/components/features/ArticleMindMapPanel.tsx",
    "src/components/features/LogsPanel.tsx",
  ],
  notes: [
    "PR-2 keeps assistant UI implementation in legacy components.",
    "First phase does not connect new external agents, browser extensions, MCP, Zotero, Anki, or MinerU.",
  ],
} as const satisfies FeatureBoundary;

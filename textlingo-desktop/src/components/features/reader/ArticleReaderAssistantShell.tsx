import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { BookOpen, Sparkles } from "lucide-react";
import type { AssistantSourceReference } from "../../../features/assistant";
import type { Article, ArticleSegment } from "../../../types";
import { AgentPanel } from "../AgentPanel";
import { ArticleChatAssistant } from "../ArticleChatAssistant";
import { ArticleExplanationPanel } from "../ArticleExplanationPanel";
import { ArticleMindMapPanel } from "../ArticleMindMapPanel";
import { AssistantSidebarShell, type AssistantPanelMode } from "../AssistantSidebarShell";
import { AssistantTaskCenter } from "../AssistantTaskCenter";

export type ArticleReaderAssistantTab = "explanation" | "mind_map" | "chat" | "agent" | "tasks";

export interface ArticleReaderAssistantShellProps {
  article: Article;
  mainContent: ReactNode;
  showAssistant: boolean;
  activeTab: ArticleReaderAssistantTab;
  onTabChange: (tab: ArticleReaderAssistantTab) => void;
  canUseAi: boolean;
  aiUnavailableMessage: string;
  selectedSegment: ArticleSegment | null;
  isGeneratingExplanation: boolean;
  onGenerateExplanation: () => void;
  targetLanguage: string;
  selectedText: string;
  onNavigateAssistantSource?: (reference: AssistantSourceReference) => void;
}

const ASSISTANT_MODE_STORAGE_KEY = "article-reader-assistant-mode";

export function ArticleReaderAssistantShell({
  article,
  mainContent,
  showAssistant,
  activeTab,
  onTabChange,
  canUseAi,
  aiUnavailableMessage,
  selectedSegment,
  isGeneratingExplanation,
  onGenerateExplanation,
  targetLanguage,
  selectedText,
  onNavigateAssistantSource,
}: ArticleReaderAssistantShellProps) {
  const { t } = useTranslation();
  const aiDisabledPanel = (
    <div className="h-full flex flex-col items-center justify-center text-muted-foreground p-8 text-center">
      <Sparkles size={48} className="mb-4 opacity-50" />
      <p>{aiUnavailableMessage}</p>
    </div>
  );

  const sidebarTabs = [
    {
      value: "explanation",
      label: t("articleReader.explanation", "讲解"),
      content: !canUseAi && !selectedSegment?.explanation ? aiDisabledPanel : selectedSegment ? (
        <ArticleExplanationPanel
          segment={selectedSegment}
          explanation={selectedSegment.explanation || null}
          isLoading={isGeneratingExplanation}
          onRegenerate={onGenerateExplanation}
        />
      ) : (
        <div className="h-full flex flex-col items-center justify-center text-muted-foreground p-8 text-center">
          <BookOpen size={48} className="mb-4 opacity-50" />
          <p>{t("articleReader.selectSegment") || "Select a sentence to see explanation"}</p>
        </div>
      ),
    },
    {
      value: "mind_map",
      label: t("articleReader.mindMap", "思维导图"),
      content: canUseAi ? ({ panelMode }: { panelMode: AssistantPanelMode }) => (
        <ArticleMindMapPanel article={article} targetLanguage={targetLanguage} panelMode={panelMode} />
      ) : aiDisabledPanel,
    },
    {
      value: "chat",
      label: t("articleReader.chat", "对话"),
      content: canUseAi ? (
        <ArticleChatAssistant
          articleId={article.id}
          articleTitle={article.title}
          targetLanguage={targetLanguage}
          selectedText={selectedText || selectedSegment?.text || ""}
        />
      ) : aiDisabledPanel,
    },
    {
      value: "agent",
      label: t("assistant.mode.agent", "Agent"),
      content: canUseAi ? (
        <AgentPanel articleId={article.id} articleTitle={article.title} targetLanguage={targetLanguage} />
      ) : aiDisabledPanel,
    },
    {
      value: "tasks",
      label: "任务",
      content: (
        <AssistantTaskCenter
          mode="reader"
          articles={[article]}
          initialArticleId={article.id}
          onNavigateSource={onNavigateAssistantSource ?? (() => undefined)}
        />
      ),
    },
  ];

  return (
    <AssistantSidebarShell
      storageKey={ASSISTANT_MODE_STORAGE_KEY}
      showAssistant={showAssistant}
      shellTestId="article-reader-shell"
      mainPaneTestId="article-reader-main-pane"
      assistantPaneTestId="article-reader-assistant-pane"
      defaultTab="explanation"
      activeTab={activeTab}
      onTabChange={(value) => onTabChange(value as ArticleReaderAssistantTab)}
      tabs={sidebarTabs}
      mainContent={mainContent}
    />
  );
}

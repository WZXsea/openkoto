import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
  FileDown,
  FileText,
  Languages,
  Loader2,
  Minus,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  Sparkles,
  Split,
} from "lucide-react";
import type { Article, ArticleSegment } from "../../../types";
import type { ViewMode } from "../VideoSubtitlePlayer";
import { Button } from "../../ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../../ui/dropdown-menu";

interface ProgressValue {
  current: number;
  total: number;
}

export interface ArticleReaderHeaderProps {
  article: Article;
  segments: ArticleSegment[];
  hasSegments: boolean;
  onBack?: () => void;
  onNext?: () => void;
  onPrev?: () => void;
  hasNext?: boolean;
  hasPrev?: boolean;
  fontSize: number;
  onFontSizeChange: (fontSize: number) => void;
  viewMode: ViewMode;
  onViewModeChange: (viewMode: ViewMode) => void;
  isBatchTranslating: boolean;
  batchProgress: ProgressValue;
  canUseAi: boolean;
  aiUnavailableMessage: string;
  onBatchTranslate: () => void;
  isResegmenting: boolean;
  onResegment: () => void;
  onOpenCandidateBox: () => void;
  onArticleExport: (format: "md" | "docx", includeExplanation: boolean) => void;
  isEditing: boolean;
  onToggleEditing: () => void;
  isTranslating: boolean;
  translationProgress: ProgressValue | null;
  onTranslate: () => void;
  showAssistant: boolean;
  onToggleAssistant: () => void;
}

export function ArticleReaderHeader({
  article,
  segments,
  hasSegments,
  onBack,
  onNext,
  onPrev,
  hasNext,
  hasPrev,
  fontSize,
  onFontSizeChange,
  viewMode,
  onViewModeChange,
  isBatchTranslating,
  batchProgress,
  canUseAi,
  aiUnavailableMessage,
  onBatchTranslate,
  isResegmenting,
  onResegment,
  onOpenCandidateBox,
  onArticleExport,
  isEditing,
  onToggleEditing,
  isTranslating,
  translationProgress,
  onTranslate,
  showAssistant,
  onToggleAssistant,
}: ArticleReaderHeaderProps) {
  const { t } = useTranslation();
  const editDocumentLabel = isEditing
    ? t("articleReader.cancel")
    : t("articleReader.edit");

  return (
    <header className="flex flex-col gap-3 p-4 border-b border-border bg-card/50 backdrop-blur-sm supports-[backdrop-filter]:bg-card/50">
      <div className="flex items-center gap-4 min-w-0">
        {onBack && (
          <Button variant="ghost" size="sm" onClick={onBack} aria-label={t("common.back", "返回")} title={t("common.back", "返回")}>
            <ChevronLeft size={18} />
          </Button>
        )}
        <div className="min-w-0 overflow-hidden">
          <h1 className="text-xl font-semibold text-foreground truncate">
            {article.title || t("articleReader.untitled")}
          </h1>
          {hasSegments && (
            <div className="flex items-center gap-2 mt-1">
              <div className="flex items-center gap-3 px-2 py-0.5 bg-muted/40 rounded-md border border-border/50">
                <div className="flex items-center gap-1.5">
                  <div className="h-1.5 w-1.5 rounded-full bg-yellow-500" />
                  <span className="text-[10px] text-muted-foreground font-medium">
                    {segments.filter((segment) => segment.translation).length} / {segments.length}
                    <span className="ml-1 opacity-80">{t("articleReader.translated") || "已翻译"}</span>
                  </span>
                </div>
                <div className="w-px h-3 bg-border/50" />
                <div className="flex items-center gap-1.5">
                  <div className="h-1.5 w-1.5 rounded-full bg-green-500" />
                  <span className="text-[10px] text-muted-foreground font-medium">
                    {segments.filter((segment) => segment.explanation).length} / {segments.length}
                    <span className="ml-1 opacity-80">{t("articleReader.parsed") || "已解析"}</span>
                  </span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        {onPrev && (
          <Button variant="ghost" size="sm" onClick={onPrev} disabled={!hasPrev} title="Previous Article" aria-label="Previous Article">
            <ChevronLeft size={18} />
          </Button>
        )}
        {onNext && (
          <Button variant="ghost" size="sm" onClick={onNext} disabled={!hasNext} title="Next Article" aria-label="Next Article">
            <ChevronRight size={18} />
          </Button>
        )}

        <div className="w-px h-4 bg-border mx-1" />

        <div className="flex items-center gap-0.5 bg-muted/50 rounded-lg p-0.5 mr-2 border border-border">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onFontSizeChange(Math.max(12, fontSize - 2))}
            className="h-7 w-7 p-0 hover:bg-background text-foreground"
            title="Decrease font size"
            aria-label="Decrease font size"
          >
            <Minus size={14} />
          </Button>
          <span className="text-xs text-muted-foreground w-6 text-center" aria-label={`Font size ${fontSize}`}>{fontSize}</span>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onFontSizeChange(Math.min(32, fontSize + 2))}
            className="h-7 w-7 p-0 hover:bg-background text-foreground"
            title="Increase font size"
            aria-label="Increase font size"
          >
            <Plus size={14} />
          </Button>
        </div>

        {hasSegments ? (
          <>
            {!article.media_path && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant={viewMode !== "original" ? "default" : "secondary"}
                    size="sm"
                    title={t("articleReader.viewModeLabel") || "View Mode"}
                    className="h-8 md:h-9"
                    data-testid="reader-toolbar-view-mode-trigger"
                  >
                    {viewMode === "original" && <Eye size={16} />}
                    {viewMode === "bilingual" && <Split size={16} />}
                    {viewMode === "translation" && <Languages size={16} />}
                    <span className="ml-2 hidden xl:inline">
                      {t(`articleReader.viewMode.${viewMode}`) || (viewMode === "original" ? "Original" : viewMode === "bilingual" ? "Bilingual" : "Translation")}
                    </span>
                    <ChevronDown size={14} className="ml-1 opacity-50" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={() => onViewModeChange("original")}>
                    <div className="flex items-center justify-between w-full min-w-[120px]">
                      <span>{t("articleReader.viewMode.original") || "Original"}</span>
                      {viewMode === "original" && <Check size={14} />}
                    </div>
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => onViewModeChange("bilingual")}>
                    <div className="flex items-center justify-between w-full">
                      <span>{t("articleReader.viewMode.bilingual") || "Bilingual"}</span>
                      {viewMode === "bilingual" && <Check size={14} />}
                    </div>
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => onViewModeChange("translation")}>
                    <div className="flex items-center justify-between w-full">
                      <span>{t("articleReader.viewMode.translation") || "Translation"}</span>
                      {viewMode === "translation" && <Check size={14} />}
                    </div>
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            )}

            {isBatchTranslating ? (
              <div className="flex items-center gap-2 px-3 py-1.5 bg-muted rounded-md border border-border h-8 md:h-9">
                <Loader2 size={14} className="animate-spin text-primary" />
                <span className="text-xs text-muted-foreground font-mono">
                  {Math.round((batchProgress.current / batchProgress.total) * 100)}%
                </span>
              </div>
            ) : (
              <Button
                variant="secondary"
                size="sm"
                onClick={onBatchTranslate}
                disabled={isBatchTranslating || !hasSegments || !canUseAi}
                title={canUseAi ? t("articleReader.analyzeAll") : aiUnavailableMessage}
                className="h-8 md:h-9"
              >
                <Sparkles size={16} />
                <span className="ml-2 hidden xl:inline">{t("articleReader.analyzeAll") || "Deep Dive Translate"}</span>
              </Button>
            )}

            {!article.media_path && (
              <Button
                variant="secondary"
                size="sm"
                onClick={onResegment}
                disabled={isResegmenting}
                className="h-8 md:h-9"
                title={t("articleReader.resegment")}
              >
                {isResegmenting ? <Loader2 size={16} className="animate-spin" /> : <Split size={16} />}
                <span className="ml-2 hidden xl:inline">{t("articleReader.segment")}</span>
              </Button>
            )}

            {!article.media_path && (
              <Button variant="secondary" size="sm" onClick={onOpenCandidateBox} className="h-8 md:h-9" title="学习候选箱">
                <Plus size={16} />
                <span className="ml-2 hidden xl:inline">候选箱</span>
              </Button>
            )}

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="secondary" size="sm" className="h-8 md:h-9 gap-2" title={t("articleReader.export") || "Export"}>
                  <FileDown size={16} />
                  <span className="hidden xl:inline">{t("articleReader.export") || "Export"}</span>
                  <ChevronDown size={14} className="opacity-60" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-[220px]">
                <DropdownMenuLabel>{t("articleReader.exportMarkdown") || "Markdown"}</DropdownMenuLabel>
                <DropdownMenuItem onClick={() => onArticleExport("md", false)}>
                  {t("articleReader.exportOriginalTranslationMd") || "Original + Translation (MD)"}
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => onArticleExport("md", true)}>
                  {t("articleReader.exportAnnotatedMd") || "Original + Translation + Notes (MD)"}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>{t("articleReader.exportDocx") || "DOCX"}</DropdownMenuLabel>
                <DropdownMenuItem onClick={() => onArticleExport("docx", false)}>
                  {t("articleReader.exportOriginalTranslationDocx") || "Original + Translation (DOCX)"}
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => onArticleExport("docx", true)}>
                  {t("articleReader.exportAnnotatedDocx") || "Original + Translation + Notes (DOCX)"}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        ) : (
          !article.media_path ? (
            <Button variant="secondary" size="sm" onClick={onResegment} disabled={isResegmenting} className="h-8 md:h-9">
              {isResegmenting ? <Loader2 size={16} className="animate-spin" /> : <Split size={16} />}
              <span className="ml-2 hidden xl:inline">{t("articleReader.segment")}</span>
            </Button>
          ) : null
        )}

        <div className="w-px h-4 bg-border mx-1" />

        {!article.media_path && !article.book_path && !article.book_type && (
          <Button
            variant="secondary"
            size="sm"
            onClick={onToggleEditing}
            className="h-8 gap-1.5 px-2.5 md:h-9"
            title={editDocumentLabel}
            aria-label={editDocumentLabel}
          >
            <FileText size={16} />
            <span>{editDocumentLabel}</span>
          </Button>
        )}

        <Button
          size="sm"
          onClick={onTranslate}
          disabled={isTranslating || !canUseAi}
          className="gap-2 h-8 md:h-9 relative overflow-hidden"
          title={canUseAi ? t("articleReader.translate") : aiUnavailableMessage}
          variant="secondary"
        >
          {isTranslating && translationProgress && translationProgress.total > 0 && (
            <div
              className="absolute inset-0 bg-primary/10 transition-all duration-300"
              style={{ width: `${Math.min(100, (translationProgress.current / translationProgress.total) * 100)}%` }}
            />
          )}
          {isTranslating ? (
            translationProgress && translationProgress.total > 0 ? (
              <span className="text-xs font-mono z-10 text-primary">
                {Math.round((translationProgress.current / translationProgress.total) * 100)}%
              </span>
            ) : (
              <Loader2 size={16} className="animate-spin" />
            )
          ) : (
            <Languages size={16} />
          )}
          <span className="hidden xl:inline z-10">{t("articleReader.translate")}</span>
        </Button>

        <div className="w-px h-4 bg-border mx-1" />

        <Button
          variant="ghost"
          size="sm"
          onClick={onToggleAssistant}
          title={showAssistant ? "Hide Assistant" : "Show Assistant"}
          aria-label={showAssistant ? "Hide Assistant" : "Show Assistant"}
          className="h-8 w-8 p-0"
        >
          {showAssistant ? <PanelRightClose size={18} /> : <PanelRightOpen size={18} />}
        </Button>
      </div>
    </header>
  );
}

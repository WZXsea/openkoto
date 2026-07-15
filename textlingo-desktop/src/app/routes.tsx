import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { ArticleReader } from "../components/features/ArticleReader";
import { AnnotationWorkbench } from "../components/features/AnnotationWorkbench";
import { AssistantTaskCenter } from "../components/features/AssistantTaskCenter";
import { BookReader } from "../components/features/BookReader";
import { FavoritesPage } from "../components/features/FavoritesPage";
import { HomePage } from "../components/features/HomePage";
import { KtvExportPage } from "../components/features/KtvExportPage";
import { LearningWorkbench } from "../components/features/LearningWorkbench";
import { MaterialsWorkbenchPage } from "../components/features/MaterialsWorkbenchPage";
import { createAnnotationsApi } from "../features/annotations";
import type { AssistantSourceReference } from "../features/assistant";
import type { Article } from "../lib/tauri";
import { createMaterialsApi } from "../features/materials/api";
import { materialEditorApi } from "../features/editor";
import type { MaterialArticle, MaterialFilters } from "../features/materials/types";
import {
  toSourceLocator,
  type AnnotationResolution,
  type ReaderAnnotationDraft,
  type ReadingProgressChangeHandler,
  type ReadingProgressLocator,
  type ReadingProgressUpdate,
  type ReaderKind,
} from "../features/reader";
import type { Annotation } from "../types";
import type { AppScreen, MaterialViewMode } from "./navigation";

interface AppRoutesProps {
  activeScreen: AppScreen;
  activeAnnotation: Annotation | null;
  articles: Article[];
  canUseKtvExport: boolean;
  isLoading: boolean;
  focusedLearningItemId: string | null;
  selectedArticle: Article | null;
  selectedIndex: number;
  viewMode: MaterialViewMode;
  materialFilters: MaterialFilters;
  materialsScrollTop: number;
  onArticleUpdate: () => Promise<void>;
  onBackFromFavorites: () => void;
  onBackToList: () => void;
  onBackToReader: () => void;
  onDeleteArticle: (id: string) => Promise<void>;
  onEditArticle: (article: Article) => void;
  onNewMaterial: () => void;
  onOpenFavorites: () => void;
  onOpenLearning: () => void;
  onOpenMaterials: () => void;
  onNextArticle: () => void;
  onNavigateAnnotationSource: (annotation: Annotation) => void;
  onNavigateAssistantSource: (reference: AssistantSourceReference) => void;
  onOpenKtvExport: () => void;
  onPreviousArticle: () => void;
  onRefresh: () => Promise<Article[]>;
  onSelectArticle: (article: Article) => void;
  onMaterialFiltersChange: (filters: MaterialFilters) => void;
  onMaterialsScrollTopChange: (value: number) => void;
  onViewModeChange: (mode: MaterialViewMode) => void;
}

const READER_KINDS = new Set<ReaderKind>(["article", "pdf", "epub", "txt", "media"]);

function getInitialProgress(article: Article): ReadingProgressUpdate | undefined {
  const progress = (article as MaterialArticle).reading_progress;
  if (!progress || typeof progress !== "object" || Array.isArray(progress)) return undefined;
  const candidate = progress as {
    reader_kind?: unknown;
    locator?: unknown;
    progress_ratio?: unknown;
    status?: unknown;
  };
  if (!READER_KINDS.has(candidate.reader_kind as ReaderKind)
    || !candidate.locator || typeof candidate.locator !== "object"
    || typeof candidate.progress_ratio !== "number") return undefined;
  return {
    reader_kind: candidate.reader_kind as ReaderKind,
    locator: candidate.locator as ReadingProgressLocator,
    progress_ratio: candidate.progress_ratio,
    status: candidate.status === "completed" ? "completed" : "reading",
  };
}

export function AppRoutes({
  activeScreen,
  activeAnnotation,
  articles,
  canUseKtvExport,
  isLoading,
  focusedLearningItemId,
  selectedArticle,
  selectedIndex,
  viewMode,
  materialFilters,
  materialsScrollTop,
  onArticleUpdate,
  onBackFromFavorites,
  onBackToList,
  onBackToReader,
  onDeleteArticle,
  onEditArticle,
  onNewMaterial,
  onOpenFavorites,
  onOpenLearning,
  onOpenMaterials,
  onNextArticle,
  onNavigateAnnotationSource,
  onNavigateAssistantSource,
  onOpenKtvExport,
  onPreviousArticle,
  onRefresh,
  onSelectArticle,
  onMaterialFiltersChange,
  onMaterialsScrollTopChange,
  onViewModeChange,
}: AppRoutesProps) {
  const materialsApi = useMemo(() => createMaterialsApi(), []);
  const annotationsApi = useMemo(() => createAnnotationsApi(), []);
  const [progressError, setProgressError] = useState<string | null>(null);
  const [initialProgress, setInitialProgress] = useState<ReadingProgressUpdate | undefined>(undefined);
  const progressSaveQueue = useRef<Promise<void>>(Promise.resolve());
  const [isInitialProgressLoading, setIsInitialProgressLoading] = useState(false);
  const [readerAnnotation, setReaderAnnotation] = useState<Annotation | null>(activeAnnotation);
  const [annotationMessage, setAnnotationMessage] = useState<string | null>(null);
  useEffect(() => {
    setReaderAnnotation(activeAnnotation);
    setAnnotationMessage(null);
  }, [activeAnnotation, selectedArticle?.id]);

  useEffect(() => {
    if (!selectedArticle || activeAnnotation) return;
    let cancelled = false;
    void Promise.resolve()
      .then(() => annotationsApi.list({ material_id: selectedArticle.id, limit: 1, offset: 0 }))
      .then((items) => {
        if (!cancelled) setReaderAnnotation(items[0] ?? null);
      })
      .catch(() => {
        if (!cancelled) setReaderAnnotation(null);
      });
    return () => { cancelled = true; };
  }, [activeAnnotation, annotationsApi, selectedArticle]);

  useEffect(() => {
    if (!selectedArticle) {
      setInitialProgress(undefined);
      setIsInitialProgressLoading(false);
      return;
    }
    let cancelled = false;
    setIsInitialProgressLoading(true);
    setProgressError(null);
    void materialsApi.getReadingProgress(selectedArticle.id)
      .then((progress) => {
        if (!cancelled) setInitialProgress(progress ?? getInitialProgress(selectedArticle));
      })
      .catch((error) => {
        if (!cancelled) {
          setInitialProgress(getInitialProgress(selectedArticle));
          setProgressError(error instanceof Error ? error.message : "无法读取阅读进度");
        }
      })
      .finally(() => {
        if (!cancelled) setIsInitialProgressLoading(false);
      });
    return () => { cancelled = true; };
  }, [materialsApi, selectedArticle]);

  const handleReadingProgress = useCallback<ReadingProgressChangeHandler>((update) => {
    if (!selectedArticle) return;
    const materialId = selectedArticle.id;
    progressSaveQueue.current = progressSaveQueue.current
      .catch(() => undefined)
      .then(async () => {
        try {
          await materialsApi.upsertReadingProgress(materialId, update);
          setProgressError(null);
        } catch (error) {
          setProgressError(error instanceof Error ? error.message : "阅读进度未保存");
        }
      });
  }, [materialsApi, selectedArticle]);

  const handleAnnotationResolved = useCallback((resolution: AnnotationResolution) => {
    setAnnotationMessage(resolution.message);
  }, []);

  const handleAnnotationDraftCreated = useCallback(async (draft: ReaderAnnotationDraft) => {
    if (!selectedArticle) return;
    const locator = toSourceLocator(draft.locator);
    const segmentId = "segment_id" in locator ? locator.segment_id ?? null : null;
    try {
      const created = await annotationsApi.create({
        material_id: selectedArticle.id,
        segment_id: segmentId,
        kind: "highlight",
        locator,
        source_text: draft.source_text,
        material_revision: draft.material_revision ?? null,
        content_sha256: draft.content_sha256 ?? null,
        color: "#facc15",
        tags: [],
        client_request_id: crypto.randomUUID(),
      });
      setReaderAnnotation(created);
      setAnnotationMessage("高亮已保存");
    } catch (error) {
      setAnnotationMessage(`高亮保存失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }, [annotationsApi, selectedArticle]);

  const handleOpenEditableDerivative = useCallback(async () => {
    if (!selectedArticle) return;
    const result = await materialEditorApi.createEditableDerivative(selectedArticle.id);
    const derivative = await invoke<Article>("get_article", { id: result.derivative_material_id });
    onSelectArticle(derivative);
  }, [onSelectArticle, selectedArticle]);

  if (selectedArticle) {
    if (activeScreen === "ktv-export" && canUseKtvExport) {
      return <KtvExportPage article={selectedArticle} onBack={onBackToReader} />;
    }

    if (isInitialProgressLoading) {
      return <div className="flex h-full items-center justify-center text-sm text-muted-foreground" role="status">正在恢复阅读位置</div>;
    }

    if (selectedArticle.book_path) {
      return (
        <>
          {progressError && <p className="px-4 pt-3 text-sm text-destructive" role="alert">阅读进度未保存：{progressError}</p>}
          {annotationMessage && <p className="px-4 pt-2 text-xs text-muted-foreground" role="status">{annotationMessage}</p>}
          <BookReader key={selectedArticle.id} article={selectedArticle} onBack={onBackToList} onUpdate={onArticleUpdate} initialProgress={initialProgress} onProgressChange={handleReadingProgress} annotation={readerAnnotation} onAnnotationResolved={handleAnnotationResolved} onAnnotationDraftCreated={handleAnnotationDraftCreated} onOpenEditableDerivative={handleOpenEditableDerivative} />
        </>
      );
    }

    return (
      <>
        {progressError && <p className="px-4 pt-3 text-sm text-destructive" role="alert">阅读进度未保存：{progressError}</p>}
        {annotationMessage && <p className="px-4 pt-2 text-xs text-muted-foreground" role="status">{annotationMessage}</p>}
        <ArticleReader key={selectedArticle.id} article={selectedArticle} onBack={onBackToList} onNext={onNextArticle} onPrev={onPreviousArticle} hasNext={selectedIndex < articles.length - 1} hasPrev={selectedIndex > 0} onUpdate={onArticleUpdate} onOpenKtvExport={canUseKtvExport ? onOpenKtvExport : undefined} initialProgress={initialProgress} onProgressChange={handleReadingProgress} annotation={readerAnnotation} onAnnotationResolved={handleAnnotationResolved} onAnnotationDraftCreated={handleAnnotationDraftCreated} materialRevision={selectedArticle.current_revision !== undefined ? String(selectedArticle.current_revision) : selectedArticle.material_revision} contentSha256={selectedArticle.content_sha256} onNavigateAssistantSource={onNavigateAssistantSource} />
      </>
    );
  }

  if (activeScreen === "favorites") {
    return (
      <div className="h-full overflow-y-auto">
        <div className="mx-auto flex min-h-full max-w-[1500px] flex-col px-4 py-5 sm:px-6">
          <div className="mb-4 flex gap-1 border-b border-border">
            <button type="button" className="px-4 py-2.5 text-sm text-muted-foreground" onClick={onBackFromFavorites}>学习整理</button>
            <button type="button" className="border-b-2 border-primary px-4 py-2.5 text-sm font-medium">词包与已收录</button>
          </div>
          <FavoritesPage onBack={onBackFromFavorites} onSelectArticle={onSelectArticle} />
        </div>
      </div>
    );
  }

  if (activeScreen === "annotations") {
    return (
      <div className="h-full w-full max-w-7xl mx-auto p-4 sm:p-6">
        <h2 className="mb-4 text-xl font-semibold">批注与摘录</h2>
        <AnnotationWorkbench
          className="h-[calc(100%-3rem)]"
          materials={articles.map(({ id, title }) => ({ id, title }))}
          annotationsApi={annotationsApi}
          onNavigateToSource={onNavigateAnnotationSource}
        />
      </div>
    );
  }

  if (activeScreen === "assistant") {
    return (
      <div className="h-full min-h-0 w-full p-3 sm:p-5">
        <AssistantTaskCenter articles={articles} onNavigateSource={onNavigateAssistantSource} />
      </div>
    );
  }

  if (activeScreen === "learning") {
    return (
      <div className="h-full w-full max-w-[1600px] mx-auto p-4 sm:p-6">
        <div className="mb-4 flex gap-1 border-b border-border">
          <button type="button" className="border-b-2 border-primary px-4 py-2.5 text-sm font-medium">学习整理</button>
          <button type="button" className="px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground" onClick={onOpenFavorites}>词包与已收录</button>
        </div>
        <LearningWorkbench
          className="h-[calc(100%-3.5rem)]"
          initialItemId={focusedLearningItemId ?? undefined}
          materials={articles.map(({ id, title, source_type }) => ({
            id,
            title,
            sourceType: source_type,
          }))}
          onNavigateToSource={(item) => {
            if (!item.material_id) return;
            const article = articles.find((candidate) => candidate.id === item.material_id);
            if (article) onSelectArticle(article);
          }}
        />
      </div>
    );
  }

  if (activeScreen === "materials") {
    return <MaterialsWorkbenchPage articles={articles} isLoading={isLoading} filters={materialFilters} viewMode={viewMode} initialScrollTop={materialsScrollTop} onScrollTopChange={onMaterialsScrollTopChange} onFiltersChange={onMaterialFiltersChange} onViewModeChange={onViewModeChange} onSelectArticle={onSelectArticle} onDeleteArticle={onDeleteArticle} onEditArticle={onEditArticle} onNewMaterial={onNewMaterial} onArticleUpdate={onArticleUpdate} onRefresh={onRefresh} />;
  }

  return <HomePage articles={articles} onSelectArticle={onSelectArticle} onNewMaterial={onNewMaterial} onOpenMaterials={onOpenMaterials} onOpenLearning={onOpenLearning} />;
}

import { LayoutGrid, List, RotateCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { ArticleList } from "../components/features/ArticleList";
import { ArticleReader } from "../components/features/ArticleReader";
import { BookReader } from "../components/features/BookReader";
import { FavoritesPage } from "../components/features/FavoritesPage";
import { KtvExportPage } from "../components/features/KtvExportPage";
import { Button } from "../components/ui/button";
import type { Article } from "../lib/tauri";
import { createMaterialsApi } from "../features/materials/api";
import { DuplicateResolutionDialog } from "../features/materials/DuplicateResolutionDialog";
import { MaterialImportJobsPanel } from "../features/materials/MaterialImportJobsPanel";
import { MaterialTagsPanel } from "../features/materials/MaterialTagsPanel";
import type { MaterialImportJob, MaterialImportJobsApi, MaterialTagsApi, ManagedMaterialTag } from "../features/materials/materialManagement";
import { DEFAULT_MATERIAL_FILTERS, type MaterialArticle, type MaterialFilters } from "../features/materials/types";
import type { ReadingProgressChangeHandler, ReadingProgressLocator, ReadingProgressUpdate, ReaderKind } from "../features/reader";
import { getAppNavigationItem, type AppScreen, type MaterialViewMode } from "./navigation";

interface AppRoutesProps {
  activeScreen: AppScreen;
  articles: Article[];
  canUseKtvExport: boolean;
  isLoading: boolean;
  selectedArticle: Article | null;
  selectedIndex: number;
  viewMode: MaterialViewMode;
  onArticleUpdate: () => Promise<void>;
  onBackFromFavorites: () => void;
  onBackToList: () => void;
  onBackToReader: () => void;
  onDeleteArticle: (id: string) => Promise<void>;
  onEditArticle: (article: Article) => void;
  onNewMaterial: () => void;
  onNextArticle: () => void;
  onOpenKtvExport: () => void;
  onPreviousArticle: () => void;
  onRefresh: () => Promise<Article[]>;
  onSelectArticle: (article: Article) => void;
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

function resolveDuplicateTitles(jobs: MaterialImportJob[], articles: Article[]): MaterialImportJob[] {
  const titles = new Map(articles.map((article) => [article.id, article.title]));
  return jobs.map((job) => ({
    ...job,
    preview: job.preview ? {
      ...job.preview,
      duplicateMatches: job.preview.duplicateMatches?.map((match) => ({
        ...match,
        title: titles.get(match.materialId) || match.title,
      })),
    } : job.preview,
  }));
}

export function AppRoutes({
  activeScreen,
  articles,
  canUseKtvExport,
  isLoading,
  selectedArticle,
  selectedIndex,
  viewMode,
  onArticleUpdate,
  onBackFromFavorites,
  onBackToList,
  onBackToReader,
  onDeleteArticle,
  onEditArticle,
  onNewMaterial,
  onNextArticle,
  onOpenKtvExport,
  onPreviousArticle,
  onRefresh,
  onSelectArticle,
  onViewModeChange,
}: AppRoutesProps) {
  const { t } = useTranslation();
  const homeNavItem = getAppNavigationItem("home");
  const materialsApi = useMemo(() => createMaterialsApi(), []);
  const [selectedMaterialIds, setSelectedMaterialIds] = useState<string[]>([]);
  const [materialFilters, setMaterialFilters] = useState<MaterialFilters>(DEFAULT_MATERIAL_FILTERS);
  const [tags, setTags] = useState<ManagedMaterialTag[]>([]);
  const [jobs, setJobs] = useState<MaterialImportJob[]>([]);
  const [isWorkbenchLoading, setIsWorkbenchLoading] = useState(true);
  const [workbenchError, setWorkbenchError] = useState<string | null>(null);
  const [progressError, setProgressError] = useState<string | null>(null);
  const [initialProgress, setInitialProgress] = useState<ReadingProgressUpdate | undefined>(undefined);
  const progressSaveQueue = useRef<Promise<void>>(Promise.resolve());
  const [isInitialProgressLoading, setIsInitialProgressLoading] = useState(false);
  const [duplicateJob, setDuplicateJob] = useState<MaterialImportJob | null>(null);
  const titledJobs = useMemo(() => resolveDuplicateTitles(jobs, articles), [articles, jobs]);

  const refreshWorkbench = useCallback(async (): Promise<void> => {
    setIsWorkbenchLoading(true);
    setWorkbenchError(null);
    try {
      const [freshTags, freshJobs] = await Promise.all([
        materialsApi.listTags(),
        materialsApi.listImportJobs(),
      ]);
      setTags(freshTags);
      setJobs(freshJobs);
    } catch (error) {
      setWorkbenchError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsWorkbenchLoading(false);
    }
  }, [materialsApi]);

  const refreshAllMaterials = useCallback(async (): Promise<Article[]> => {
    const freshArticles = await onRefresh();
    await refreshWorkbench();
    return freshArticles;
  }, [onRefresh, refreshWorkbench]);

  useEffect(() => {
    void refreshWorkbench();
  }, [refreshWorkbench]);

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

  const refreshTags = useCallback(async () => {
    await refreshAllMaterials();
  }, [refreshAllMaterials]);

  const tagsApi = useMemo<MaterialTagsApi>(() => ({
    createTag: async (input) => { await materialsApi.tags.createTag(input); await refreshTags(); },
    renameTag: async (input) => { await materialsApi.tags.renameTag(input); await refreshTags(); },
    deleteTag: async (tagId) => { await materialsApi.tags.deleteTag(tagId); await refreshTags(); },
    mergeTags: async (input) => { await materialsApi.tags.mergeTags(input); await refreshTags(); },
    applyTags: async (input) => { await materialsApi.tags.applyTags(input); await refreshTags(); },
  }), [materialsApi, refreshTags]);

  const jobsApi = useMemo<MaterialImportJobsApi>(() => ({
    retryJob: async (jobId) => { await materialsApi.jobs.retryJob(jobId); await refreshWorkbench(); },
    cancelJob: async (jobId) => { await materialsApi.jobs.cancelJob(jobId); await refreshWorkbench(); },
    resolveJob: async (jobId, action) => materialsApi.jobs.resolveJob!(jobId, action),
  }), [materialsApi, refreshWorkbench]);

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

  const handleDuplicateResolution = useCallback(async (action: "cancel" | "open_existing" | "replace" | "keep_copy") => {
    const job = duplicateJob;
    setDuplicateJob(null);
    if (!job) return;
    const duplicate = job.preview?.duplicateMatches?.[0];
    if (action === "cancel") {
      await jobsApi.cancelJob(job.id);
      return;
    }
    if (!jobsApi.resolveJob) {
      setWorkbenchError("当前版本无法恢复该导入任务。");
      return;
    }
    try {
      const resolved = await jobsApi.resolveJob(job.id, action);
      const freshArticles = await refreshAllMaterials();
      const article = freshArticles.find((item) => item.id === resolved.id)
        ?? (duplicate ? freshArticles.find((item) => item.id === duplicate.materialId) : undefined);
      if (article) onSelectArticle(article);
    } catch (error) {
      setWorkbenchError(error instanceof Error ? error.message : String(error));
    }
  }, [duplicateJob, jobsApi, onSelectArticle, refreshAllMaterials]);

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
          <BookReader key={selectedArticle.id} article={selectedArticle} onBack={onBackToList} onUpdate={onArticleUpdate} initialProgress={initialProgress} onProgressChange={handleReadingProgress} />
        </>
      );
    }

    return (
      <>
        {progressError && <p className="px-4 pt-3 text-sm text-destructive" role="alert">阅读进度未保存：{progressError}</p>}
        <ArticleReader key={selectedArticle.id} article={selectedArticle} onBack={onBackToList} onNext={onNextArticle} onPrev={onPreviousArticle} hasNext={selectedIndex < articles.length - 1} hasPrev={selectedIndex > 0} onUpdate={onArticleUpdate} onOpenKtvExport={canUseKtvExport ? onOpenKtvExport : undefined} initialProgress={initialProgress} onProgressChange={handleReadingProgress} />
      </>
    );
  }

  if (activeScreen === "favorites") {
    return (
      <FavoritesPage
        onBack={onBackFromFavorites}
        onSelectArticle={onSelectArticle}
      />
    );
  }

  return (
    <div className="h-full w-full max-w-7xl mx-auto p-4 sm:p-6 overflow-y-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-xl font-semibold">
          {t(homeNavItem.labelKey, homeNavItem.fallbackLabel).replace("我的文章", "我的素材")}
        </h2>
        <div className="flex items-center gap-2 bg-muted/50 p-1 rounded-lg border border-border">
          <Button
            variant={viewMode === "list" ? "secondary" : "ghost"}
            size="sm"
            onClick={() => onViewModeChange("list")}
            className="h-7 px-2"
            title={t("articleList.listView")}
          >
            <List size={14} />
          </Button>
          <Button
            variant={viewMode === "card" ? "secondary" : "ghost"}
            size="sm"
            onClick={() => onViewModeChange("card")}
            className="h-7 px-2"
            title={t("articleList.cardView")}
          >
            <LayoutGrid size={14} />
          </Button>
        </div>
        <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void refreshAllMaterials()}
            disabled={isLoading}
            title={t("common.refresh")}
          >
            <RotateCw size={16} className={isLoading ? "animate-spin" : ""} />
          </Button>
        </div>
      </div>
      <ArticleList
        articles={articles}
        isLoading={isLoading}
        onSelectArticle={onSelectArticle}
        onDelete={onDeleteArticle}
        onBulkArchive={async (ids) => { await materialsApi.archive(ids); await refreshAllMaterials(); }}
        onBulkDelete={async (ids) => { await materialsApi.remove(ids); await refreshAllMaterials(); }}
        onSelectionChange={setSelectedMaterialIds}
        onEdit={onEditArticle}
        onNewMaterial={onNewMaterial}
        onUpdate={onArticleUpdate}
        selectedId={undefined}
        viewMode={viewMode}
        filters={materialFilters}
        onFiltersChange={setMaterialFilters}
      />
      <div className="mt-8 grid gap-8 xl:grid-cols-2">
        <MaterialTagsPanel tags={tags} selectedMaterialIds={selectedMaterialIds} api={tagsApi} isLoading={isWorkbenchLoading} error={workbenchError} onRetry={() => void refreshWorkbench()} />
        <MaterialImportJobsPanel jobs={titledJobs} api={jobsApi} isLoading={isWorkbenchLoading} error={workbenchError} onRetry={() => void refreshWorkbench()} onResolveDuplicate={setDuplicateJob} />
      </div>
      <DuplicateResolutionDialog isOpen={Boolean(duplicateJob)} duplicate={duplicateJob?.preview?.duplicateMatches?.[0] ?? null} onResolve={(action) => void handleDuplicateResolution(action)} />
    </div>
  );
}

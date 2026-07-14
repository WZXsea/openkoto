import { LayoutGrid, List, Plus, RotateCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { createMaterialsApi } from "../../features/materials/api";
import { DuplicateResolutionDialog } from "../../features/materials/DuplicateResolutionDialog";
import { MaterialImportJobsPanel } from "../../features/materials/MaterialImportJobsPanel";
import { MaterialTagsPanel } from "../../features/materials/MaterialTagsPanel";
import type {
  MaterialImportJob,
  MaterialImportJobsApi,
  MaterialTagsApi,
  ManagedMaterialTag,
} from "../../features/materials/materialManagement";
import type { MaterialFilters } from "../../features/materials/types";
import type { Article } from "../../types";
import type { MaterialViewMode } from "../../app/navigation";
import { Button } from "../ui/button";
import { ArticleList } from "./ArticleList";

type WorkbenchTab = "materials" | "tags" | "imports";

interface MaterialsWorkbenchPageProps {
  articles: Article[];
  isLoading: boolean;
  filters: MaterialFilters;
  viewMode: MaterialViewMode;
  initialScrollTop: number;
  onScrollTopChange: (value: number) => void;
  onFiltersChange: (filters: MaterialFilters) => void;
  onViewModeChange: (mode: MaterialViewMode) => void;
  onSelectArticle: (article: Article) => void;
  onDeleteArticle: (id: string) => Promise<void>;
  onEditArticle: (article: Article) => void;
  onNewMaterial: () => void;
  onArticleUpdate: () => Promise<void>;
  onRefresh: () => Promise<Article[]>;
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

export function MaterialsWorkbenchPage({
  articles,
  isLoading,
  filters,
  viewMode,
  initialScrollTop,
  onScrollTopChange,
  onFiltersChange,
  onViewModeChange,
  onSelectArticle,
  onDeleteArticle,
  onEditArticle,
  onNewMaterial,
  onArticleUpdate,
  onRefresh,
}: MaterialsWorkbenchPageProps) {
  const materialsApi = useMemo(() => createMaterialsApi(), []);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [activeTab, setActiveTab] = useState<WorkbenchTab>("materials");
  const [selectedMaterialIds, setSelectedMaterialIds] = useState<string[]>([]);
  const [tags, setTags] = useState<ManagedMaterialTag[]>([]);
  const [jobs, setJobs] = useState<MaterialImportJob[]>([]);
  const [isWorkbenchLoading, setIsWorkbenchLoading] = useState(true);
  const [workbenchError, setWorkbenchError] = useState<string | null>(null);
  const [duplicateJob, setDuplicateJob] = useState<MaterialImportJob | null>(null);
  const titledJobs = useMemo(() => resolveDuplicateTitles(jobs, articles), [articles, jobs]);

  const refreshWorkbench = useCallback(async () => {
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

  const refreshAll = useCallback(async () => {
    const freshArticles = await onRefresh();
    await refreshWorkbench();
    return freshArticles;
  }, [onRefresh, refreshWorkbench]);

  useEffect(() => {
    void refreshWorkbench();
  }, [refreshWorkbench]);

  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      if (scrollContainerRef.current) scrollContainerRef.current.scrollTop = initialScrollTop;
    });
    return () => cancelAnimationFrame(frame);
  }, [initialScrollTop]);

  const tagsApi = useMemo<MaterialTagsApi>(() => ({
    createTag: async (input) => { await materialsApi.tags.createTag(input); await refreshAll(); },
    renameTag: async (input) => { await materialsApi.tags.renameTag(input); await refreshAll(); },
    deleteTag: async (tagId) => { await materialsApi.tags.deleteTag(tagId); await refreshAll(); },
    mergeTags: async (input) => { await materialsApi.tags.mergeTags(input); await refreshAll(); },
    applyTags: async (input) => { await materialsApi.tags.applyTags(input); await refreshAll(); },
  }), [materialsApi, refreshAll]);

  const jobsApi = useMemo<MaterialImportJobsApi>(() => ({
    retryJob: async (jobId) => { await materialsApi.jobs.retryJob(jobId); await refreshWorkbench(); },
    cancelJob: async (jobId) => { await materialsApi.jobs.cancelJob(jobId); await refreshWorkbench(); },
    resolveJob: async (jobId, action) => materialsApi.jobs.resolveJob!(jobId, action),
  }), [materialsApi, refreshWorkbench]);

  const handleDuplicateResolution = useCallback(async (action: "cancel" | "open_existing" | "replace" | "keep_copy") => {
    const job = duplicateJob;
    setDuplicateJob(null);
    if (!job) return;
    if (action === "cancel") {
      await jobsApi.cancelJob(job.id);
      return;
    }
    if (!jobsApi.resolveJob) {
      setWorkbenchError("当前版本无法恢复该导入任务。");
      return;
    }
    try {
      const duplicate = job.preview?.duplicateMatches?.[0];
      const resolved = await jobsApi.resolveJob(job.id, action);
      const freshArticles = await refreshAll();
      const article = freshArticles.find((item) => item.id === resolved.id)
        ?? (duplicate ? freshArticles.find((item) => item.id === duplicate.materialId) : undefined);
      if (article) onSelectArticle(article);
    } catch (error) {
      setWorkbenchError(error instanceof Error ? error.message : String(error));
    }
  }, [duplicateJob, jobsApi, onSelectArticle, refreshAll]);

  const tabs: Array<{ id: WorkbenchTab; label: string; count?: number }> = [
    { id: "materials", label: "素材", count: articles.length },
    { id: "tags", label: "标签", count: tags.length },
    { id: "imports", label: "导入任务", count: jobs.length },
  ];

  return (
    <div
      ref={scrollContainerRef}
      className="h-full overflow-y-auto"
      onScroll={(event) => onScrollTopChange(event.currentTarget.scrollTop)}
    >
      <div className="mx-auto max-w-[1500px] px-5 py-7 sm:px-8 lg:px-10">
        <header className="mb-6 flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="mb-1 text-sm text-muted-foreground">阅读素材管理</p>
            <h1 className="text-2xl font-semibold tracking-tight">素材库</h1>
          </div>
          <div className="flex items-center gap-2">
            {activeTab === "materials" && (
              <div className="flex items-center rounded-lg border border-border bg-muted/35 p-1">
                <Button variant={viewMode === "list" ? "secondary" : "ghost"} size="sm" onClick={() => onViewModeChange("list")} className="h-7 px-2" title="列表视图"><List size={14} /></Button>
                <Button variant={viewMode === "card" ? "secondary" : "ghost"} size="sm" onClick={() => onViewModeChange("card")} className="h-7 px-2" title="卡片视图"><LayoutGrid size={14} /></Button>
              </div>
            )}
            <Button variant="ghost" size="sm" onClick={() => void refreshAll()} disabled={isLoading} title="刷新"><RotateCw size={16} className={isLoading ? "animate-spin" : ""} /></Button>
            <Button size="sm" onClick={onNewMaterial} className="gap-1.5"><Plus size={15} />导入素材</Button>
          </div>
        </header>

        <div className="mb-6 flex gap-1 border-b border-border" role="tablist" aria-label="素材库页签">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={activeTab === tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`relative px-4 py-2.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${activeTab === tab.id ? "text-foreground" : "text-muted-foreground hover:text-foreground"}`}
            >
              {tab.label}{typeof tab.count === "number" && <span className="ml-1.5 text-xs text-muted-foreground">{tab.count}</span>}
              {activeTab === tab.id && <span className="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-primary" />}
            </button>
          ))}
        </div>

        {activeTab === "materials" && (
          <ArticleList
            articles={articles}
            isLoading={isLoading}
            onSelectArticle={onSelectArticle}
            onDelete={onDeleteArticle}
            onBulkArchive={async (ids) => { await materialsApi.archive(ids); await refreshAll(); }}
            onBulkDelete={async (ids) => { await materialsApi.remove(ids); await refreshAll(); }}
            onSelectionChange={setSelectedMaterialIds}
            onEdit={onEditArticle}
            onNewMaterial={onNewMaterial}
            onUpdate={onArticleUpdate}
            viewMode={viewMode}
            filters={filters}
            onFiltersChange={onFiltersChange}
            showContinueReading={false}
          />
        )}
        {activeTab === "tags" && <MaterialTagsPanel tags={tags} selectedMaterialIds={selectedMaterialIds} api={tagsApi} isLoading={isWorkbenchLoading} error={workbenchError} onRetry={() => void refreshWorkbench()} />}
        {activeTab === "imports" && <MaterialImportJobsPanel jobs={titledJobs} api={jobsApi} isLoading={isWorkbenchLoading} error={workbenchError} onRetry={() => void refreshWorkbench()} onResolveDuplicate={setDuplicateJob} />}
      </div>
      <DuplicateResolutionDialog isOpen={Boolean(duplicateJob)} duplicate={duplicateJob?.preview?.duplicateMatches?.[0] ?? null} onResolve={(action) => void handleDuplicateResolution(action)} />
    </div>
  );
}

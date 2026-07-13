import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Archive,
  BookOpen,
  CalendarDays,
  ChevronRight,
  CircleAlert,
  Clock3,
  ExternalLink,
  FileText,
  FilterX,
  Globe2,
  Loader2,
  MoreHorizontal,
  Music2,
  Pencil,
  Search,
  Trash2,
  Video,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "../ui/button";
import { Dialog, DialogFooter } from "../ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../ui/dropdown-menu";
import { Input } from "../ui/input";
import { Select } from "../ui/select";
import { formatDate } from "../../lib/utils";
import type { Article } from "../../types";
import {
  filterAndSortMaterials,
  getContinueReadingMaterials,
  getMaterialOpenedAt,
  getMaterialProgress,
  getMaterialTagLabels,
  getMaterialTags,
  getMaterialType,
} from "../../features/materials/selectors";
import {
  DEFAULT_MATERIAL_FILTERS,
  MATERIAL_TYPES,
  type MaterialArticle,
  type MaterialFilters,
  type ReadingStatus,
} from "../../features/materials/types";

interface ArticleListProps {
  articles: Article[];
  isLoading: boolean;
  error?: string | null;
  onRetry?: () => void;
  onSelectArticle: (article: Article) => void;
  onDelete: (id: string) => Promise<void>;
  onBulkDelete?: (ids: string[]) => Promise<void>;
  onBulkArchive?: (ids: string[]) => Promise<void>;
  onSelectionChange?: (ids: string[]) => void;
  onEdit: (article: Article) => void;
  onNewMaterial?: () => void;
  selectedId?: string;
  viewMode: "list" | "card";
  filters?: MaterialFilters;
  onFiltersChange?: (filters: MaterialFilters) => void;
  /** Kept for the legacy route while per-material maintenance remains available. */
  onUpdate?: () => void;
}

const readingStatusLabels: Record<ReadingStatus, string> = {
  unread: "未开始",
  in_progress: "阅读中",
  completed: "已完成",
  archived: "已归档",
};

const typeLabels: Record<string, string> = {
  article: "文章",
  web: "网页",
  text: "文本",
  book: "书籍",
  video: "视频",
  audio: "音频",
};

function MaterialTypeIcon({ type, size = 16 }: { type: string; size?: number }) {
  const className = "shrink-0 text-muted-foreground";
  if (type === "web") return <Globe2 className={className} size={size} />;
  if (type === "book") return <BookOpen className={className} size={size} />;
  if (type === "video") return <Video className={className} size={size} />;
  if (type === "audio") return <Music2 className={className} size={size} />;
  return <FileText className={className} size={size} />;
}

function Progress({ article }: { article: MaterialArticle }) {
  const progress = getMaterialProgress(article);
  if (progress === null) return null;
  return (
    <div className="flex min-w-0 items-center gap-2" aria-label={`阅读进度 ${progress}%`}>
      <div className="h-1.5 w-20 overflow-hidden rounded bg-muted">
        <div className="h-full bg-primary" style={{ width: `${progress}%` }} />
      </div>
      <span className="text-xs tabular-nums text-muted-foreground">{progress}%</span>
    </div>
  );
}

export function ArticleList({
  articles,
  isLoading,
  error,
  onRetry,
  onSelectArticle,
  onDelete,
  onBulkDelete,
  onBulkArchive,
  onSelectionChange,
  onEdit,
  onNewMaterial,
  selectedId,
  viewMode,
  filters: controlledFilters,
  onFiltersChange,
  onUpdate,
}: ArticleListProps) {
  const { t } = useTranslation();
  const [localFilters, setLocalFilters] = useState<MaterialFilters>(DEFAULT_MATERIAL_FILTERS);
  const filters = controlledFilters ?? localFilters;
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [pendingAction, setPendingAction] = useState<"archive" | "delete" | null>(null);
  const [isApplyingAction, setIsApplyingAction] = useState(false);
  const [maintainingId, setMaintainingId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const visibleArticles = useMemo(() => filterAndSortMaterials(articles, filters), [articles, filters]);
  const continueReading = useMemo(() => getContinueReadingMaterials(articles), [articles]);
  const tags = useMemo(() => getMaterialTags(articles), [articles]);
  const hasFilters = filters.query !== "" || filters.type !== "all" || filters.readingStatus !== "all" || filters.tag !== "all" || filters.createdFrom !== "" || filters.createdTo !== "" || filters.sort !== "recent";
  const allVisibleSelected = visibleArticles.length > 0 && visibleArticles.every((article) => selectedIds.includes(article.id));

  useEffect(() => {
    const validIds = new Set(articles.map((article) => article.id));
    setSelectedIds((current) => current.filter((id) => validIds.has(id)));
  }, [articles]);

  useEffect(() => {
    onSelectionChange?.(selectedIds);
  }, [onSelectionChange, selectedIds]);

  const setFilters = (next: MaterialFilters) => {
    if (controlledFilters === undefined) setLocalFilters(next);
    onFiltersChange?.(next);
  };
  const updateFilters = (next: Partial<MaterialFilters>) => setFilters({ ...filters, ...next });
  const clearFilters = () => setFilters(DEFAULT_MATERIAL_FILTERS);
  const toggleSelected = (id: string) => setSelectedIds((current) => current.includes(id)
    ? current.filter((selectedId) => selectedId !== id)
    : [...current, id]);
  const toggleAllVisible = () => setSelectedIds((current) => allVisibleSelected
    ? current.filter((id) => !visibleArticles.some((article) => article.id === id))
    : [...new Set([...current, ...visibleArticles.map((article) => article.id)])]);

  const applyBulkAction = async () => {
    if (!pendingAction || selectedIds.length === 0) return;
    setIsApplyingAction(true);
    setActionError(null);
    try {
      if (pendingAction === "archive") {
        if (!onBulkArchive) throw new Error(t("materials.archiveUnavailable", "当前后端尚不支持归档"));
        await onBulkArchive(selectedIds);
      } else if (onBulkDelete) {
        await onBulkDelete(selectedIds);
      } else {
        await Promise.all(selectedIds.map(onDelete));
      }
      setSelectedIds([]);
      setPendingAction(null);
    } catch (caught) {
      setActionError(caught instanceof Error ? caught.message : "操作未完成");
    } finally {
      setIsApplyingAction(false);
    }
  };

  const runMaintenance = async (command: "delete_article_subtitles_cmd" | "delete_article_analysis_cmd", articleId: string) => {
    setMaintainingId(articleId);
    setActionError(null);
    try {
      await invoke(command, { id: articleId });
      onUpdate?.();
    } catch (caught) {
      setActionError(caught instanceof Error ? caught.message : "素材维护操作未完成");
    } finally {
      setMaintainingId(null);
    }
  };

  if (isLoading) {
    return (
      <div className="flex min-h-64 items-center justify-center text-sm text-muted-foreground" role="status">
        <Loader2 className="mr-2 animate-spin" size={18} /> {t("articleList.loading", "正在加载素材")}
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex min-h-64 flex-col items-center justify-center gap-3 text-center" role="alert">
        <CircleAlert className="text-destructive" size={24} />
        <p className="text-sm text-muted-foreground">{error}</p>
        {onRetry && <Button variant="outline" size="sm" onClick={onRetry}>{t("common.refresh", "重试")}</Button>}
      </div>
    );
  }

  if (articles.length === 0) {
    return (
      <div className="flex min-h-64 flex-col items-center justify-center gap-3 text-center">
        <BookOpen className="text-muted-foreground" size={28} />
        <p className="font-medium">{t("articleList.noArticles", "暂无素材")}</p>
        <p className="text-sm text-muted-foreground">{t("articleList.createFirst", "导入或新建第一份阅读素材")}</p>
        {onNewMaterial && <Button size="sm" onClick={onNewMaterial}>{t("header.newMaterial", "新建素材")}</Button>}
      </div>
    );
  }

  return (
    <section className="min-w-0 space-y-5 pb-8" aria-label={t("materials.workbench", "素材工作台")} data-view-mode={viewMode}>
      {continueReading.length > 0 && (
        <section aria-labelledby="continue-reading-title">
          <div className="mb-2 flex items-center justify-between gap-3">
            <h2 id="continue-reading-title" className="text-sm font-semibold">{t("materials.continueReading", "继续阅读")}</h2>
            <span className="text-xs text-muted-foreground">{continueReading.length} {t("materials.items", "项")}</span>
          </div>
          <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
            {continueReading.map((article) => (
              <button
                key={article.id}
                type="button"
                onClick={() => onSelectArticle(article)}
                className="flex min-w-0 items-center gap-3 rounded-lg border border-border bg-card px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <MaterialTypeIcon type={getMaterialType(article)} />
                <span className="min-w-0 flex-1">
                  <span className="block break-words text-sm font-medium">{article.title || t("articleList.untitled", "未命名素材")}</span>
                  <span className="mt-1 block"><Progress article={article} /></span>
                </span>
                <ChevronRight size={16} className="shrink-0 text-muted-foreground" />
              </button>
            ))}
          </div>
        </section>
      )}

      <section aria-labelledby="all-materials-title" className="min-w-0">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 id="all-materials-title" className="text-sm font-semibold">{t("materials.allMaterials", "全部素材")}</h2>
            <p className="mt-0.5 text-xs text-muted-foreground">{visibleArticles.length} {t("materials.items", "项")}</p>
          </div>
          {hasFilters && <Button variant="ghost" size="sm" onClick={clearFilters}><FilterX size={15} className="mr-1.5" />{t("materials.clearFilters", "清除筛选")}</Button>}
        </div>

        <div className="grid gap-2 border-y border-border py-3 md:grid-cols-[minmax(220px,1fr)_130px_130px_130px_130px]">
          <div className="relative min-w-0">
            <Search className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" size={16} />
            <Input aria-label={t("materials.search", "搜索素材")} value={filters.query} onChange={(event) => updateFilters({ query: event.target.value })} placeholder={t("materials.searchPlaceholder", "搜索标题、来源或标签")} className="pl-9" />
          </div>
          <Select aria-label={t("materials.type", "素材类型")} value={filters.type} onChange={(event) => updateFilters({ type: event.target.value as MaterialFilters["type"] })}>
            <option value="all">{t("materials.allTypes", "全部类型")}</option>
            {MATERIAL_TYPES.map((type) => <option key={type} value={type}>{typeLabels[type]}</option>)}
          </Select>
          <Select aria-label={t("materials.readingStatus", "阅读状态")} value={filters.readingStatus} onChange={(event) => updateFilters({ readingStatus: event.target.value as MaterialFilters["readingStatus"] })}>
            <option value="all">{t("materials.allStatuses", "全部状态")}</option>
            {(Object.keys(readingStatusLabels) as ReadingStatus[]).map((status) => <option key={status} value={status}>{readingStatusLabels[status]}</option>)}
          </Select>
          <Select aria-label={t("materials.tags", "标签")} value={filters.tag} onChange={(event) => updateFilters({ tag: event.target.value })}>
            <option value="all">{t("materials.allTags", "全部标签")}</option>
            {tags.map((tag) => <option key={tag} value={tag}>{tag}</option>)}
          </Select>
          <Select aria-label={t("materials.sort", "排序")} value={filters.sort} onChange={(event) => updateFilters({ sort: event.target.value as MaterialFilters["sort"] })}>
            <option value="recent">{t("materials.sortRecent", "最近打开")}</option>
            <option value="created">{t("materials.sortCreated", "最近导入")}</option>
            <option value="progress">{t("materials.sortProgress", "阅读进度")}</option>
            <option value="title">{t("materials.sortTitle", "标题")}</option>
          </Select>
        </div>
        <div className="flex flex-wrap items-center gap-2 border-b border-border py-3">
          <CalendarDays size={16} className="shrink-0 text-muted-foreground" />
          <label className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
            <span>导入日期从</span>
            <Input type="date" aria-label="导入日期从" value={filters.createdFrom} onChange={(event) => updateFilters({ createdFrom: event.target.value })} className="h-8 w-[150px]" />
          </label>
          <label className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
            <span>至</span>
            <Input type="date" aria-label="导入日期至" value={filters.createdTo} min={filters.createdFrom || undefined} onChange={(event) => updateFilters({ createdTo: event.target.value })} className="h-8 w-[150px]" />
          </label>
        </div>

        {selectedIds.length > 0 && (
          <div className="my-3 flex flex-wrap items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2">
            <span className="text-sm">{t("materials.selectedCount", { count: selectedIds.length, defaultValue: `已选择 ${selectedIds.length} 项` })}</span>
            <div className="ml-auto flex gap-1">
              <Button variant="ghost" size="sm" onClick={() => setPendingAction("archive")} disabled={!onBulkArchive} title={!onBulkArchive ? t("materials.archiveUnavailable", "当前后端尚不支持归档") : undefined}><Archive size={15} className="mr-1.5" />{t("materials.archive", "归档")}</Button>
              <Button variant="ghost" size="sm" className="text-destructive hover:text-destructive" onClick={() => setPendingAction("delete")}><Trash2 size={15} className="mr-1.5" />{t("articleList.delete", "删除")}</Button>
            </div>
          </div>
        )}

        {actionError && <p className="my-3 text-sm text-destructive" role="alert">{actionError}</p>}

        {visibleArticles.length === 0 ? (
          <div className="flex min-h-48 flex-col items-center justify-center gap-2 text-center">
            <Search className="text-muted-foreground" size={22} />
            <p className="text-sm font-medium">{t("materials.noResults", "没有匹配的素材")}</p>
            <Button variant="ghost" size="sm" onClick={clearFilters}>{t("materials.clearFilters", "清除筛选")}</Button>
          </div>
        ) : (
          viewMode === "list" ? (
            <div className="overflow-hidden rounded-lg border border-border" data-testid="material-list-layout">
              <div className="hidden grid-cols-[36px_minmax(220px,1fr)_minmax(120px,0.55fr)_minmax(150px,0.7fr)_132px_40px] items-center gap-3 border-b border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground lg:grid">
                <input aria-label={t("materials.selectAll", "选择当前结果")} type="checkbox" checked={allVisibleSelected} onChange={toggleAllVisible} />
                <span>{t("materials.material", "素材")}</span><span>{t("materials.source", "来源")}</span><span>{t("materials.tags", "标签")}</span><span>{t("materials.lastOpened", "最近打开")}</span><span />
              </div>
              <div className="divide-y divide-border">
                {visibleArticles.map((article) => {
                  const type = getMaterialType(article);
                  const isSelected = selectedIds.includes(article.id);
                  const lastOpened = getMaterialOpenedAt(article);
                  const tags = getMaterialTagLabels(article);
                  const isMedia = type === "video" || type === "audio";
                  return (
                    <article key={article.id} className={`grid min-w-0 grid-cols-[32px_minmax(0,1fr)_36px] items-start gap-2 px-3 py-3 lg:grid-cols-[36px_minmax(220px,1fr)_minmax(120px,0.55fr)_minmax(150px,0.7fr)_132px_40px] lg:items-center lg:gap-3 ${selectedId === article.id ? "bg-primary/5" : "hover:bg-muted/30"}`}>
                      <input aria-label={`${t("materials.select", "选择")} ${article.title}`} type="checkbox" checked={isSelected} onClick={(event) => event.stopPropagation()} onChange={() => toggleSelected(article.id)} />
                      <button type="button" onClick={() => onSelectArticle(article)} className="min-w-0 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
                        <span className="flex min-w-0 items-start gap-2"><MaterialTypeIcon type={type} /><span className="min-w-0"><span className="block break-words text-sm font-medium">{article.title || t("articleList.untitled", "未命名素材")}</span><span className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground"><span>{typeLabels[type]}</span>{article.import_status === "failed" && <span className="break-words text-destructive">{article.import_error || t("materials.importFailed", "导入失败")}</span>}{article.import_status === "importing" && <span>{t("materials.importing", "导入中")}</span>}<Progress article={article} /></span><span className="mt-1 block break-all text-xs text-muted-foreground lg:hidden">{article.source_name || article.source_url || "本地素材"}</span></span></span>
                      </button>
                      <span className="hidden break-all text-xs text-muted-foreground lg:block">{article.source_name || article.source_url || "本地素材"}</span>
                      <span className="hidden min-w-0 flex-wrap gap-1 lg:flex">{tags.slice(0, 2).map((tag) => <span key={tag} className="max-w-full break-all rounded border border-border px-1.5 py-0.5 text-xs text-muted-foreground">{tag}</span>)}</span>
                      <span className="hidden text-xs text-muted-foreground lg:block"><Clock3 className="mr-1 inline" size={12} />{formatDate(lastOpened)}</span>
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-8 w-8" aria-label={`${t("materials.actions", "操作")} ${article.title}`} onClick={(event) => event.stopPropagation()}><MoreHorizontal size={17} /></Button></DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem onClick={() => onSelectArticle(article)}><BookOpen className="mr-2" size={15} />{t("materials.open", "打开")}</DropdownMenuItem>
                          <DropdownMenuItem onClick={() => onEdit(article)}><Pencil className="mr-2" size={15} />{t("common.edit", "编辑")}</DropdownMenuItem>
                          {article.source_url && <DropdownMenuItem onClick={() => void openUrl(article.source_url!)}><ExternalLink className="mr-2" size={15} />{t("articleList.openSource", "打开来源")}</DropdownMenuItem>}
                          {isMedia && <><DropdownMenuItem disabled={maintainingId === article.id} onClick={() => void runMaintenance("delete_article_subtitles_cmd", article.id)}>{t("articleList.deleteSubtitles", "删除字幕")}</DropdownMenuItem><DropdownMenuItem disabled={maintainingId === article.id} onClick={() => void runMaintenance("delete_article_analysis_cmd", article.id)}>{t("articleList.deleteAnalysis", "删除翻译解析")}</DropdownMenuItem></>}
                          <DropdownMenuItem className="text-destructive focus:text-destructive" onClick={() => { setSelectedIds([article.id]); setPendingAction("delete"); }}><Trash2 className="mr-2" size={15} />{t("articleList.delete", "删除")}</DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </article>
                  );
                })}
              </div>
            </div>
          ) : (
            <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-3" data-testid="material-card-layout">
              {visibleArticles.map((article) => {
                const type = getMaterialType(article);
                const isSelected = selectedIds.includes(article.id);
                const tags = getMaterialTagLabels(article);
                const isMedia = type === "video" || type === "audio";
                return (
                  <article key={article.id} className={`grid min-w-0 grid-cols-[28px_minmax(0,1fr)_36px] items-start gap-2 rounded-lg border border-border p-3 ${selectedId === article.id ? "bg-primary/5" : "hover:bg-muted/30"}`}>
                    <input aria-label={`${t("materials.select", "选择")} ${article.title}`} type="checkbox" checked={isSelected} onClick={(event) => event.stopPropagation()} onChange={() => toggleSelected(article.id)} />
                    <button type="button" onClick={() => onSelectArticle(article)} className="min-w-0 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
                      <span className="flex min-w-0 items-start gap-2"><MaterialTypeIcon type={type} /><span className="min-w-0"><span className="block break-words text-sm font-medium">{article.title || t("articleList.untitled", "未命名素材")}</span><span className="mt-1 block break-all text-xs text-muted-foreground">{article.source_name || article.source_url || "本地素材"}</span><span className="mt-2 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground"><span>{typeLabels[type]}</span><Progress article={article} /></span>{tags.length > 0 && <span className="mt-2 flex min-w-0 flex-wrap gap-1">{tags.slice(0, 3).map((tag) => <span key={tag} className="max-w-full break-all rounded border border-border px-1.5 py-0.5 text-xs text-muted-foreground">{tag}</span>)}</span>}</span></span>
                    </button>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-8 w-8" aria-label={`${t("materials.actions", "操作")} ${article.title}`} onClick={(event) => event.stopPropagation()}><MoreHorizontal size={17} /></Button></DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => onSelectArticle(article)}><BookOpen className="mr-2" size={15} />{t("materials.open", "打开")}</DropdownMenuItem>
                        <DropdownMenuItem onClick={() => onEdit(article)}><Pencil className="mr-2" size={15} />{t("common.edit", "编辑")}</DropdownMenuItem>
                        {article.source_url && <DropdownMenuItem onClick={() => void openUrl(article.source_url!)}><ExternalLink className="mr-2" size={15} />{t("articleList.openSource", "打开来源")}</DropdownMenuItem>}
                        {isMedia && <><DropdownMenuItem disabled={maintainingId === article.id} onClick={() => void runMaintenance("delete_article_subtitles_cmd", article.id)}>{t("articleList.deleteSubtitles", "删除字幕")}</DropdownMenuItem><DropdownMenuItem disabled={maintainingId === article.id} onClick={() => void runMaintenance("delete_article_analysis_cmd", article.id)}>{t("articleList.deleteAnalysis", "删除翻译解析")}</DropdownMenuItem></>}
                        <DropdownMenuItem className="text-destructive focus:text-destructive" onClick={() => { setSelectedIds([article.id]); setPendingAction("delete"); }}><Trash2 className="mr-2" size={15} />{t("articleList.delete", "删除")}</DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </article>
                );
              })}
            </div>
          )
        )}
      </section>

      <Dialog open={pendingAction !== null} onOpenChange={(open) => !open && !isApplyingAction && setPendingAction(null)} title={pendingAction === "delete" ? t("articleList.delete", "删除素材") : t("materials.archive", "归档素材")}>
        <p className="text-sm text-muted-foreground">{pendingAction === "delete" ? t("materials.deleteSelectedConfirm", { count: selectedIds.length, defaultValue: `将删除 ${selectedIds.length} 项素材，此操作不可恢复。` }) : t("materials.archiveSelectedConfirm", { count: selectedIds.length, defaultValue: `将归档 ${selectedIds.length} 项素材。` })}</p>
        <DialogFooter>
          <Button variant="ghost" size="sm" disabled={isApplyingAction} onClick={() => setPendingAction(null)}>{t("common.cancel", "取消")}</Button>
          <Button variant={pendingAction === "delete" ? "danger" : "default"} size="sm" disabled={isApplyingAction} onClick={() => void applyBulkAction()}>{isApplyingAction && <Loader2 className="mr-1.5 animate-spin" size={15} />}{pendingAction === "delete" ? t("articleList.delete", "删除") : t("materials.archive", "归档")}</Button>
        </DialogFooter>
      </Dialog>
    </section>
  );
}

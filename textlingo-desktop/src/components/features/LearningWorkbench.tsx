import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  Archive,
  Check,
  CircleAlert,
  FileCheck2,
  GitMerge,
  Loader2,
  LocateFixed,
  RefreshCw,
  RotateCcw,
  Save,
  Tags,
  X,
} from "lucide-react";

import {
  createLearningItemDraft,
  LEARNING_ITEM_STATUSES,
  LEARNING_ITEM_STATUS_LABELS,
  LEARNING_ITEM_TYPES,
  LEARNING_ITEM_TYPE_LABELS,
  learningWorkbenchApi as defaultLearningWorkbenchApi,
  normalizeTags,
  QUALITY_FLAG_LABELS,
  restoreStatusFor,
  type BulkOrganizeLearningItemInput,
  type BulkOrganizeLearningItemResult,
  type CanonicalLearningItemStatus,
  type LearningItemDraft,
  type LearningMaterialOption,
  type LearningQualityFlag,
  type LearningReview,
  type LegacyLearningItemMigrationResult,
  type LearningWorkbenchApi,
  type LearningWorkbenchItem,
  type ListLearningWorkbenchItemsQuery,
} from "../../features/learning";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Select } from "../ui/select";
import { Textarea } from "../ui/textarea";

type FilterValue = "all" | string;

interface BulkFeedback {
  succeeded: number;
  failed: number;
  failures: BulkOrganizeLearningItemResult[];
}

export interface LearningWorkbenchProps {
  api?: LearningWorkbenchApi;
  materials?: LearningMaterialOption[];
  initialMaterialId?: string;
  initialItemId?: string;
  onNavigateToSource: (item: LearningWorkbenchItem) => void;
  onError?: (message: string) => void;
  onSuccess?: (message: string) => void;
  className?: string;
}

function sourceTypeLabel(sourceType?: string | null): string {
  switch (sourceType) {
    case "article": return "文章";
    case "web": return "网页";
    case "text":
    case "text_file": return "文本";
    case "book": return "书籍";
    case "video": return "视频";
    case "audio": return "音频";
    default: return sourceType || "未知类型";
  }
}

function itemStatusLabel(status: string): string {
  return LEARNING_ITEM_STATUS_LABELS[status as CanonicalLearningItemStatus] ?? status;
}

function itemTypeLabel(itemType: string): string {
  return LEARNING_ITEM_TYPE_LABELS[itemType as keyof typeof LEARNING_ITEM_TYPE_LABELS] ?? itemType;
}

function qualityFlagLabel(flag: string): string {
  return QUALITY_FLAG_LABELS[flag] ?? flag;
}

function replaceNeedsVerification(flags: LearningQualityFlag[], enabled: boolean): LearningQualityFlag[] {
  const next = flags.filter((flag) => flag !== "needs_verification");
  return enabled ? [...next, "needs_verification"] : next;
}

function mergeMaterialOptions(
  materials: LearningMaterialOption[],
  items: LearningWorkbenchItem[],
): LearningMaterialOption[] {
  const byId = new Map(materials.map((material) => [material.id, material]));
  for (const item of items) {
    if (!item.material_id || byId.has(item.material_id)) continue;
    byId.set(item.material_id, {
      id: item.material_id,
      title: item.source_material_title || "未命名素材",
      sourceType: item.source_type || undefined,
    });
  }
  return Array.from(byId.values()).sort((left, right) => left.title.localeCompare(right.title, "zh-CN"));
}

export function LearningWorkbench({
  api = defaultLearningWorkbenchApi,
  materials = [],
  initialMaterialId,
  initialItemId,
  onNavigateToSource,
  onError,
  onSuccess,
  className,
}: LearningWorkbenchProps) {
  const [status, setStatus] = useState<FilterValue>("candidate");
  const [itemType, setItemType] = useState<FilterValue>("all");
  const [materialId, setMaterialId] = useState<FilterValue>(initialMaterialId || "all");
  const [sourceType, setSourceType] = useState<FilterValue>("all");
  const [tag, setTag] = useState<FilterValue>("all");
  const [qualityFlag, setQualityFlag] = useState<FilterValue>("all");
  const [items, setItems] = useState<LearningWorkbenchItem[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [activeItemId, setActiveItemId] = useState<string | null>(null);
  const [draft, setDraft] = useState<LearningItemDraft | null>(null);
  const [bulkTagsInput, setBulkTagsInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isBulkPending, setIsBulkPending] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [bulkFeedback, setBulkFeedback] = useState<BulkFeedback | null>(null);
  const [review, setReview] = useState<LearningReview | null>(null);
  const [migration, setMigration] = useState<LegacyLearningItemMigrationResult | null>(null);
  const [isReviewLoading, setIsReviewLoading] = useState(false);
  const [isPreviewPending, setIsPreviewPending] = useState(false);
  const [isMigrationPending, setIsMigrationPending] = useState(false);
  const loadRequestId = useRef(0);
  const handledInitialItemId = useRef<string | null>(null);

  const buildQuery = useCallback((): ListLearningWorkbenchItemsQuery => ({
    status: status === "all" ? undefined : status,
    item_type: itemType === "all" ? undefined : itemType,
    material_id: materialId === "all" ? undefined : materialId,
    source_type: sourceType === "all" ? undefined : sourceType,
    tag: tag === "all" ? undefined : tag,
    quality_flag: qualityFlag === "all" ? undefined : qualityFlag,
    limit: 300,
    offset: 0,
  }), [itemType, materialId, qualityFlag, sourceType, status, tag]);

  const loadItems = useCallback(async () => {
    const requestId = ++loadRequestId.current;
    setIsLoading(true);
    setLoadError(null);
    try {
      const listed = await api.list(buildQuery());
      if (requestId !== loadRequestId.current) return;
      const focusItemId = initialItemId && handledInitialItemId.current !== initialItemId ? initialItemId : undefined;
      let focusedItem = focusItemId ? listed.find((item) => item.id === focusItemId) : undefined;
      if (focusItemId && !focusedItem && api.get) {
        focusedItem = await api.get(focusItemId);
        if (requestId !== loadRequestId.current) return;
      }
      if (focusItemId && focusedItem) handledInitialItemId.current = focusItemId;
      const visibleItems = focusedItem && !listed.some((item) => item.id === focusedItem.id)
        ? [focusedItem, ...listed]
        : listed;
      setItems(visibleItems);
      setSelectedIds((current) => new Set(Array.from(current).filter((id) => listed.some((item) => item.id === id))));
      setActiveItemId((current) => focusedItem?.id ?? (current && visibleItems.some((item) => item.id === current)
        ? current
        : visibleItems[0]?.id ?? null));
      if (focusedItem && status !== focusedItem.status && LEARNING_ITEM_STATUSES.includes(focusedItem.status as CanonicalLearningItemStatus)) {
        setStatus(focusedItem.status);
      }
    } catch (reason) {
      if (requestId !== loadRequestId.current) return;
      const message = String(reason);
      setLoadError(message);
      onError?.(message);
    } finally {
      if (requestId === loadRequestId.current) setIsLoading(false);
    }
  }, [api, buildQuery, initialItemId, onError, status]);

  useEffect(() => () => {
    loadRequestId.current += 1;
  }, []);

  useEffect(() => {
    void loadItems();
  }, [loadItems]);

  const loadReview = useCallback(async () => {
    if (materialId === "all" ? !api.getDailyReview : !api.getMaterialReview) return;
    setIsReviewLoading(true);
    try {
      const loaded = materialId === "all"
        ? await api.getDailyReview?.(-new Date().getTimezoneOffset())
        : await api.getMaterialReview?.(materialId);
      if (loaded) setReview(loaded);
    } catch (reason) {
      onError?.(`复盘摘要加载失败：${String(reason)}`);
    } finally {
      setIsReviewLoading(false);
    }
  }, [api, materialId, onError]);

  useEffect(() => {
    void loadReview();
  }, [loadReview]);

  const activeItem = useMemo(
    () => items.find((item) => item.id === activeItemId) ?? null,
    [activeItemId, items],
  );

  useEffect(() => {
    setDraft(activeItem ? createLearningItemDraft(activeItem) : null);
  }, [activeItem]);

  const knownMaterials = useMemo(() => mergeMaterialOptions(materials, items), [items, materials]);
  const knownTags = useMemo(
    () => Array.from(new Set(items.flatMap((item) => item.tags))).sort((left, right) => left.localeCompare(right, "zh-CN")),
    [items],
  );
  const knownQualityFlags = useMemo(
    () => Array.from(new Set([
      "needs_verification",
      "possible_duplicate",
      "insufficient_context",
      ...items.flatMap((item) => item.quality_flags),
    ])).sort((left, right) => qualityFlagLabel(left).localeCompare(qualityFlagLabel(right), "zh-CN")),
    [items],
  );
  const knownSourceTypes = useMemo(
    () => Array.from(new Set([
      ...materials.map((material) => material.sourceType).filter((value): value is string => Boolean(value)),
      ...items.map((item) => item.source_type).filter((value): value is string => Boolean(value)),
    ])).sort(),
    [items, materials],
  );
  const selectedItems = useMemo(
    () => items.filter((item) => selectedIds.has(item.id)),
    [items, selectedIds],
  );
  const allVisibleSelected = items.length > 0 && items.every((item) => selectedIds.has(item.id));

  const replaceItem = (updated: LearningWorkbenchItem) => {
    setItems((current) => current.map((item) => item.id === updated.id ? updated : item));
  };

  const handleLocalPreview = async () => {
    if (!activeItem || !api.recordLocalPreview) return;
    setIsPreviewPending(true);
    try {
      await api.recordLocalPreview(activeItem.id, { origin: "learning_workbench" });
      onSuccess?.("本地预习已记录");
      await loadReview();
    } catch (reason) {
      onError?.(`本地预习记录失败：${String(reason)}`);
    } finally {
      setIsPreviewPending(false);
    }
  };

  const handleLegacyMigration = async (dryRun: boolean) => {
    if (!api.migrateLegacy) return;
    setIsMigrationPending(true);
    try {
      const result = await api.migrateLegacy(dryRun);
      setMigration(result);
      onSuccess?.(dryRun ? "兼容数据检查完成" : `兼容迁移完成：${result.migrated} 项`);
      if (!dryRun) await Promise.all([loadItems(), loadReview()]);
    } catch (reason) {
      onError?.(`兼容迁移失败：${String(reason)}`);
    } finally {
      setIsMigrationPending(false);
    }
  };

  const handleSave = async () => {
    if (!activeItem || !draft) return;
    setIsSaving(true);
    setLoadError(null);
    try {
      const updated = await api.update(activeItem.id, {
        item_type: draft.itemType,
        text: draft.text.trim(),
        meaning_in_context: draft.meaningInContext.trim() || null,
        definition_zh: draft.definitionZh.trim() || null,
        definition_en: draft.definitionEn.trim() || null,
        tags: normalizeTags(draft.tagsInput),
        quality_flags: replaceNeedsVerification(activeItem.quality_flags, draft.needsVerification),
      });
      replaceItem(updated);
      onSuccess?.("候选项已保存");
    } catch (reason) {
      const message = String(reason);
      setLoadError(message);
      onError?.(message);
    } finally {
      setIsSaving(false);
    }
  };

  const handleBulk = async (actionLabel: string, changes: BulkOrganizeLearningItemInput[]) => {
    if (changes.length === 0) return;
    setIsBulkPending(true);
    setBulkFeedback(null);
    try {
      const response = await api.bulkOrganize(changes);
      const failures = response.results.filter((result) => !result.success);
      setBulkFeedback({ succeeded: response.succeeded, failed: response.failed, failures });
      setSelectedIds(new Set(failures.map((failure) => failure.id)));
      for (const result of response.results) {
        if (result.success && result.item) replaceItem(result.item);
      }
      if (response.succeeded > 0) onSuccess?.(`${actionLabel}：${response.succeeded} 项成功`);
      if (response.failed > 0) onError?.(`${actionLabel}：${response.failed} 项失败`);
      await loadItems();
    } catch (reason) {
      const message = String(reason);
      setBulkFeedback({
        succeeded: 0,
        failed: changes.length,
        failures: changes.map(({ id }) => ({ id, success: false, error: { code: "request_failed", message } })),
      });
      onError?.(message);
    } finally {
      setIsBulkPending(false);
    }
  };

  const acceptChanges = selectedItems
    .filter((item) => item.status !== "accepted")
    .map((item) => ({
      id: item.id,
      status: "accepted" as const,
      favorite_type: item.item_type === "grammar" ? "grammar" as const : "vocabulary" as const,
      pack_ids: [],
    }));
  const rejectChanges = selectedItems
    .filter((item) => item.status !== "rejected")
    .map((item) => ({ id: item.id, status: "rejected" as const }));
  const archiveChanges = selectedItems
    .filter((item) => item.status !== "archived")
    .map((item) => ({ id: item.id, status: "archived" as const }));
  const restoreChanges = selectedItems
    .filter((item) => item.status === "archived" && !item.merged_into_id)
    .map((item) => ({ id: item.id, status: restoreStatusFor(item) }));
  const mergeChanges = activeItem && selectedIds.has(activeItem.id)
    ? selectedItems
      .filter((item) => item.id !== activeItem.id)
      .map((item) => ({ id: item.id, merge_into_id: activeItem.id }))
    : [];

  return (
    <section
      className={`flex min-h-0 flex-col overflow-hidden rounded-xl border border-border bg-background ${className ?? ""}`}
      aria-label="学习候选工作台"
      data-testid="learning-workbench"
    >
      <header className="border-b border-border p-4">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-semibold text-foreground">学习候选工作台</h2>
            <p className="mt-0.5 text-xs text-muted-foreground">人工审核、批量整理并保留原文证据</p>
          </div>
          <Button variant="ghost" size="icon" className="h-8 w-8" aria-label="刷新候选项" title="刷新候选项" onClick={() => void loadItems()} disabled={isLoading}>
            {isLoading ? <Loader2 size={15} className="animate-spin" /> : <RefreshCw size={15} />}
          </Button>
        </div>

        <div className="mt-3 grid gap-2 sm:grid-cols-2 xl:grid-cols-6">
          <label className="text-xs text-muted-foreground">
            状态
            <Select aria-label="筛选状态" value={status} onChange={(event) => setStatus(event.target.value)} className="mt-1 h-8">
              <option value="all">全部状态</option>
              {LEARNING_ITEM_STATUSES.map((value) => <option key={value} value={value}>{LEARNING_ITEM_STATUS_LABELS[value]}</option>)}
            </Select>
          </label>
          <label className="text-xs text-muted-foreground">
            类型
            <Select aria-label="筛选类型" value={itemType} onChange={(event) => setItemType(event.target.value)} className="mt-1 h-8">
              <option value="all">全部类型</option>
              {LEARNING_ITEM_TYPES.map((value) => <option key={value} value={value}>{LEARNING_ITEM_TYPE_LABELS[value]}</option>)}
            </Select>
          </label>
          <label className="text-xs text-muted-foreground">
            来源素材
            <Select aria-label="筛选来源素材" value={materialId} onChange={(event) => setMaterialId(event.target.value)} className="mt-1 h-8">
              <option value="all">全部素材</option>
              {knownMaterials.map((material) => <option key={material.id} value={material.id}>{material.title}</option>)}
            </Select>
          </label>
          <label className="text-xs text-muted-foreground">
            来源类型
            <Select aria-label="筛选来源类型" value={sourceType} onChange={(event) => setSourceType(event.target.value)} className="mt-1 h-8">
              <option value="all">全部来源</option>
              {knownSourceTypes.map((value) => <option key={value} value={value}>{sourceTypeLabel(value)}</option>)}
            </Select>
          </label>
          <label className="text-xs text-muted-foreground">
            标签
            <Select aria-label="筛选标签" value={tag} onChange={(event) => setTag(event.target.value)} className="mt-1 h-8">
              <option value="all">全部标签</option>
              {knownTags.map((value) => <option key={value} value={value}>{value}</option>)}
            </Select>
          </label>
          <label className="text-xs text-muted-foreground">
            质量标记
            <Select aria-label="筛选质量标记" value={qualityFlag} onChange={(event) => setQualityFlag(event.target.value)} className="mt-1 h-8">
              <option value="all">全部质量标记</option>
              {knownQualityFlags.map((value) => <option key={value} value={value}>{qualityFlagLabel(value)}</option>)}
            </Select>
          </label>
        </div>
      </header>

      {(api.getDailyReview || api.getMaterialReview || api.migrateLegacy) && (
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-primary/5 px-4 py-2 text-xs">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-muted-foreground" aria-label="本地复盘摘要">
            <span className="flex items-center gap-1 font-medium text-foreground">
              {isReviewLoading ? <Loader2 size={13} className="animate-spin" /> : <Activity size={13} />}
              {materialId === "all" ? "今日复盘" : "素材复盘"}
            </span>
            <span>活动 {review?.total_events ?? 0}</span>
            <span>学习项 {review?.unique_learning_items ?? 0}</span>
            <span>阅读 {review?.event_counts.read ?? 0}</span>
            <span>接受 {review?.event_counts.accept ?? 0}</span>
            <span>本地预习 {review?.event_counts.local_preview ?? 0}</span>
          </div>
          {api.migrateLegacy && (
            <div className="flex flex-wrap items-center gap-2">
              {migration && (
                <span className="text-muted-foreground">
                  {migration.dry_run ? "检查" : "迁移"}：计划 {migration.planned}，完成 {migration.migrated}，冲突 {migration.conflicts.length}
                </span>
              )}
              <Button variant="ghost" size="sm" className="h-7" disabled={isMigrationPending} onClick={() => void handleLegacyMigration(true)}>
                {isMigrationPending ? <Loader2 size={13} className="mr-1 animate-spin" /> : null}兼容数据检查
              </Button>
              {migration?.dry_run && migration.planned > 0 && (
                <Button variant="outline" size="sm" className="h-7" disabled={isMigrationPending} onClick={() => void handleLegacyMigration(false)}>
                  执行兼容迁移
                </Button>
              )}
            </div>
          )}
        </div>
      )}

      <div className="border-b border-border bg-muted/20 px-4 py-3">
        <div className="flex flex-wrap items-center gap-2">
          <label className="mr-1 flex items-center gap-2 text-xs text-muted-foreground">
            <input
              type="checkbox"
              aria-label="选择全部当前候选项"
              checked={allVisibleSelected}
              onChange={(event) => setSelectedIds(event.target.checked ? new Set(items.map((item) => item.id)) : new Set())}
            />
            已选 {selectedIds.size} 项
          </label>
          <Button size="sm" className="h-8 gap-1" disabled={isBulkPending || acceptChanges.length === 0} onClick={() => void handleBulk("批量接受", acceptChanges)}>
            <Check size={14} />批量接受
          </Button>
          <Button variant="secondary" size="sm" className="h-8 gap-1" disabled={isBulkPending || rejectChanges.length === 0} onClick={() => void handleBulk("批量拒绝", rejectChanges)}>
            <X size={14} />批量拒绝
          </Button>
          <Button variant="outline" size="sm" className="h-8 gap-1" disabled={isBulkPending || archiveChanges.length === 0} onClick={() => void handleBulk("批量归档", archiveChanges)}>
            <Archive size={14} />批量归档
          </Button>
          <Button variant="outline" size="sm" className="h-8 gap-1" disabled={isBulkPending || restoreChanges.length === 0} onClick={() => void handleBulk("批量恢复", restoreChanges)}>
            <RotateCcw size={14} />批量恢复
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1"
            disabled={isBulkPending || selectedItems.length === 0}
            onClick={() => void handleBulk("标记待核验", selectedItems.map((item) => ({ id: item.id, quality_flags: replaceNeedsVerification(item.quality_flags, true) })))}
          >
            <CircleAlert size={14} />标记待核验
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-8"
            disabled={isBulkPending || !selectedItems.some((item) => item.quality_flags.includes("needs_verification"))}
            onClick={() => void handleBulk("清除待核验", selectedItems.map((item) => ({ id: item.id, quality_flags: replaceNeedsVerification(item.quality_flags, false) })))}
          >
            清除待核验
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1"
            disabled={isBulkPending || mergeChanges.length === 0}
            title="保留当前详情项，并将其余选中项归档为重复项"
            onClick={() => void handleBulk("合并到当前项", mergeChanges)}
          >
            <GitMerge size={14} />合并到当前项
          </Button>
          {isBulkPending && <Loader2 aria-label="正在执行批量操作" size={15} className="animate-spin text-muted-foreground" />}
        </div>
        <div className="mt-2 flex max-w-xl items-center gap-2">
          <Input aria-label="批量标签" value={bulkTagsInput} onChange={(event) => setBulkTagsInput(event.target.value)} className="h-8" placeholder="逗号分隔，将替换已选项标签" />
          <Button
            variant="outline"
            size="sm"
            className="h-8 shrink-0 gap-1"
            disabled={isBulkPending || selectedItems.length === 0}
            onClick={() => void handleBulk("批量设置标签", selectedItems.map((item) => ({ id: item.id, tags: normalizeTags(bulkTagsInput) })))}
          >
            <Tags size={14} />设置标签
          </Button>
        </div>
      </div>

      {loadError && (
        <div role="alert" className="flex items-center justify-between gap-3 border-b border-destructive/30 bg-destructive/10 px-4 py-2 text-sm text-destructive">
          <span>加载或保存失败：{loadError}</span>
          <Button variant="ghost" size="sm" className="h-7" onClick={() => void loadItems()}>重试</Button>
        </div>
      )}

      {bulkFeedback && (
        <div role={bulkFeedback.failed > 0 ? "alert" : "status"} className={`border-b px-4 py-2 text-sm ${bulkFeedback.failed > 0 ? "border-amber-300 bg-amber-50 text-amber-900 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200" : "border-emerald-300 bg-emerald-50 text-emerald-900 dark:border-emerald-900 dark:bg-emerald-950/30 dark:text-emerald-200"}`}>
          <p>批量操作完成：{bulkFeedback.succeeded} 项成功，{bulkFeedback.failed} 项失败。</p>
          {bulkFeedback.failures.length > 0 && (
            <ul className="mt-1 list-disc pl-5 text-xs">
              {bulkFeedback.failures.map((failure) => (
                <li key={failure.id}>{failure.id}：{failure.error?.message || "未知错误"}</li>
              ))}
            </ul>
          )}
        </div>
      )}

      <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(300px,0.85fr)_minmax(420px,1.15fr)]">
        <div className="min-h-0 overflow-y-auto border-r border-border" aria-live="polite">
          {isLoading && items.length === 0 && (
            <div className="flex items-center justify-center gap-2 px-4 py-12 text-sm text-muted-foreground">
              <Loader2 size={16} className="animate-spin" />正在加载候选项
            </div>
          )}
          {!isLoading && !loadError && items.length === 0 && (
            <div className="px-4 py-12 text-center">
              <FileCheck2 size={28} className="mx-auto text-muted-foreground" />
              <p className="mt-2 text-sm text-muted-foreground">暂无匹配的学习项</p>
            </div>
          )}
          {items.map((item) => {
            const isActive = activeItemId === item.id;
            const needsVerification = item.quality_flags.includes("needs_verification");
            return (
              <article key={item.id} className={`border-b border-border p-3 last:border-b-0 ${isActive ? "bg-primary/5" : "hover:bg-muted/30"}`} data-testid={`learning-item-${item.id}`}>
                <div className="flex items-start gap-2">
                  <input
                    type="checkbox"
                    className="mt-1"
                    aria-label={`选择 ${item.text}`}
                    checked={selectedIds.has(item.id)}
                    onChange={(event) => setSelectedIds((current) => {
                      const next = new Set(current);
                      if (event.target.checked) next.add(item.id);
                      else next.delete(item.id);
                      return next;
                    })}
                  />
                  <button className="min-w-0 flex-1 text-left" onClick={() => setActiveItemId(item.id)} aria-label={`查看 ${item.text}`}>
                    <div className="flex flex-wrap items-center gap-1.5">
                      <span className="break-words text-sm font-medium text-foreground">{item.text}</span>
                      <span className="rounded bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">{itemTypeLabel(item.item_type)}</span>
                      <span className="rounded bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">{itemStatusLabel(item.status)}</span>
                      {needsVerification && <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[11px] text-amber-800 dark:bg-amber-950/50 dark:text-amber-300">待人工核验</span>}
                    </div>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{item.source_sentence || "无原文句子"}</p>
                    <p className="mt-1 truncate text-[11px] text-muted-foreground">{item.source_material_title || "未知素材"} · {sourceTypeLabel(item.source_type)}</p>
                  </button>
                </div>
              </article>
            );
          })}
        </div>

        <div className="min-h-0 overflow-y-auto p-4">
          {!activeItem || !draft ? (
            <p className="py-12 text-center text-sm text-muted-foreground">选择一个学习项查看详情</p>
          ) : (
            <div data-testid="learning-item-detail">
              <div className="flex flex-wrap items-start justify-between gap-2">
                <div>
                  <h3 className="text-sm font-semibold text-foreground">候选项详情</h3>
                  <p className="mt-0.5 text-xs text-muted-foreground">{activeItem.source_material_title || "未知素材"} · {sourceTypeLabel(activeItem.source_type)}</p>
                </div>
                <Button variant="outline" size="sm" className="h-8 gap-1" onClick={() => onNavigateToSource(activeItem)} disabled={!activeItem.material_id}>
                  <LocateFixed size={14} />返回原文
                </Button>
              </div>

              <div className="mt-4 grid gap-3 sm:grid-cols-[150px_minmax(0,1fr)]">
                <label className="text-xs text-muted-foreground">
                  类型
                  <Select aria-label="编辑候选项类型" value={draft.itemType} onChange={(event) => setDraft({ ...draft, itemType: event.target.value })} className="mt-1 h-9">
                    {LEARNING_ITEM_TYPES.map((value) => <option key={value} value={value}>{LEARNING_ITEM_TYPE_LABELS[value]}</option>)}
                  </Select>
                </label>
                <label className="text-xs text-muted-foreground">
                  学习内容
                  <Input aria-label="编辑学习内容" value={draft.text} onChange={(event) => setDraft({ ...draft, text: event.target.value })} className="mt-1 h-9" />
                </label>
              </div>

              <label className="mt-3 block text-xs text-muted-foreground">
                语境含义
                <Textarea aria-label="编辑语境含义" value={draft.meaningInContext} onChange={(event) => setDraft({ ...draft, meaningInContext: event.target.value })} className="mt-1 min-h-20" />
              </label>
              <div className="mt-3 grid gap-3 sm:grid-cols-2">
                <label className="text-xs text-muted-foreground">
                  中文定义
                  <Textarea aria-label="编辑中文定义" value={draft.definitionZh} onChange={(event) => setDraft({ ...draft, definitionZh: event.target.value })} className="mt-1 min-h-20" />
                </label>
                <label className="text-xs text-muted-foreground">
                  英文定义
                  <Textarea aria-label="编辑英文定义" value={draft.definitionEn} onChange={(event) => setDraft({ ...draft, definitionEn: event.target.value })} className="mt-1 min-h-20" />
                </label>
              </div>
              <label className="mt-3 block text-xs text-muted-foreground">
                标签
                <Input aria-label="编辑候选项标签" value={draft.tagsInput} onChange={(event) => setDraft({ ...draft, tagsInput: event.target.value })} className="mt-1 h-9" placeholder="逗号分隔" />
              </label>
              <label className="mt-3 flex items-center gap-2 rounded-lg border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
                <input type="checkbox" aria-label="待人工核验" checked={draft.needsVerification} onChange={(event) => setDraft({ ...draft, needsVerification: event.target.checked })} />
                待人工核验（适用于专业词、医学词和统计学词）
              </label>

              <section className="mt-5 rounded-lg border border-border bg-muted/20 p-3" aria-label="原文证据">
                <h4 className="text-xs font-semibold text-foreground">原文证据</h4>
                <blockquote className="mt-2 whitespace-pre-wrap border-l-2 border-primary/50 pl-3 text-sm text-foreground">{activeItem.source_sentence || "无原文句子"}</blockquote>
                {(activeItem.context_before || activeItem.context_after) && (
                  <div className="mt-3 grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
                    <div><span className="font-medium text-foreground">上文：</span>{activeItem.context_before || "无"}</div>
                    <div><span className="font-medium text-foreground">下文：</span>{activeItem.context_after || "无"}</div>
                  </div>
                )}
                <p className="mt-3 text-xs text-muted-foreground">来源：{activeItem.source_material_title || "未知素材"}{activeItem.segment_id ? ` · 段落 ${activeItem.segment_id}` : ""}</p>
              </section>

              {activeItem.quality_flags.filter((flag) => flag !== "needs_verification").length > 0 && (
                <div className="mt-3 flex flex-wrap gap-1" aria-label="其他质量标记">
                  {activeItem.quality_flags.filter((flag) => flag !== "needs_verification").map((flag) => (
                    <span key={flag} className="rounded bg-muted px-2 py-1 text-xs text-muted-foreground">{qualityFlagLabel(flag)}</span>
                  ))}
                </div>
              )}

              <div className="mt-4 flex flex-wrap justify-end gap-2">
                {api.recordLocalPreview && (
                  <Button variant="outline" className="gap-1" onClick={() => void handleLocalPreview()} disabled={isPreviewPending || activeItem.status !== "accepted"} title={activeItem.status === "accepted" ? "记录一次本地预习" : "接受后可进行本地预习"}>
                    {isPreviewPending ? <Loader2 size={15} className="animate-spin" /> : <Activity size={15} />}本地预习
                  </Button>
                )}
                <Button className="gap-1" onClick={() => void handleSave()} disabled={isSaving || !draft.text.trim()}>
                  {isSaving ? <Loader2 size={15} className="animate-spin" /> : <Save size={15} />}保存候选项
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

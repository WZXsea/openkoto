import { useEffect, useMemo, useState } from "react";
import { BookOpenCheck, Check, Loader2, LocateFixed, RefreshCw, Trash2 } from "lucide-react";

import type { Annotation, AnnotationKind, UpdateAnnotationInput } from "../../types";
import {
  annotationsApi as defaultAnnotationsApi,
  formatAnnotationLocator,
  getAnnotationLocatorStatus,
  type AnnotationsApi,
} from "../../features/annotations";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Select } from "../ui/select";
import { Textarea } from "../ui/textarea";

const ANNOTATION_KINDS: AnnotationKind[] = ["highlight", "excerpt", "note", "vocabulary", "grammar"];

const KIND_LABELS: Record<AnnotationKind, string> = {
  highlight: "高亮",
  excerpt: "摘录",
  note: "笔记",
  vocabulary: "词汇",
  grammar: "语法",
};

interface AnnotationDraft {
  note: NonNullable<UpdateAnnotationInput["note"]>;
  color: NonNullable<UpdateAnnotationInput["color"]>;
  tagsInput: string;
}

export interface AnnotationMaterialOption {
  id: string;
  title: string;
}

export interface AnnotationWorkbenchProps {
  materialId?: string;
  materials?: AnnotationMaterialOption[];
  annotationsApi?: AnnotationsApi;
  onNavigateToSource: (annotation: Annotation) => void;
  onConverted?: (annotation: Annotation) => void;
  onError?: (message: string) => void;
  className?: string;
}

function createDraft(annotation: Annotation): AnnotationDraft {
  return {
    note: annotation.note ?? "",
    color: annotation.color ?? "#facc15",
    tagsInput: annotation.tags.join(", "),
  };
}

function normalizeTags(value: string): string[] {
  return Array.from(new Set(value.split(",").map((tag) => tag.trim()).filter(Boolean)));
}

function statusLabel(annotation: Annotation): string {
  switch (getAnnotationLocatorStatus(annotation)) {
    case "precise":
      return "精确定位";
    case "fallback":
      return "降级定位";
    default:
      return "定位失效";
  }
}

function statusClass(annotation: Annotation): string {
  switch (getAnnotationLocatorStatus(annotation)) {
    case "precise":
      return "text-emerald-700 dark:text-emerald-400";
    case "fallback":
      return "text-amber-700 dark:text-amber-400";
    default:
      return "text-destructive";
  }
}

export function AnnotationWorkbench({
  materialId,
  materials = [],
  annotationsApi: api = defaultAnnotationsApi,
  onNavigateToSource,
  onConverted,
  onError,
  className,
}: AnnotationWorkbenchProps) {
  const [selectedMaterialId, setSelectedMaterialId] = useState(materialId ?? "");
  const [kind, setKind] = useState<AnnotationKind | "">("");
  const [tag, setTag] = useState("");
  const [query, setQuery] = useState("");
  const [createdAfter, setCreatedAfter] = useState("");
  const [createdBefore, setCreatedBefore] = useState("");
  const [annotations, setAnnotations] = useState<Annotation[]>([]);
  const [drafts, setDrafts] = useState<Record<string, AnnotationDraft>>({});
  const [isLoading, setIsLoading] = useState(false);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setSelectedMaterialId(materialId ?? "");
  }, [materialId]);

  const knownTags = useMemo(() => Array.from(new Set(annotations.flatMap((annotation) => annotation.tags))).sort(), [annotations]);

  const loadAnnotations = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const listed = await api.list({
        material_id: selectedMaterialId || undefined,
        kind: kind || undefined,
        tag: tag || undefined,
        q: query.trim() || undefined,
        ...(createdAfter ? { created_after: new Date(createdAfter).toISOString() } : {}),
        ...(createdBefore ? { created_before: new Date(createdBefore).toISOString() } : {}),
        limit: 200,
        offset: 0,
      });
      setAnnotations(listed);
      setDrafts(Object.fromEntries(listed.map((annotation) => [annotation.id, createDraft(annotation)])));
    } catch (reason) {
      const message = String(reason);
      setError(message);
      onError?.(message);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void loadAnnotations();
  }, [api, selectedMaterialId, kind, tag, query, createdAfter, createdBefore]);

  const updateDraft = (id: string, patch: Partial<AnnotationDraft>) => {
    setDrafts((current) => ({ ...current, [id]: { ...current[id], ...patch } }));
  };

  const replaceAnnotation = (updated: Annotation) => {
    setAnnotations((current) => current.map((annotation) => annotation.id === updated.id ? updated : annotation));
    setDrafts((current) => ({ ...current, [updated.id]: createDraft(updated) }));
  };

  const handleSave = async (annotation: Annotation) => {
    const draft = drafts[annotation.id] ?? createDraft(annotation);
    setPendingId(annotation.id);
    try {
      const updated = await api.update(annotation.id, {
        note: draft.note || null,
        color: draft.color || null,
        tags: normalizeTags(draft.tagsInput),
      });
      replaceAnnotation(updated);
    } catch (reason) {
      const message = String(reason);
      setError(message);
      onError?.(message);
    } finally {
      setPendingId(null);
    }
  };

  const handleDelete = async (annotation: Annotation) => {
    setPendingId(annotation.id);
    try {
      await api.remove(annotation.id);
      setAnnotations((current) => current.filter((item) => item.id !== annotation.id));
      setDrafts((current) => {
        const next = { ...current };
        delete next[annotation.id];
        return next;
      });
    } catch (reason) {
      const message = String(reason);
      setError(message);
      onError?.(message);
    } finally {
      setPendingId(null);
    }
  };

  const handleConvert = async (annotation: Annotation) => {
    setPendingId(annotation.id);
    try {
      const result = await api.convertToLearningItem(annotation.id);
      replaceAnnotation(result.annotation);
      onConverted?.(result.annotation);
    } catch (reason) {
      const message = String(reason);
      setError(message);
      onError?.(message);
    } finally {
      setPendingId(null);
    }
  };

  return (
    <section className={`flex min-h-0 flex-col border border-border bg-background ${className ?? ""}`} aria-label="批注工作台" data-testid="annotation-workbench">
      <div className="flex flex-wrap items-end gap-2 border-b border-border p-3">
        <label className="min-w-40 flex-1 text-xs text-muted-foreground">
          素材
          <Select aria-label="筛选素材" value={selectedMaterialId} onChange={(event) => setSelectedMaterialId(event.target.value)} className="mt-1 h-8">
            <option value="">全部素材</option>
            {materials.map((material) => <option key={material.id} value={material.id}>{material.title}</option>)}
          </Select>
        </label>
        <label className="min-w-28 flex-1 text-xs text-muted-foreground">
          类型
          <Select aria-label="筛选类型" value={kind} onChange={(event) => setKind(event.target.value as AnnotationKind | "")} className="mt-1 h-8">
            <option value="">全部类型</option>
            {ANNOTATION_KINDS.map((value) => <option key={value} value={value}>{KIND_LABELS[value]}</option>)}
          </Select>
        </label>
        <label className="min-w-28 flex-1 text-xs text-muted-foreground">
          标签
          <Select aria-label="筛选标签" value={tag} onChange={(event) => setTag(event.target.value)} className="mt-1 h-8">
            <option value="">全部标签</option>
            {knownTags.map((value) => <option key={value} value={value}>{value}</option>)}
          </Select>
        </label>
        <label className="min-w-44 flex-[2] text-xs text-muted-foreground">
          关键词
          <Input aria-label="筛选关键词" value={query} onChange={(event) => setQuery(event.target.value)} className="mt-1 h-8" placeholder="文本、笔记或标签" />
        </label>
        <label className="min-w-44 text-xs text-muted-foreground">
          起始时间
          <Input aria-label="筛选起始时间" type="datetime-local" value={createdAfter} onChange={(event) => setCreatedAfter(event.target.value)} className="mt-1 h-8" />
        </label>
        <label className="min-w-44 text-xs text-muted-foreground">
          截止时间
          <Input aria-label="筛选截止时间" type="datetime-local" value={createdBefore} onChange={(event) => setCreatedBefore(event.target.value)} className="mt-1 h-8" />
        </label>
        <Button variant="ghost" size="icon" className="h-8 w-8" title="刷新批注" aria-label="刷新批注" onClick={() => void loadAnnotations()} disabled={isLoading}>
          {isLoading ? <Loader2 size={15} className="animate-spin" /> : <RefreshCw size={15} />}
        </Button>
      </div>

      {error && <p role="alert" className="border-b border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>}

      <div className="min-h-0 flex-1 overflow-y-auto" aria-live="polite">
        {!isLoading && annotations.length === 0 && <p className="px-3 py-10 text-center text-sm text-muted-foreground">暂无匹配批注</p>}
        {annotations.map((annotation) => {
          const draft = drafts[annotation.id] ?? createDraft(annotation);
          const isPending = pendingId === annotation.id;
          const locatorStatus = getAnnotationLocatorStatus(annotation);
          return (
            <article key={annotation.id} className="border-b border-border px-3 py-3 last:border-b-0" data-testid={`annotation-${annotation.id}`}>
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                    <span>{KIND_LABELS[annotation.kind]}</span>
                    <span className={statusClass(annotation)}>{statusLabel(annotation)}</span>
                    <span>{formatAnnotationLocator(annotation)}</span>
                  </div>
                  <p className="mt-1 whitespace-pre-wrap break-words text-sm text-foreground">{annotation.source_text}</p>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Button variant="ghost" size="icon" className="h-8 w-8" title={locatorStatus === "invalid" ? "定位已失效" : "回到原文"} aria-label={`回到原文 ${annotation.source_text}`} onClick={() => onNavigateToSource(annotation)} disabled={locatorStatus === "invalid"}>
                    <LocateFixed size={15} />
                  </Button>
                  <Button variant="ghost" size="icon" className="h-8 w-8" title="转换为学习候选" aria-label={`转换为学习候选 ${annotation.source_text}`} onClick={() => void handleConvert(annotation)} disabled={isPending || Boolean(annotation.learning_item_id)}>
                    {isPending ? <Loader2 size={15} className="animate-spin" /> : <BookOpenCheck size={15} />}
                  </Button>
                  <Button variant="ghost" size="icon" className="h-8 w-8 text-destructive" title="删除批注" aria-label={`删除批注 ${annotation.source_text}`} onClick={() => void handleDelete(annotation)} disabled={isPending}>
                    <Trash2 size={15} />
                  </Button>
                </div>
              </div>

              <div className="mt-3 grid gap-2 sm:grid-cols-[40px_minmax(0,1fr)]">
                <input aria-label={`${annotation.source_text} 颜色`} type="color" value={draft.color || "#facc15"} onChange={(event) => updateDraft(annotation.id, { color: event.target.value })} className="h-9 w-full cursor-pointer rounded border border-input bg-background p-0.5" />
                <Input aria-label={`${annotation.source_text} 标签`} value={draft.tagsInput} onChange={(event) => updateDraft(annotation.id, { tagsInput: event.target.value })} placeholder="标签，使用逗号分隔" className="h-9" />
              </div>
              <Textarea aria-label={`${annotation.source_text} 笔记`} value={draft.note ?? ""} onChange={(event) => updateDraft(annotation.id, { note: event.target.value })} placeholder="添加批注笔记" className="mt-2 min-h-16 text-sm" />
              <div className="mt-2 flex justify-end">
                <Button variant="outline" size="icon" className="h-8 w-8" title="保存批注" aria-label={`保存批注 ${annotation.source_text}`} onClick={() => void handleSave(annotation)} disabled={isPending}>
                  {isPending ? <Loader2 size={15} className="animate-spin" /> : <Check size={15} />}
                </Button>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}

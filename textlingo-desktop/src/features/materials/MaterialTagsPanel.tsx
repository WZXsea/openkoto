import { Check, Loader2, Merge, Pencil, Plus, Tag, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";

import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import type { ManagedMaterialTag, MaterialTagBulkMode, MaterialTagsApi } from "./materialManagement";

interface PendingConfirmation {
  title: string;
  description: string;
  confirmLabel: string;
  destructive?: boolean;
  run: () => Promise<void>;
}

interface MaterialTagsPanelProps {
  tags: ManagedMaterialTag[];
  selectedMaterialIds: string[];
  api: MaterialTagsApi;
  isLoading?: boolean;
  error?: string | null;
  onRetry?: () => void;
}

const bulkModeLabels: Record<MaterialTagBulkMode, string> = {
  add: "添加标签",
  remove: "移除标签",
  replace: "替换全部标签",
};

function normalizeName(value: string): string {
  return value.trim().replace(/\s+/g, " ");
}

export function MaterialTagsPanel({
  tags,
  selectedMaterialIds,
  api,
  isLoading = false,
  error,
  onRetry,
}: MaterialTagsPanelProps) {
  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState("#2563eb");
  const [editingTagId, setEditingTagId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [editingColor, setEditingColor] = useState("");
  const [mergeTargets, setMergeTargets] = useState<Record<string, string>>({});
  const [bulkMode, setBulkMode] = useState<MaterialTagBulkMode>("add");
  const [bulkTagIds, setBulkTagIds] = useState<string[]>([]);
  const [pending, setPending] = useState<PendingConfirmation | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const sortedTags = useMemo(() => [...tags].sort((left, right) => left.name.localeCompare(right.name)), [tags]);

  const run = async (operation: () => Promise<void>) => {
    setIsSubmitting(true);
    setActionError(null);
    try {
      await operation();
    } catch (caught) {
      setActionError(caught instanceof Error ? caught.message : "标签操作未完成");
    } finally {
      setIsSubmitting(false);
    }
  };

  const createTag = async () => {
    const name = normalizeName(newName);
    if (!name) {
      setActionError("请输入标签名称");
      return;
    }
    await run(async () => {
      await api.createTag({ name, color: newColor });
      setNewName("");
    });
  };

  const saveRename = async (tag: ManagedMaterialTag) => {
    const name = normalizeName(editingName);
    if (!name) {
      setActionError("请输入标签名称");
      return;
    }
    await run(async () => {
      await api.renameTag({ tagId: tag.id, name, color: editingColor });
      setEditingTagId(null);
    });
  };

  const toggleBulkTag = (tagId: string) => setBulkTagIds((current) => current.includes(tagId)
    ? current.filter((id) => id !== tagId)
    : [...current, tagId]);

  const requestBulkApply = () => {
    if (selectedMaterialIds.length === 0) {
      setActionError("请先选择需要标记的素材");
      return;
    }
    if (bulkTagIds.length === 0) {
      setActionError("请至少选择一个标签");
      return;
    }
    setPending({
      title: bulkModeLabels[bulkMode],
      description: `将对 ${selectedMaterialIds.length} 项素材执行${bulkModeLabels[bulkMode]}。`,
      confirmLabel: "确认应用",
      run: async () => {
        await api.applyTags({ materialIds: selectedMaterialIds, tagIds: bulkTagIds, mode: bulkMode });
        setBulkTagIds([]);
      },
    });
  };

  const confirmPending = async () => {
    if (!pending) return;
    const operation = pending.run;
    await run(async () => {
      await operation();
      setPending(null);
    });
  };

  return (
    <section className="min-w-0 space-y-4" aria-label="标签管理" aria-busy={isLoading || isSubmitting}>
      <header className="flex flex-wrap items-start justify-between gap-2 border-b border-border pb-3">
        <div>
          <h2 className="flex items-center gap-2 text-sm font-semibold"><Tag size={16} />标签管理</h2>
          <p className="mt-1 text-xs text-muted-foreground">{tags.length} 个标签，已选择 {selectedMaterialIds.length} 项素材</p>
        </div>
      </header>

      {(error || actionError) && (
        <div className="flex flex-wrap items-center gap-2 text-sm text-destructive" role="alert">
          <span>{actionError || error}</span>
          {error && onRetry && <Button type="button" variant="ghost" size="sm" onClick={onRetry}>重试</Button>}
        </div>
      )}

      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto_auto]">
        <Input aria-label="新标签名称" value={newName} onChange={(event) => setNewName(event.target.value)} placeholder="新标签名称" disabled={isLoading || isSubmitting} />
        <input aria-label="新标签颜色" type="color" value={newColor} onChange={(event) => setNewColor(event.target.value)} className="h-10 w-full cursor-pointer rounded-lg border border-input bg-background p-1 sm:w-12" disabled={isLoading || isSubmitting} />
        <Button type="button" size="sm" onClick={createTag} disabled={isLoading || isSubmitting}><Plus size={15} className="mr-1.5" />创建</Button>
      </div>

      <div className="overflow-x-auto border-y border-border">
        <div className="min-w-[420px] divide-y divide-border">
          {sortedTags.map((tag) => {
            const isEditing = editingTagId === tag.id;
            const targetId = mergeTargets[tag.id] || "";
            return (
              <div key={tag.id} className="grid grid-cols-[auto_minmax(120px,1fr)_80px_minmax(118px,0.8fr)_auto] items-center gap-2 px-1 py-2">
                <label className="flex h-8 w-8 cursor-pointer items-center justify-center" title={`选择 ${tag.name} 用于批量操作`}>
                  <input aria-label={`批量选择 ${tag.name}`} type="checkbox" checked={bulkTagIds.includes(tag.id)} onChange={() => toggleBulkTag(tag.id)} disabled={isLoading || isSubmitting} />
                </label>
                {isEditing ? (
                  <Input aria-label={`重命名 ${tag.name}`} value={editingName} onChange={(event) => setEditingName(event.target.value)} className="h-8" />
                ) : (
                  <div className="flex min-w-0 items-center gap-2">
                    <span aria-label={`${tag.name} 颜色`} className="h-3 w-3 shrink-0 rounded-sm border border-black/10" style={{ backgroundColor: tag.color }} />
                    <span className="truncate text-sm" title={tag.name}>{tag.name}</span>
                  </div>
                )}
                {isEditing ? (
                  <input aria-label={`${tag.name} 颜色`} type="color" value={editingColor} onChange={(event) => setEditingColor(event.target.value)} className="h-8 w-10 cursor-pointer rounded border border-input bg-background p-0.5" />
                ) : <span className="text-xs tabular-nums text-muted-foreground">{tag.materialCount ?? 0} 项</span>}
                <select aria-label={`合并 ${tag.name} 到`} value={targetId} onChange={(event) => setMergeTargets((current) => ({ ...current, [tag.id]: event.target.value }))} className="h-8 min-w-0 rounded-lg border border-input bg-background px-2 text-xs" disabled={isEditing || isLoading || isSubmitting}>
                  <option value="">合并到...</option>
                  {sortedTags.filter((candidate) => candidate.id !== tag.id).map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.name}</option>)}
                </select>
                <div className="flex items-center gap-1">
                  {isEditing ? (
                    <>
                      <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={`保存 ${tag.name}`} title="保存" onClick={() => saveRename(tag)} disabled={isLoading || isSubmitting}><Check size={15} /></Button>
                      <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={`取消编辑 ${tag.name}`} title="取消" onClick={() => setEditingTagId(null)} disabled={isSubmitting}><X size={15} /></Button>
                    </>
                  ) : (
                    <>
                      <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={`重命名 ${tag.name}`} title="重命名" onClick={() => { setEditingTagId(tag.id); setEditingName(tag.name); setEditingColor(tag.color); }} disabled={isLoading || isSubmitting}><Pencil size={15} /></Button>
                      <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={`合并 ${tag.name}`} title="合并" disabled={!targetId || isLoading || isSubmitting} onClick={() => setPending({
                        title: "合并标签",
                        description: `“${tag.name}”将合并到所选标签，原标签将被删除。`,
                        confirmLabel: "确认合并",
                        destructive: true,
                        run: () => api.mergeTags({ sourceTagId: tag.id, targetTagId: targetId }),
                      })}><Merge size={15} /></Button>
                      <Button type="button" variant="ghost" size="icon" className="h-8 w-8 text-destructive hover:text-destructive" aria-label={`删除 ${tag.name}`} title="删除" disabled={isLoading || isSubmitting} onClick={() => setPending({
                        title: "删除标签",
                        description: `删除“${tag.name}”后，该标签会从关联素材移除。`,
                        confirmLabel: "确认删除",
                        destructive: true,
                        run: () => api.deleteTag(tag.id),
                      })}><Trash2 size={15} /></Button>
                    </>
                  )}
                </div>
              </div>
            );
          })}
          {!isLoading && sortedTags.length === 0 && <p className="px-2 py-6 text-center text-sm text-muted-foreground">暂无标签</p>}
          {isLoading && <p className="flex items-center justify-center gap-2 px-2 py-6 text-sm text-muted-foreground"><Loader2 size={16} className="animate-spin" />正在加载标签</p>}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 border-b border-border pb-4">
        <select aria-label="批量标签操作" value={bulkMode} onChange={(event) => setBulkMode(event.target.value as MaterialTagBulkMode)} className="h-8 rounded-lg border border-input bg-background px-2 text-sm" disabled={isLoading || isSubmitting}>
          {(Object.keys(bulkModeLabels) as MaterialTagBulkMode[]).map((mode) => <option key={mode} value={mode}>{bulkModeLabels[mode]}</option>)}
        </select>
        <Button type="button" variant="outline" size="sm" onClick={requestBulkApply} disabled={isLoading || isSubmitting || selectedMaterialIds.length === 0}>应用到 {selectedMaterialIds.length} 项</Button>
      </div>

      {pending && (
        <div className="flex flex-wrap items-center gap-2 border border-border bg-muted/30 px-3 py-2" role="alertdialog" aria-label={pending.title}>
          <div className="min-w-0 flex-1"><p className="text-sm font-medium">{pending.title}</p><p className="text-xs text-muted-foreground">{pending.description}</p></div>
          <Button type="button" variant="ghost" size="sm" onClick={() => setPending(null)} disabled={isSubmitting}>取消</Button>
          <Button type="button" variant={pending.destructive ? "danger" : "default"} size="sm" onClick={confirmPending} disabled={isSubmitting}>{isSubmitting && <Loader2 size={14} className="mr-1.5 animate-spin" />}{pending.confirmLabel}</Button>
        </div>
      )}
    </section>
  );
}

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Archive, Check, Loader2, Plus, RefreshCw, Sparkles, Trash2, X } from "lucide-react";

import {
  createLearningItemFromSelection,
  deleteLearningItem,
  listLearningItems,
  updateLearningItem,
} from "../../lib/learningItems";
import type { Article, LearningItem, SegmentExplanation, VocabularyItem } from "../../types";
import { Button } from "../ui/button";
import { Textarea } from "../ui/textarea";
import { SelectPackDialog } from "./SelectPackDialog";

export interface ReaderSelectionContext {
  materialId: string;
  segmentId?: string;
  selectedText: string;
  sourceSentence: string;
  contextBefore?: string;
  contextAfter?: string;
}

interface LearningCandidateBoxProps {
  article: Article;
  selection: ReaderSelectionContext | null;
  open: boolean;
  canUseAi: boolean;
  targetLanguage: string;
  onOpenChange: (open: boolean) => void;
  onError: (message: string) => void;
  onSuccess: (message: string) => void;
}

type LearningItemFilter = "candidate" | "accepted" | "all";

export function LearningCandidateBox({
  article,
  selection,
  open,
  canUseAi,
  targetLanguage,
  onOpenChange,
  onError,
  onSuccess,
}: LearningCandidateBoxProps) {
  const [items, setItems] = useState<LearningItem[]>([]);
  const [filter, setFilter] = useState<LearningItemFilter>("candidate");
  const [isLoading, setIsLoading] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [savingItemId, setSavingItemId] = useState<string | null>(null);
  const [aiItemId, setAiItemId] = useState<string | null>(null);
  const [pendingPackItem, setPendingPackItem] = useState<LearningItem | null>(null);

  const visibleItems = useMemo(() => {
    if (filter === "all") return items;
    return items.filter((item) => item.status === filter);
  }, [filter, items]);

  const loadItems = async () => {
    setIsLoading(true);
    try {
      const nextItems = await listLearningItems({
        material_id: article.id,
        limit: 200,
      });
      setItems(nextItems);
    } catch (error) {
      onError(String(error));
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    void loadItems();
  }, [article.id, open]);

  if (!open) return null;

  const upsertLocalItem = (item: LearningItem) => {
    setItems((prev) => {
      const index = prev.findIndex((existing) => existing.id === item.id);
      if (index === -1) return [item, ...prev];
      const next = [...prev];
      next[index] = item;
      return next;
    });
  };

  const removeLocalItem = (id: string) => {
    setItems((prev) => prev.filter((item) => item.id !== id));
  };

  const handleCreateFromSelection = async () => {
    if (!selection) return;
    setIsCreating(true);
    try {
      const created = await createLearningItemFromSelection({
        material_id: selection.materialId,
        segment_id: selection.segmentId ?? null,
        selected_text: selection.selectedText,
        source_sentence: selection.sourceSentence,
        context_before: selection.contextBefore ?? null,
        context_after: selection.contextAfter ?? null,
        tags: ["reader"],
      });
      upsertLocalItem(created);
      setFilter("candidate");
      onOpenChange(true);
      onSuccess("已加入候选箱");
    } catch (error) {
      onError(String(error));
    } finally {
      setIsCreating(false);
    }
  };

  const handlePatchItem = async (item: LearningItem, patch: Record<string, unknown>) => {
    setSavingItemId(item.id);
    try {
      const updated = await updateLearningItem(item.id, patch);
      upsertLocalItem(updated);
      return updated;
    } catch (error) {
      onError(String(error));
      return null;
    } finally {
      setSavingItemId(null);
    }
  };

  const handleDraftChange = (itemId: string, patch: Partial<LearningItem>) => {
    setItems((prev) => prev.map((item) => (item.id === itemId ? { ...item, ...patch } : item)));
  };

  const handleAiFill = async (item: LearningItem) => {
    if (!canUseAi) {
      onError("需要先配置 AI 模型");
      return;
    }

    setAiItemId(item.id);
    try {
      const explanation = await invoke<SegmentExplanation>("segment_translate_explain_cmd", {
        text: item.source_sentence || item.text,
        targetLanguage,
      });
      const matched = findMatchingVocabulary(explanation.vocabulary || [], item.text);
      const meaning = matched?.meaning || explanation.translation || explanation.explanation;
      const example = matched?.example || item.source_sentence;
      const updated = await handlePatchItem(item, {
        meaning_in_context: meaning,
        definition_zh: matched?.meaning || explanation.translation || null,
        definition_en: explanation.explanation || null,
        examples: example ? [{ text: example }] : [],
        ai_explanation: explanation,
      });
      if (updated) onSuccess("已用 AI 补全候选项");
    } catch (error) {
      onError(String(error));
    } finally {
      setAiItemId(null);
    }
  };

  const handleAcceptItem = async (item: LearningItem, packIds: string[]) => {
    setSavingItemId(item.id);
    try {
      if (item.item_type === "grammar") {
        await invoke("add_favorite_grammar_cmd", {
          point: item.text,
          explanation: item.meaning_in_context || item.definition_zh || item.definition_en || item.source_sentence || item.text,
          example: item.source_sentence || null,
          sourceArticleId: item.material_id || article.id,
          sourceArticleTitle: item.source_material_title || article.title,
        });
      } else {
        await invoke("add_favorite_vocabulary_cmd", {
          word: item.text,
          meaning: item.meaning_in_context || item.definition_zh || item.definition_en || item.text,
          usage: "",
          explanation: item.definition_en || item.meaning_in_context || null,
          example: firstExampleText(item) || item.source_sentence || null,
          reading: null,
          sourceArticleId: item.material_id || article.id,
          sourceArticleTitle: item.source_material_title || article.title,
          packIds,
        });
      }

      const updated = await updateLearningItem(item.id, { status: "accepted" });
      upsertLocalItem(updated);
      setFilter("accepted");
      onSuccess("已加入本地词包");
    } catch (error) {
      onError(String(error));
    } finally {
      setPendingPackItem(null);
      setSavingItemId(null);
    }
  };

  const handleAcceptWithPack = async (packIds: string[]) => {
    if (!pendingPackItem) return;
    await handleAcceptItem(pendingPackItem, packIds);
  };

  const handleDelete = async (item: LearningItem) => {
    setSavingItemId(item.id);
    try {
      await deleteLearningItem(item.id);
      removeLocalItem(item.id);
      onSuccess("候选项已删除");
    } catch (error) {
      onError(String(error));
    } finally {
      setSavingItemId(null);
    }
  };

  return (
    <>
      <section
        className="absolute bottom-4 right-4 z-30 flex max-h-[72%] w-[min(430px,calc(100%-2rem))] flex-col rounded-lg border border-border bg-background shadow-xl"
        aria-label="学习候选箱"
        data-testid="learning-candidate-box"
      >
        <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
          <div className="min-w-0">
            <h3 className="truncate text-sm font-semibold text-foreground">候选箱</h3>
            <p className="truncate text-xs text-muted-foreground">{items.length} 项</p>
          </div>
          <div className="flex items-center gap-1">
            <Button variant="ghost" size="sm" className="h-8 w-8 p-0" onClick={() => void loadItems()} title="刷新">
              {isLoading ? <Loader2 size={15} className="animate-spin" /> : <RefreshCw size={15} />}
            </Button>
            <Button variant="ghost" size="sm" className="h-8 w-8 p-0" onClick={() => onOpenChange(false)} title="关闭">
              <X size={16} />
            </Button>
          </div>
        </div>

        {selection && (
          <div className="border-b border-border bg-muted/30 px-3 py-2">
            <p className="line-clamp-2 text-sm text-foreground">{selection.selectedText}</p>
            <div className="mt-2 flex justify-end">
              <Button size="sm" className="h-8 gap-2" onClick={() => void handleCreateFromSelection()} disabled={isCreating}>
                {isCreating ? <Loader2 size={14} className="animate-spin" /> : <Plus size={14} />}
                加入候选
              </Button>
            </div>
          </div>
        )}

        <div className="flex gap-1 border-b border-border px-3 py-2">
          {(["candidate", "accepted", "all"] as const).map((value) => (
            <Button
              key={value}
              variant={filter === value ? "secondary" : "ghost"}
              size="sm"
              className="h-7 flex-1 px-2 text-xs"
              onClick={() => setFilter(value)}
            >
              {value === "candidate" ? "候选" : value === "accepted" ? "已接受" : "全部"}
            </Button>
          ))}
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-3">
          {visibleItems.length === 0 ? (
            <div className="py-8 text-center text-sm text-muted-foreground">暂无候选项</div>
          ) : (
            <div className="space-y-3">
              {visibleItems.map((item) => (
                <article key={item.id} className="rounded-lg border border-border bg-card p-3">
                  <div className="mb-2 flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <p className="break-words text-sm font-medium text-foreground">{item.text}</p>
                      <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{item.source_sentence}</p>
                    </div>
                    <span className="shrink-0 rounded-md bg-muted px-2 py-1 text-[11px] text-muted-foreground">
                      {item.item_type}
                    </span>
                  </div>

                  <Textarea
                    value={item.meaning_in_context || ""}
                    onChange={(event) => handleDraftChange(item.id, { meaning_in_context: event.target.value })}
                    placeholder="语境含义"
                    className="min-h-[58px] text-sm"
                  />

                  <div className="mt-2 grid grid-cols-2 gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-8 gap-2"
                      onClick={() => void handleAiFill(item)}
                      disabled={!canUseAi || aiItemId === item.id || savingItemId === item.id}
                    >
                      {aiItemId === item.id ? <Loader2 size={14} className="animate-spin" /> : <Sparkles size={14} />}
                      AI 补全
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-8 gap-2"
                      onClick={() => void handlePatchItem(item, { meaning_in_context: item.meaning_in_context || "" })}
                      disabled={savingItemId === item.id}
                    >
                      {savingItemId === item.id ? <Loader2 size={14} className="animate-spin" /> : <Check size={14} />}
                      保存
                    </Button>
                  </div>

                  <div className="mt-2 grid grid-cols-3 gap-2">
                    <Button
                      size="sm"
                      className="h-8 gap-1"
                      onClick={() => {
                        if (item.item_type === "grammar") {
                          void handleAcceptItem(item, []);
                        } else {
                          setPendingPackItem(item);
                        }
                      }}
                      disabled={savingItemId === item.id}
                    >
                      <Check size={14} />
                      接受
                    </Button>
                    <Button
                      variant="secondary"
                      size="sm"
                      className="h-8 gap-1"
                      onClick={() => void handlePatchItem(item, { status: item.status === "archived" ? "candidate" : "archived" })}
                      disabled={savingItemId === item.id}
                    >
                      <Archive size={14} />
                      归档
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-8 gap-1 text-destructive hover:text-destructive"
                      onClick={() => void handleDelete(item)}
                      disabled={savingItemId === item.id}
                    >
                      <Trash2 size={14} />
                      删除
                    </Button>
                  </div>
                </article>
              ))}
            </div>
          )}
        </div>
      </section>

      <SelectPackDialog
        open={Boolean(pendingPackItem && pendingPackItem.item_type !== "grammar")}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setPendingPackItem(null);
        }}
        onConfirm={handleAcceptWithPack}
      />
    </>
  );
}

function findMatchingVocabulary(vocabulary: VocabularyItem[], text: string) {
  const normalizedText = normalize(text);
  return vocabulary.find((item) => normalize(item.word) === normalizedText)
    || vocabulary.find((item) => normalizedText.includes(normalize(item.word)));
}

function normalize(value: string) {
  return value.trim().toLowerCase();
}

function firstExampleText(item: LearningItem) {
  for (const example of item.examples || []) {
    if (typeof example === "string") return example;
    if (example && typeof example === "object" && "text" in example) {
      const text = (example as { text?: unknown }).text;
      if (typeof text === "string" && text.trim()) return text;
    }
  }
  return null;
}

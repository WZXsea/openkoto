import { Extension, type Editor } from "@tiptap/core";
import Placeholder from "@tiptap/extension-placeholder";
import UniqueID from "@tiptap/extension-unique-id";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import {
  AlertTriangle,
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Bold,
  CheckCircle2,
  ChevronDown,
  Cloud,
  CloudOff,
  Code2,
  FileClock,
  FileCode2,
  Heading2,
  Heading3,
  History,
  Italic,
  List,
  ListOrdered,
  Link2,
  Loader2,
  Minus,
  Pilcrow,
  Plus,
  Quote,
  Redo2,
  Save,
  Undo2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "../../components/ui/button";
import { Dialog, DialogFooter } from "../../components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../../components/ui/dropdown-menu";
import {
  blocksToTiptapDocument,
  MATERIAL_BLOCK_NODE_TYPES,
  markdownToMaterialBlocks,
  materialBlocksForPersistence,
  materialBlocksFingerprint,
  materialBlocksToMarkdown,
  normalizeMaterialBlocks,
  tiptapDocumentToBlocks,
} from "./documentModel";
import { isMaterialPreviewChanged, isMaterialRevisionConflict, materialEditorApi } from "./materialEditorApi";
import {
  requiresMaterialEditConfirmation,
  type MaterialDocument,
  type MaterialDraft,
  type MaterialEditImpact,
  type MaterialEditorBlock,
  type MaterialEditorPhase,
  type MaterialRevisionDetail,
  type MaterialRevisionSummary,
} from "./types";
import { PointerBlockDragHandle } from "./PointerBlockDragHandle";
import { reorderEditorBlock } from "./reorderMaterialBlock";

const AUTOSAVE_DELAY_MS = 2000;

const MaterialBlockMetadata = Extension.create({
  name: "materialBlockMetadata",
  addGlobalAttributes() {
    return [{
      types: [...MATERIAL_BLOCK_NODE_TYPES],
      attributes: {
        materialAttrs: {
          default: {},
          rendered: false,
        },
        materialDomId: {
          default: null,
          parseHTML: (element: HTMLElement) => element.getAttribute("data-block-id"),
          renderHTML: (attributes: Record<string, unknown>) => typeof attributes.blockId === "string"
            ? { "data-block-id": attributes.blockId }
            : {},
        },
      },
    }];
  },
});

const editorExtensions = [
  StarterKit.configure({
    heading: { levels: [2, 3] },
    codeBlock: false,
    trailingNode: false,
    link: { openOnClick: false, autolink: true, linkOnPaste: true },
  }),
  MaterialBlockMetadata,
  UniqueID.configure({
    attributeName: "blockId",
    types: [...MATERIAL_BLOCK_NODE_TYPES],
    generateID: () => typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
      ? crypto.randomUUID()
      : `block-${Date.now()}-${Math.random().toString(16).slice(2)}`,
  }),
  Placeholder.configure({
    placeholder: "输入外文正文，或输入 / 选择块类型",
  }),
];

type DraftSaveStatus = "idle" | "saving" | "saved" | "failed";

interface MaterialDocumentEditorProps {
  materialId: string;
  title: string;
  onCancel: () => void;
  onCommitted?: (document: MaterialDocument) => void;
}

interface EditorCanvasProps {
  blocks: MaterialEditorBlock[];
  disabled: boolean;
  onBlocksChange: (blocks: MaterialEditorBlock[]) => void;
  onSave: () => void;
}

function moveCurrentBlock(editor: Editor, direction: -1 | 1): boolean {
  const blocks = tiptapDocumentToBlocks(editor.getJSON());
  const validIds = new Set(blocks.flatMap((block) => block.id ? [block.id] : []));
  const { $from } = editor.state.selection;
  let sourceId: string | null = null;

  for (let depth = $from.depth; depth > 0; depth -= 1) {
    const blockId = $from.node(depth).attrs.blockId;
    if (typeof blockId === "string" && validIds.has(blockId)) {
      sourceId = blockId;
      break;
    }
  }

  if (!sourceId) {
    for (const node of [$from.nodeAfter, $from.nodeBefore]) {
      const blockId = node?.attrs.blockId;
      if (typeof blockId === "string" && validIds.has(blockId)) {
        sourceId = blockId;
        break;
      }
    }
  }

  if (!sourceId) return false;
  const index = blocks.findIndex((block) => block.id === sourceId);
  const target = blocks[index + direction];
  if (index < 0 || !target?.id) return false;
  return reorderEditorBlock(editor, sourceId, target.id, direction < 0 ? "before" : "after");
}

function editLink(editor: Editor): void {
  const existing = editor.getAttributes("link").href as string | undefined;
  const href = window.prompt("输入链接地址；留空可移除链接", existing ?? "https://");
  if (href === null) return;
  if (href.trim() === "") {
    editor.chain().focus().extendMarkRange("link").unsetLink().run();
    return;
  }
  editor.chain().focus().extendMarkRange("link").setLink({ href: href.trim() }).run();
}

type EditorBlockKind = "paragraph" | "heading_2" | "heading_3" | "bullet_list" | "ordered_list" | "quote" | "divider";

function applyBlockType(editor: Editor, type: EditorBlockKind): void {
  const chain = editor.chain().focus();
  if (type === "paragraph") chain.setParagraph().run();
  else if (type === "heading_2") chain.toggleHeading({ level: 2 }).run();
  else if (type === "heading_3") chain.toggleHeading({ level: 3 }).run();
  else if (type === "bullet_list") chain.toggleBulletList().run();
  else if (type === "ordered_list") chain.toggleOrderedList().run();
  else if (type === "quote") chain.toggleBlockquote().run();
  else chain.setHorizontalRule().run();
}

const blockTypeOptions: Array<{
  type: EditorBlockKind;
  label: string;
  icon: typeof Pilcrow;
}> = [
  { type: "paragraph", label: "正文", icon: Pilcrow },
  { type: "heading_2", label: "二级标题", icon: Heading2 },
  { type: "heading_3", label: "三级标题", icon: Heading3 },
  { type: "bullet_list", label: "无序列表", icon: List },
  { type: "ordered_list", label: "有序列表", icon: ListOrdered },
  { type: "quote", label: "引用", icon: Quote },
  { type: "divider", label: "分隔线", icon: Minus },
];

function MaterialEditorCanvas({ blocks, disabled, onBlocksChange, onSave }: EditorCanvasProps) {
  const [slashOpen, setSlashOpen] = useState(false);
  const editor = useEditor({
    extensions: editorExtensions,
    content: blocksToTiptapDocument(blocks),
    editable: !disabled,
    immediatelyRender: false,
    shouldRerenderOnTransaction: true,
    onUpdate: ({ editor: currentEditor }) => {
      onBlocksChange(tiptapDocumentToBlocks(currentEditor.getJSON()));
    },
  });

  useEffect(() => {
    editor?.setEditable(!disabled);
  }, [disabled, editor]);

  if (!editor) {
    return <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground" role="status">正在初始化编辑器</div>;
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const modifier = event.metaKey || event.ctrlKey;
    if (modifier && event.key.toLowerCase() === "s") {
      event.preventDefault();
      onSave();
      return;
    }
    if (modifier && event.key.toLowerCase() === "k") {
      event.preventDefault();
      editLink(editor);
      return;
    }
    if (event.altKey && event.key === "ArrowUp") {
      event.preventDefault();
      moveCurrentBlock(editor, -1);
      return;
    }
    if (event.altKey && event.key === "ArrowDown") {
      event.preventDefault();
      moveCurrentBlock(editor, 1);
      return;
    }
    if (event.key === "/" && editor.state.selection.$from.parent.textContent.length === 0) {
      event.preventDefault();
      setSlashOpen(true);
      return;
    }
    if (event.key === "Escape" && slashOpen) {
      event.preventDefault();
      setSlashOpen(false);
    }
  };

  return (
    <div className="relative flex min-h-0 flex-1 flex-col" onKeyDownCapture={handleKeyDown}>
      <div className="sticky top-0 z-20 flex flex-wrap items-center gap-1 border-b border-border bg-background/95 px-3 py-2 backdrop-blur-sm sm:px-5" aria-label="正文格式工具栏">
        <Button size="sm" variant="ghost" onClick={() => editor.chain().focus().undo().run()} disabled={!editor.can().undo()} aria-label="撤销" title="撤销 (⌘Z)"><Undo2 size={16} /></Button>
        <Button size="sm" variant="ghost" onClick={() => editor.chain().focus().redo().run()} disabled={!editor.can().redo()} aria-label="重做" title="重做 (⇧⌘Z)"><Redo2 size={16} /></Button>
        <span className="mx-1 h-5 w-px bg-border" />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="sm" variant="secondary" className="gap-1.5" aria-label="选择块类型"><Pilcrow size={16} /><span className="hidden sm:inline">块类型</span><ChevronDown size={13} /></Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {blockTypeOptions.map(({ type, label, icon: Icon }) => (
              <DropdownMenuItem key={type} onClick={() => applyBlockType(editor, type)}><Icon size={15} />{label}</DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <Button size="sm" variant={editor.isActive("bold") ? "default" : "ghost"} onClick={() => editor.chain().focus().toggleBold().run()} aria-label="粗体"><Bold size={16} /></Button>
        <Button size="sm" variant={editor.isActive("italic") ? "default" : "ghost"} onClick={() => editor.chain().focus().toggleItalic().run()} aria-label="斜体"><Italic size={16} /></Button>
        <Button size="sm" variant={editor.isActive("code") ? "default" : "ghost"} onClick={() => editor.chain().focus().toggleCode().run()} aria-label="行内代码"><Code2 size={16} /></Button>
        <Button size="sm" variant={editor.isActive("link") ? "default" : "ghost"} onClick={() => editLink(editor)} aria-label="编辑链接" title="编辑链接 (⌘K)"><Link2 size={16} /></Button>
        <span className="mx-1 h-5 w-px bg-border" />
        <Button size="sm" variant="ghost" onClick={() => moveCurrentBlock(editor, -1)} aria-label="上移当前块" title="上移当前块 (⌥↑)"><ArrowUp size={16} /></Button>
        <Button size="sm" variant="ghost" onClick={() => moveCurrentBlock(editor, 1)} aria-label="下移当前块" title="下移当前块 (⌥↓)"><ArrowDown size={16} /></Button>
        <Button
          size="sm"
          variant="ghost"
          className="gap-1.5"
          onClick={() => editor.chain().focus().insertContent({ type: "paragraph" }).run()}
        >
          <Plus size={16} /><span className="hidden sm:inline">添加段落</span>
        </Button>
      </div>

      <div className="relative flex-1 overflow-y-auto px-4 py-8 sm:px-8" data-testid="material-block-editor-scroll">
        <PointerBlockDragHandle
          editor={editor}
          validBlockIds={tiptapDocumentToBlocks(editor.getJSON()).flatMap((block) => block.id ? [block.id] : [])}
          disabled={disabled}
        />
        {slashOpen && (
          <div className="absolute left-1/2 top-5 z-30 grid w-[min(320px,calc(100%-2rem))] -translate-x-1/2 gap-1 rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-xl" role="menu" aria-label="插入块">
            <p className="px-2 py-1 text-xs text-muted-foreground">选择块类型</p>
            {blockTypeOptions.map(({ type, label, icon: Icon }) => (
              <button
                key={type}
                type="button"
                role="menuitem"
                className="flex items-center gap-2 rounded-lg px-2 py-2 text-left text-sm hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                onClick={() => { applyBlockType(editor, type); setSlashOpen(false); }}
              >
                <Icon size={16} />{label}
              </button>
            ))}
          </div>
        )}
        <EditorContent editor={editor} className="openkoto-material-editor openkoto-reader-font mx-auto max-w-3xl" data-testid="material-block-editor" />
      </div>
    </div>
  );
}

function ImpactGrid({ impact }: { impact: MaterialEditImpact }) {
  const items = [
    ["新增块", impact.inserted_blocks],
    ["修改块", impact.updated_blocks],
    ["删除块", impact.deleted_blocks],
    ["移动块", impact.moved_blocks],
    ["翻译待更新", impact.stale_translations],
    ["精讲待更新", impact.stale_explanations],
    ["批注受影响", impact.affected_annotations],
    ["学习项受影响", impact.affected_learning_items],
  ] as const;
  return (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-4" data-testid="material-edit-impact-grid">
      {items.map(([label, value]) => (
        <div key={label} className={`rounded-xl border px-3 py-2 ${value > 0 ? "border-amber-500/30 bg-amber-500/10" : "border-border bg-muted/30"}`}>
          <p className="text-[11px] text-muted-foreground">{label}</p>
          <p className="mt-1 text-lg font-semibold">{value}</p>
        </div>
      ))}
    </div>
  );
}

function extractRevisionBlocks(detail: MaterialRevisionDetail | null): MaterialEditorBlock[] {
  if (!detail) return [];
  const snapshot = detail.snapshot;
  return Array.isArray(snapshot.blocks) ? snapshot.blocks : [];
}

function revisionDiff(current: MaterialEditorBlock[], historic: MaterialEditorBlock[]) {
  const currentById = new Map(current.filter((block) => block.id).map((block) => [block.id!, block]));
  const historicById = new Map(historic.filter((block) => block.id).map((block) => [block.id!, block]));
  const removed = historic.filter((block) => block.id && !currentById.has(block.id));
  const added = current.filter((block) => block.id && !historicById.has(block.id));
  const changed = historic.filter((block) => {
    if (!block.id || !currentById.has(block.id)) return false;
    const next = currentById.get(block.id)!;
    return next.text !== block.text
      || next.block_type !== block.block_type
      || JSON.stringify(next.attrs ?? {}) !== JSON.stringify(block.attrs ?? {});
  });
  const moved = historic.filter((block) => block.id && currentById.has(block.id) && currentById.get(block.id)?.block_order !== block.block_order);
  return { removed, added, changed, moved };
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error && typeof (error as { message?: unknown }).message === "string") {
    return (error as { message: string }).message;
  }
  return error instanceof Error ? error.message : String(error);
}

function revisionLabel(revision: Pick<MaterialRevisionSummary, "action" | "change_summary">): string {
  const rawSummary: unknown = revision.change_summary;
  if (typeof rawSummary === "string" && rawSummary.trim()) return rawSummary;
  if (rawSummary && typeof rawSummary === "object") {
    const summary = rawSummary as Record<string, unknown>;
    const parts = [
      ["新增", summary.inserted_blocks],
      ["修改", summary.updated_blocks],
      ["删除", summary.deleted_blocks],
      ["移动", summary.moved_blocks],
    ].filter((entry): entry is [string, number] => typeof entry[1] === "number" && entry[1] > 0)
      .map(([label, value]) => `${label} ${value}`);
    if (parts.length > 0) return parts.join(" · ");
  }
  if (revision.action === "migration") return "初始版本";
  if (revision.action === "restore") return "恢复历史版本";
  return "编辑正文";
}

export function MaterialDocumentEditor({ materialId, title, onCancel, onCommitted }: MaterialDocumentEditorProps) {
  const [phase, setPhase] = useState<MaterialEditorPhase>("loading");
  const [document, setDocument] = useState<MaterialDocument | null>(null);
  const [baselineBlocks, setBaselineBlocks] = useState<MaterialEditorBlock[]>([]);
  const [blocks, setBlocks] = useState<MaterialEditorBlock[]>([]);
  const [editorKey, setEditorKey] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [previewImpact, setPreviewImpact] = useState<MaterialEditImpact | null>(null);
  const [discardOpen, setDiscardOpen] = useState(false);
  const [impactOpen, setImpactOpen] = useState(false);
  const [draft, setDraft] = useState<MaterialDraft | null>(null);
  const [draftStatus, setDraftStatus] = useState<DraftSaveStatus>("idle");
  const [historyOpen, setHistoryOpen] = useState(false);
  const [revisions, setRevisions] = useState<MaterialRevisionSummary[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [selectedRevision, setSelectedRevision] = useState<MaterialRevisionDetail | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<MaterialRevisionSummary | null>(null);
  const [restoreDirtyTarget, setRestoreDirtyTarget] = useState<MaterialRevisionSummary | null>(null);
  const [restoreBusy, setRestoreBusy] = useState(false);
  const [sourceOpen, setSourceOpen] = useState(false);
  const [sourceText, setSourceText] = useState("");
  const [draftEpoch, setDraftEpoch] = useState(0);
  const lastDraftFingerprintRef = useRef<string | null>(null);
  const exitAfterSaveRef = useRef(false);
  const previewTokenRef = useRef<string | null>(null);
  const sourceOriginalRef = useRef("");
  const sourceOriginalBlocksRef = useRef<MaterialEditorBlock[]>([]);
  const draftEpochRef = useRef(0);
  const draftSaveQueueRef = useRef<Promise<void>>(Promise.resolve());
  const restoreAfterSaveRef = useRef<MaterialRevisionSummary | null>(null);
  const nativeCloseRequestedRef = useRef(false);
  const nativeCloseAllowedRef = useRef(false);
  const dirty = useMemo(
    () => materialBlocksFingerprint(blocks) !== materialBlocksFingerprint(baselineBlocks),
    [baselineBlocks, blocks],
  );
  const modifiedBlockCount = useMemo(() => {
    const baselineById = new Map(
      baselineBlocks
        .filter((block): block is MaterialEditorBlock & { id: string } => Boolean(block.id))
        .map((block, index) => [block.id, { block, index }]),
    );
    const currentIds = new Set(blocks.flatMap((block) => block.id ? [block.id] : []));
    const changedCurrent = blocks.filter((block, index) => {
      if (!block.id) return true;
      const baseline = baselineById.get(block.id);
      return !baseline
        || baseline.index !== index
        || materialBlocksFingerprint([baseline.block]) !== materialBlocksFingerprint([block]);
    }).length;
    const deleted = baselineBlocks.filter((block) => block.id && !currentIds.has(block.id)).length;
    return changedCurrent + deleted;
  }, [baselineBlocks, blocks]);
  const busy = phase === "loading" || phase === "previewing" || phase === "saving";

  const finishExit = useCallback(async () => {
    if (!nativeCloseRequestedRef.current) {
      onCancel();
      return;
    }
    nativeCloseRequestedRef.current = false;
    nativeCloseAllowedRef.current = true;
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().close();
    } catch {
      onCancel();
    }
  }, [onCancel]);

  const queueDraftSave = useCallback((payload: { base_revision: number; blocks: MaterialEditorBlock[] }, fingerprint?: string) => {
    const epoch = draftEpochRef.current;
    setDraftStatus("saving");
    const operation = draftSaveQueueRef.current
      .catch(() => undefined)
      .then(() => materialEditorApi.saveDraft(materialId, payload));
    draftSaveQueueRef.current = operation.then(() => undefined, () => undefined);
    return operation.then((savedDraft) => {
      if (epoch === draftEpochRef.current) {
        if (fingerprint) lastDraftFingerprintRef.current = fingerprint;
        setDraft(savedDraft);
        setDraftStatus("saved");
      }
      return savedDraft;
    }).catch((draftError) => {
      if (epoch === draftEpochRef.current) setDraftStatus("failed");
      throw draftError;
    });
  }, [materialId]);

  const deleteDraftSafely = useCallback(async (suppressFingerprint?: string) => {
    draftEpochRef.current += 1;
    setDraftEpoch(draftEpochRef.current);
    if (suppressFingerprint) lastDraftFingerprintRef.current = suppressFingerprint;
    await draftSaveQueueRef.current.catch(() => undefined);
    await materialEditorApi.deleteDraft(materialId);
    setDraft(null);
    setDraftStatus("idle");
  }, [materialId]);

  const performRestore = useCallback(async (
    target: MaterialRevisionSummary,
    baseDocument: MaterialDocument,
    preserveDraft = false,
  ) => {
    setRestoreBusy(true);
    try {
      const result = await materialEditorApi.restoreRevision(
        materialId,
        target.revision,
        baseDocument.current_revision,
        crypto.randomUUID(),
        preserveDraft,
      );
      const restored = normalizeMaterialBlocks(result.document.blocks);
      setDocument(result.document);
      setBaselineBlocks(restored);
      setBlocks(restored);
      setEditorKey((current) => current + 1);
      setHistoryOpen(false);
      setRestoreTarget(null);
      setRestoreDirtyTarget(null);
      setPhase("editing");
      onCommitted?.(result.document);
      return true;
    } catch (restoreError) {
      setError(`恢复版本失败：${errorMessage(restoreError)}`);
      return false;
    } finally {
      setRestoreBusy(false);
    }
  }, [materialId, onCommitted]);

  const loadDocument = useCallback(async () => {
    setPhase("loading");
    setError(null);
    try {
      const [nextDocument, savedDraft] = await Promise.all([
        materialEditorApi.getDocument(materialId),
        materialEditorApi.getDraft(materialId).catch(() => null),
      ]);
      const normalized = normalizeMaterialBlocks(nextDocument.blocks);
      setDocument(nextDocument);
      setBaselineBlocks(normalized);
      setBlocks(normalized);
      setDraft(savedDraft);
      setPreviewImpact(null);
      previewTokenRef.current = null;
      setEditorKey((current) => current + 1);
      setPhase("editing");
    } catch (loadError) {
      setError(`无法加载可编辑正文：${errorMessage(loadError)}`);
      setPhase("error");
    }
  }, [materialId]);

  useEffect(() => { void loadDocument(); }, [loadDocument]);

  useEffect(() => {
    if (!dirty) return;
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  }, [dirty]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => (
      getCurrentWindow().onCloseRequested((event) => {
        if (nativeCloseAllowedRef.current || !dirty) return;
        event.preventDefault();
        nativeCloseRequestedRef.current = true;
        setDiscardOpen(true);
      })
    )).then((dispose) => { unlisten = dispose; }).catch(() => undefined);
    return () => unlisten?.();
  }, [dirty]);

  useEffect(() => {
    if (!dirty || !document || phase !== "editing") return;
    const fingerprint = materialBlocksFingerprint(blocks);
    if (lastDraftFingerprintRef.current === fingerprint) return;
    const timer = window.setTimeout(() => {
      void queueDraftSave({
        base_revision: document.current_revision,
        blocks: materialBlocksForPersistence(blocks),
      }, fingerprint).catch(() => undefined);
    }, AUTOSAVE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [blocks, dirty, document, draftEpoch, phase, queueDraftSave]);

  const commitBlocks = useCallback(async (previewToken: string | null) => {
    if (!document || phase === "saving" || !previewToken) return;
    setPhase("saving");
    setError(null);
    try {
      const result = await materialEditorApi.commitEdit(materialId, {
        base_revision: document.current_revision,
        blocks: materialBlocksForPersistence(blocks),
        client_request_id: crypto.randomUUID(),
        preview_token: previewToken,
      });
      const committedBlocks = normalizeMaterialBlocks(result.document.blocks);
      setDocument(result.document);
      setBaselineBlocks(committedBlocks);
      setBlocks(committedBlocks);
      setPreviewImpact(result.impact);
      setEditorKey((current) => current + 1);
      setDraft(null);
      setDraftStatus("idle");
      lastDraftFingerprintRef.current = null;
      previewTokenRef.current = null;
      setPhase("editing");
      await deleteDraftSafely();
      onCommitted?.(result.document);
      const restoreAfterSave = restoreAfterSaveRef.current;
      if (restoreAfterSave) {
        restoreAfterSaveRef.current = null;
        await performRestore(restoreAfterSave, result.document);
        return;
      }
      if (exitAfterSaveRef.current) {
        exitAfterSaveRef.current = false;
        await finishExit();
      }
    } catch (saveError) {
      exitAfterSaveRef.current = false;
      restoreAfterSaveRef.current = null;
      if (isMaterialPreviewChanged(saveError)) {
        previewTokenRef.current = null;
        setPreviewImpact(null);
        setError("编辑影响已变化，请重新检查后保存。当前修改和草稿已保留。");
        setPhase("editing");
        void queueDraftSave({
          base_revision: document.current_revision,
          blocks: materialBlocksForPersistence(blocks),
        }).catch(() => undefined);
      } else if (isMaterialRevisionConflict(saveError)) {
        setError("正文已在其他窗口更新。请查看最新版本后再继续编辑。");
        setPhase("conflict");
      } else {
        setError(`保存失败：${errorMessage(saveError)}`);
        setPhase("error");
      }
    }
  }, [blocks, deleteDraftSafely, document, finishExit, materialId, onCommitted, performRestore, phase, queueDraftSave]);

  const previewAndMaybeCommit = useCallback(async (previewOnly = false) => {
    if (!document || !dirty || busy) return;
    setPhase("previewing");
    setError(null);
    try {
      const preview = await materialEditorApi.previewEdit(materialId, {
        base_revision: document.current_revision,
        blocks: materialBlocksForPersistence(blocks),
      });
      setPreviewImpact(preview.impact);
      previewTokenRef.current = preview.preview_token;
      if (previewOnly) {
        setImpactOpen(true);
        setPhase("editing");
      } else if (requiresMaterialEditConfirmation(preview.impact)) {
        setPhase("confirming");
      } else {
        setPhase("editing");
        await commitBlocks(preview.preview_token);
      }
    } catch (previewError) {
      restoreAfterSaveRef.current = null;
      if (isMaterialRevisionConflict(previewError)) {
        setError("正文已在其他窗口更新。请查看最新版本后再继续编辑。");
        setPhase("conflict");
      } else {
        setError(`无法计算编辑影响：${errorMessage(previewError)}`);
        setPhase("error");
      }
    }
  }, [blocks, busy, commitBlocks, dirty, document, materialId]);

  const requestExit = () => {
    if (dirty) setDiscardOpen(true);
    else onCancel();
  };

  const discardAndExit = async () => {
    setDiscardOpen(false);
    setBlocks(baselineBlocks);
    lastDraftFingerprintRef.current = null;
    await deleteDraftSafely(materialBlocksFingerprint(baselineBlocks));
    await finishExit();
  };

  const saveAndExit = () => {
    setDiscardOpen(false);
    exitAfterSaveRef.current = true;
    void previewAndMaybeCommit(false);
  };

  const keepDraftAndExit = async () => {
    if (!document) return;
    setDiscardOpen(false);
    try {
      await queueDraftSave({
        base_revision: document.current_revision,
        blocks: materialBlocksForPersistence(blocks),
      });
      await finishExit();
    } catch (draftError) {
      setDraftStatus("failed");
      setError(`草稿保存失败：${errorMessage(draftError)}`);
    }
  };

  const openSourceEditor = () => {
    const source = materialBlocksToMarkdown(blocks);
    sourceOriginalRef.current = source;
    sourceOriginalBlocksRef.current = blocks;
    setSourceText(source);
    setSourceOpen(true);
  };

  const applySourceEditor = () => {
    if (sourceText === sourceOriginalRef.current) {
      setBlocks(sourceOriginalBlocksRef.current);
      setEditorKey((current) => current + 1);
      setSourceOpen(false);
      return;
    }
    setBlocks(markdownToMaterialBlocks(sourceText, blocks));
    setEditorKey((current) => current + 1);
    setSourceOpen(false);
  };

  const resumeDraft = () => {
    if (!draft || draft.is_stale || draft.base_revision !== document?.current_revision) return;
    setBlocks(normalizeMaterialBlocks(draft.blocks));
    setEditorKey((current) => current + 1);
  };

  const deleteDraft = async () => {
    await deleteDraftSafely(materialBlocksFingerprint(blocks));
  };

  const openHistory = async () => {
    setHistoryOpen(true);
    setHistoryLoading(true);
    setSelectedRevision(null);
    try {
      setRevisions(await materialEditorApi.listRevisions(materialId));
    } catch (historyError) {
      setError(`无法读取版本历史：${errorMessage(historyError)}`);
    } finally {
      setHistoryLoading(false);
    }
  };

  const loadRevision = async (revision: MaterialRevisionSummary) => {
    setHistoryLoading(true);
    try {
      setSelectedRevision(await materialEditorApi.getRevision(materialId, revision.revision));
    } catch (historyError) {
      setError(`无法读取该版本：${errorMessage(historyError)}`);
    } finally {
      setHistoryLoading(false);
    }
  };

  const restoreRevision = async () => {
    if (!restoreTarget || !document) return;
    await performRestore(restoreTarget, document);
  };

  const saveCurrentThenRestore = () => {
    if (!restoreDirtyTarget) return;
    restoreAfterSaveRef.current = restoreDirtyTarget;
    setRestoreDirtyTarget(null);
    void previewAndMaybeCommit(false);
  };

  const keepDraftThenRestore = async () => {
    if (!restoreDirtyTarget || !document) return;
    const target = restoreDirtyTarget;
    const payload = { base_revision: document.current_revision, blocks: materialBlocksForPersistence(blocks) };
    const retainedDraft = await queueDraftSave(payload);
    if (await performRestore(target, document, true)) {
      setDraft({ ...retainedDraft, is_stale: true });
      setDraftStatus("saved");
    }
  };

  const cancelExitRequest = () => {
    nativeCloseRequestedRef.current = false;
    setDiscardOpen(false);
  };

  const discardThenRestore = async () => {
    if (!restoreDirtyTarget || !document) return;
    const target = restoreDirtyTarget;
    await deleteDraftSafely(materialBlocksFingerprint(baselineBlocks));
    await performRestore(target, document);
  };

  const historicBlocks = extractRevisionBlocks(selectedRevision);
  const diff = revisionDiff(blocks, historicBlocks);

  return (
    <section className="flex h-full min-h-0 flex-col bg-background text-foreground" aria-label="正文编辑器" data-editor-phase={phase}>
      <header className="flex flex-wrap items-center gap-3 border-b border-border bg-card/70 px-3 py-3 backdrop-blur-sm sm:px-5">
        <Button size="sm" variant="ghost" onClick={requestExit} disabled={phase === "saving"} aria-label="返回阅读"><ArrowLeft size={18} /></Button>
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-base font-semibold sm:text-lg">{document?.title || title}</h1>
          <div className="mt-0.5 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground" role="status" aria-live="polite">
            {document?.current_revision !== undefined && <span>版本 {document.current_revision}</span>}
          </div>
        </div>
        <Button size="sm" variant="ghost" onClick={() => void openHistory()} disabled={!document || busy} className="gap-1.5"><History size={16} /><span className="hidden sm:inline">版本历史</span></Button>
        <Button size="sm" variant="ghost" onClick={openSourceEditor} disabled={!document || busy} className="gap-1.5" aria-label="编辑纯文本或 Markdown"><FileCode2 size={16} /><span className="hidden sm:inline">源文本</span></Button>
        <Button size="sm" variant="outline" onClick={() => void previewAndMaybeCommit(true)} disabled={!dirty || busy} className="gap-1.5"><AlertTriangle size={16} /><span className="hidden sm:inline">检查影响</span></Button>
        <Button size="sm" onClick={() => void previewAndMaybeCommit(false)} disabled={!dirty || busy} className="gap-1.5" aria-label="保存正文（顶部）">
          {phase === "previewing" || phase === "saving" ? <Loader2 size={16} className="animate-spin" /> : <Save size={16} />}
          <span>{phase === "saving" ? "保存中" : "保存"}</span>
        </Button>
      </header>

      {draft && (
        <div className={`flex flex-wrap items-center gap-2 border-b px-4 py-2 text-xs ${draft.is_stale || draft.base_revision !== document?.current_revision ? "border-amber-500/30 bg-amber-500/10" : "border-primary/20 bg-primary/5"}`} role="status">
          <FileClock size={15} />
          <span className="flex-1">{draft.is_stale || draft.base_revision !== document?.current_revision ? "发现基于旧版正文的草稿，不会自动覆盖当前版本。" : `发现 ${new Date(draft.updated_at).toLocaleString()} 保存的草稿。`}</span>
          {!draft.is_stale && draft.base_revision === document?.current_revision && <Button size="sm" variant="secondary" onClick={resumeDraft}>恢复草稿</Button>}
          <Button size="sm" variant="ghost" onClick={() => void deleteDraft()}>丢弃草稿</Button>
        </div>
      )}

      {error && (
        <div className="flex flex-wrap items-center gap-2 border-b border-destructive/20 bg-destructive/10 px-4 py-2 text-sm text-destructive" role="alert">
          <AlertTriangle size={16} /><span className="flex-1">{error}</span>
          {(phase === "conflict" || phase === "error") && <Button size="sm" variant="outline" onClick={() => void loadDocument()}>重新载入</Button>}
        </div>
      )}

      {phase === "loading" ? (
        <div className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground" role="status"><Loader2 size={18} className="animate-spin" />正在载入可编辑正文</div>
      ) : document ? (
        <MaterialEditorCanvas
          key={editorKey}
          blocks={blocks}
          disabled={busy || phase === "confirming" || phase === "conflict"}
          onBlocksChange={setBlocks}
          onSave={() => void previewAndMaybeCommit(false)}
        />
      ) : (
        <div className="flex flex-1 items-center justify-center"><Button variant="outline" onClick={() => void loadDocument()}>重试</Button></div>
      )}

      <footer className="sticky bottom-0 z-30 flex flex-wrap items-center gap-2 border-t border-border bg-card/95 px-3 py-2 text-xs text-muted-foreground shadow-[0_-8px_24px_rgba(0,0,0,0.04)] backdrop-blur-sm sm:px-5" aria-label="编辑状态">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1" aria-live="polite">
          <span className="font-medium text-foreground">{dirty ? `已修改 ${modifiedBlockCount} 个块` : "没有未保存修改"}</span>
          {draftStatus === "saving" && <span className="inline-flex items-center gap-1"><Loader2 size={12} className="animate-spin" />正在保存草稿</span>}
          {draftStatus === "saved" && <span className="inline-flex items-center gap-1 text-emerald-600"><Cloud size={12} />草稿已保存</span>}
          {draftStatus === "failed" && <span className="inline-flex items-center gap-1 text-amber-600"><CloudOff size={12} />草稿暂未保存</span>}
          <span className="hidden xl:inline">Enter 拆分·Backspace 合并·⌘S 保存·⌘K 链接·⌥↑/⌥↓ 移动块</span>
        </div>
        {previewImpact && <button type="button" className="rounded-lg px-2 py-1 text-primary hover:bg-primary/10" onClick={() => setImpactOpen(true)}>查看上次影响</button>}
        <Button size="sm" variant="secondary" onClick={requestExit} disabled={phase === "saving"}>取消</Button>
        <Button size="sm" onClick={() => void previewAndMaybeCommit(false)} disabled={!dirty || busy} className="gap-1.5">
          {phase === "previewing" || phase === "saving" ? <Loader2 size={15} className="animate-spin" /> : <Save size={15} />}
          {phase === "saving" ? "保存中" : "保存"}
        </Button>
      </footer>

      <Dialog open={phase === "confirming"} onOpenChange={(open) => { if (!open) { exitAfterSaveRef.current = false; restoreAfterSaveRef.current = null; setPhase("editing"); } }} title="确认正文变更" className="max-w-2xl" closeDisabled={phase === "saving"}>
        <p className="mb-4 text-sm leading-6 text-muted-foreground">正文变更会使部分翻译、精讲或定位信息失效。已收录的学习项保留原始上下文，并标记来源变化。</p>
        {previewImpact && <ImpactGrid impact={previewImpact} />}
        <DialogFooter>
          <Button variant="secondary" onClick={() => { exitAfterSaveRef.current = false; restoreAfterSaveRef.current = null; setPhase("editing"); }}>继续编辑</Button>
          <Button onClick={() => void commitBlocks(previewTokenRef.current)} disabled={!previewTokenRef.current} className="gap-1.5"><Save size={16} />确认保存</Button>
        </DialogFooter>
      </Dialog>

      <Dialog open={discardOpen} onOpenChange={(open) => { if (!open) nativeCloseRequestedRef.current = false; setDiscardOpen(open); }} title="处理未保存修改" className="max-w-md">
        <p className="text-sm leading-6 text-muted-foreground">可提交新版本、仅保留本地草稿，或丢弃本次修改。</p>
        <div className="mt-5 grid gap-2">
          <Button onClick={saveAndExit} className="justify-center gap-1.5"><Save size={16} />保存新版本并返回</Button>
          <Button variant="secondary" onClick={() => void keepDraftAndExit()} className="justify-center gap-1.5"><FileClock size={16} />保留草稿并返回</Button>
          <Button variant="danger" onClick={() => void discardAndExit()}>丢弃并返回</Button>
          <Button variant="ghost" onClick={cancelExitRequest}>继续编辑</Button>
        </div>
      </Dialog>

      <Dialog open={sourceOpen} onOpenChange={setSourceOpen} title="纯文本 / Markdown" className="h-[min(760px,calc(100vh-2rem))] max-w-4xl overflow-hidden">
        <div className="flex h-full min-h-0 flex-col gap-3 pt-1">
          <p className="text-xs text-muted-foreground">支持二、三级标题、列表、引用和分隔线。未修改块保留行内格式；直接改写块会按源文本重建该块。</p>
          <textarea
            value={sourceText}
            onChange={(event) => setSourceText(event.target.value)}
            className="min-h-0 flex-1 resize-none rounded-xl border border-input bg-background p-4 font-mono text-sm leading-6 outline-none focus-visible:ring-2 focus-visible:ring-ring"
            aria-label="Markdown 源文本"
            spellCheck={false}
          />
          <DialogFooter>
            <Button variant="secondary" onClick={() => setSourceOpen(false)}>取消</Button>
            <Button onClick={applySourceEditor}>应用到块结构</Button>
          </DialogFooter>
        </div>
      </Dialog>

      <Dialog open={impactOpen} onOpenChange={setImpactOpen} title="编辑影响" className="max-w-2xl max-sm:h-[calc(100vh-2rem)] max-sm:overflow-y-auto">
        {previewImpact ? <ImpactGrid impact={previewImpact} /> : <p className="text-sm text-muted-foreground">尚未计算影响。</p>}
        <DialogFooter><Button onClick={() => setImpactOpen(false)}>完成</Button></DialogFooter>
      </Dialog>

      <Dialog open={historyOpen} onOpenChange={setHistoryOpen} title="版本历史" className="h-[min(760px,calc(100vh-2rem))] max-w-5xl overflow-hidden p-0">
        <div className="grid h-full min-h-0 grid-rows-[220px_minmax(0,1fr)] pt-12 md:grid-cols-[300px_minmax(0,1fr)] md:grid-rows-1">
          <div className="overflow-y-auto border-b border-border p-3 md:border-b-0 md:border-r">
            {historyLoading && revisions.length === 0 && <p className="px-2 py-4 text-sm text-muted-foreground" role="status">正在读取版本</p>}
            {revisions.map((revision) => (
              <button
                key={revision.revision}
                type="button"
                className={`mb-1 w-full rounded-xl px-3 py-2 text-left ${selectedRevision?.revision === revision.revision ? "bg-primary/10 text-primary" : "hover:bg-muted"}`}
                onClick={() => void loadRevision(revision)}
              >
                <span className="block text-sm font-medium">{revisionLabel(revision)}</span>
                <span className="mt-1 block text-[11px] text-muted-foreground">{new Date(revision.created_at).toLocaleString()}</span>
              </button>
            ))}
          </div>
          <div className="min-h-0 overflow-y-auto p-4 sm:p-6">
            {!selectedRevision ? (
              <div className="flex h-full items-center justify-center text-sm text-muted-foreground">选择一个版本查看变更</div>
            ) : (
              <div>
                <div className="flex flex-wrap items-center gap-3">
                  <div className="min-w-0 flex-1"><h3 className="font-semibold">{revisionLabel(selectedRevision)}</h3><p className="mt-1 text-xs text-muted-foreground">版本 {selectedRevision.revision}</p></div>
                  <Button size="sm" variant="outline" onClick={() => {
                    const target = revisions.find((item) => item.revision === selectedRevision.revision) ?? null;
                    if (dirty) setRestoreDirtyTarget(target);
                    else setRestoreTarget(target);
                  }}>恢复此版本</Button>
                </div>
                <div className="mt-5 grid gap-3 sm:grid-cols-4">
                  <div className="rounded-xl bg-emerald-500/10 p-3 text-sm">当前新增 {diff.added.length}</div>
                  <div className="rounded-xl bg-amber-500/10 p-3 text-sm">内容/样式变更 {diff.changed.length}</div>
                  <div className="rounded-xl bg-blue-500/10 p-3 text-sm">位置变更 {diff.moved.length}</div>
                  <div className="rounded-xl bg-red-500/10 p-3 text-sm">当前已删除 {diff.removed.length}</div>
                </div>
                <div className="mt-5 space-y-2">
                  {historicBlocks.map((block) => (
                    <div key={block.id ?? block.block_order} className={`rounded-xl border px-3 py-2 text-sm ${diff.removed.includes(block) ? "border-red-500/30 bg-red-500/5 line-through" : diff.changed.includes(block) ? "border-amber-500/30 bg-amber-500/5" : "border-border"}`}>
                      <span className="mr-2 text-[10px] uppercase text-muted-foreground">{block.block_type}</span>{block.text || "分隔线"}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      </Dialog>

      <Dialog open={Boolean(restoreTarget)} onOpenChange={(open) => { if (!open) setRestoreTarget(null); }} title="恢复历史版本" className="max-w-md" closeDisabled={restoreBusy}>
        <p className="text-sm leading-6 text-muted-foreground">恢复会创建一个新版本，当前历史仍会保留。</p>
        <DialogFooter>
          <Button variant="secondary" onClick={() => setRestoreTarget(null)} disabled={restoreBusy}>取消</Button>
          <Button onClick={() => void restoreRevision()} disabled={restoreBusy} className="gap-1.5">{restoreBusy ? <Loader2 size={16} className="animate-spin" /> : <CheckCircle2 size={16} />}确认恢复</Button>
        </DialogFooter>
      </Dialog>

      <Dialog open={Boolean(restoreDirtyTarget)} onOpenChange={(open) => { if (!open) setRestoreDirtyTarget(null); }} title="恢复前处理未保存修改" className="max-w-md" closeDisabled={restoreBusy}>
        <p className="text-sm leading-6 text-muted-foreground">当前正文尚未提交。选择当前修改的处理方式后再恢复历史版本。</p>
        <div className="mt-5 grid gap-2">
          <Button onClick={saveCurrentThenRestore} disabled={restoreBusy}>保存当前版本后恢复</Button>
          <Button variant="secondary" onClick={() => void keepDraftThenRestore()} disabled={restoreBusy}>保留草稿后恢复</Button>
          <Button variant="danger" onClick={() => void discardThenRestore()} disabled={restoreBusy}>丢弃修改后恢复</Button>
          <Button variant="ghost" onClick={() => setRestoreDirtyTarget(null)} disabled={restoreBusy}>取消</Button>
        </div>
      </Dialog>
    </section>
  );
}

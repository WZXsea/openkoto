import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Loader2, FileText, FolderOpen, Import, Info } from "lucide-react";
import { Article } from "../../types";
import { MaterialImportPreviewDialogs, useMaterialImportPreview } from "../../features/materials/useMaterialImportPreview";

interface TextFileImportFormProps {
  onSave?: (article: Article) => void;
  onCancel: () => void;
}

const TEXT_FILE_EXTENSIONS = ["md", "markdown", "txt", "docx"];

function defaultTitleFromPath(path: string): string {
  return path.split(/[/\\]/).pop()?.replace(/\.(md|markdown|txt|docx)$/i, "") || "";
}

export function TextFileImportForm({ onSave, onCancel }: TextFileImportFormProps) {
  const { t } = useTranslation();
  const [filePath, setFilePath] = useState("");
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const importPreview = useMaterialImportPreview<Article>({
    commit: (importJobId, duplicatePolicy) => invoke<Article>("import_text_file_cmd", {
      filePath,
      ...(title.trim() ? { title: title.trim() } : {}),
      importJobId,
      duplicatePolicy,
    }),
    onSuccess: onSave,
    onError: (err) => setError(String(err)),
  });
  const isImporting = importPreview.isBusy;

  const handleSelectFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: t("textFileImport.fileFilterName", "文本文件"),
            extensions: TEXT_FILE_EXTENSIONS,
          },
        ],
      });

      if (selected && typeof selected === "string") {
        setFilePath(selected);
        setError(null);
        if (!title.trim()) {
          setTitle(defaultTitleFromPath(selected));
        }
      }
    } catch (err) {
      console.error("Failed to select text file:", err);
      setError(t("textFileImport.errors.selectFailed", "选择文件失败"));
    }
  };

  const handleImport = async () => {
    if (!filePath) {
      setError(t("textFileImport.errors.fileRequired", "请选择文本文件"));
      return;
    }

    setError(null);
    await importPreview.startPreview({
      sourceKind: "text_file",
      sourceUri: `file://${filePath}`,
      filePath,
      title: title.trim() || undefined,
    });
  };

  const fileName = filePath
    ? filePath.split(/[/\\]/).pop() || filePath
    : t("textFileImport.filePlaceholder", "选择 Markdown、TXT 或 DOCX 文件...");

  return (
    <div className="flex h-full flex-col">
      <MaterialImportPreviewDialogs
        preview={importPreview.preview}
        duplicate={importPreview.duplicate}
        isBusy={importPreview.isBusy}
        onConfirm={() => void importPreview.confirmPreview()}
        onCancel={() => void importPreview.cancelPreview()}
        onResolve={(action) => void importPreview.resolveDuplicate(action)}
      />
      <div className="flex-1 space-y-4 overflow-y-auto pr-1">
        {error && (
          <div className="rounded-lg border border-red-700 bg-red-900/30 p-3 text-sm text-red-300 break-words">
            {error}
          </div>
        )}

        <div className="flex gap-3 rounded-lg border border-primary/20 bg-primary/10 p-3 text-sm text-foreground/90">
          <Info className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
          <p>{t("textFileImport.hint", "导入本地文本内容，并转成可逐句精读、划词和生成学习候选的文章素材。")}</p>
        </div>

        <div>
          <label className="mb-2 block text-sm font-medium text-foreground">
            {t("textFileImport.fileLabel", "文本文件")}
          </label>
          <button
            type="button"
            onClick={handleSelectFile}
            disabled={isImporting}
            className="flex w-full items-center gap-3 rounded-lg border-2 border-dashed border-border px-4 py-3 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {filePath ? <FileText size={20} className="text-primary" /> : <FolderOpen size={20} />}
            <span className={`flex-1 truncate ${filePath ? "text-foreground" : "text-muted-foreground"}`}>
              {fileName}
            </span>
          </button>
          <p className="mt-2 text-xs text-muted-foreground">
            {t("textFileImport.description", "支持 .md、.markdown、.txt 和 .docx。PDF/EPUB 请继续使用阅读素材入口。")}
          </p>
        </div>

        <div>
          <label className="mb-2 block text-sm font-medium text-foreground">
            {t("textFileImport.titleLabel", "标题")}
            <span className="ml-1 font-normal text-muted-foreground">
              ({t("newArticle.optional", "可选")})
            </span>
          </label>
          <Input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder={t("textFileImport.titlePlaceholder", "留空则使用文件名")}
            disabled={isImporting}
          />
        </div>
      </div>

      <div className="mt-6 flex justify-end gap-3 border-t border-border pt-4">
        <Button variant="secondary" onClick={onCancel} disabled={isImporting}>
          {t("common.cancel")}
        </Button>
        <Button onClick={handleImport} disabled={isImporting || !filePath} className="gap-2">
          {isImporting ? (
            <>
              <Loader2 size={16} className="animate-spin" />
              {t("textFileImport.importing", "导入中...")}
            </>
          ) : (
            <>
              <Import size={16} />
              {t("textFileImport.import", "导入文本")}
            </>
          )}
        </Button>
      </div>
    </div>
  );
}

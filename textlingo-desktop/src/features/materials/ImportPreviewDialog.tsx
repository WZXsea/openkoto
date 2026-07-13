import { AlignLeft, FileText, Hash, HardDrive, Loader2, MapPin, X } from "lucide-react";

import { Button } from "../../components/ui/button";
import { Dialog, DialogFooter } from "../../components/ui/dialog";
import type { PreviewMaterialImportResult } from "./api";

interface ImportPreviewDialogProps {
  isOpen: boolean;
  preview: PreviewMaterialImportResult | null;
  isBusy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

function formatByteSize(value?: number): string | null {
  if (typeof value !== "number" || !Number.isFinite(value)) return null;
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function shortenHash(value?: string): string | null {
  return value ? `${value.slice(0, 12)}...${value.slice(-8)}` : null;
}

export function ImportPreviewDialog({ isOpen, preview, isBusy = false, onConfirm, onCancel }: ImportPreviewDialogProps) {
  const fileSize = formatByteSize(preview?.file.byteSize);
  const fileHash = shortenHash(preview?.file.sha256);

  return (
    <Dialog isOpen={isOpen} onClose={onCancel} title="确认导入素材" className="max-w-2xl" closeDisabled={isBusy}>
      {preview && (
        <div className="min-w-0 space-y-5">
          <div className="min-w-0 border-b border-border pb-4">
            <p className="text-base font-medium break-words">{preview.title}</p>
            {preview.sourceUri && (
              <p className="mt-2 flex min-w-0 gap-2 text-sm text-muted-foreground">
                <MapPin size={16} className="mt-0.5 shrink-0" aria-hidden="true" />
                <span className="break-all">{preview.sourceUri}</span>
              </p>
            )}
          </div>

          {preview.contentSnippet ? (
            <section aria-label="正文片段">
              <p className="mb-2 text-sm font-medium">正文片段</p>
              <p className="max-h-36 overflow-y-auto whitespace-pre-wrap border-l-2 border-primary/50 pl-3 text-sm leading-6 text-muted-foreground">
                {preview.contentSnippet}
              </p>
            </section>
          ) : (
            <section aria-label="文件摘要" className="space-y-2 text-sm text-muted-foreground">
              <p className="flex min-w-0 items-center gap-2"><FileText size={16} className="shrink-0" aria-hidden="true" /><span className="truncate">{preview.file.fileName || "未提供文件名"}</span></p>
              {fileSize && <p className="flex items-center gap-2"><HardDrive size={16} aria-hidden="true" />大小：{fileSize}</p>}
              {fileHash && <p className="flex min-w-0 items-center gap-2"><Hash size={16} className="shrink-0" aria-hidden="true" /><span className="break-all">SHA-256：{fileHash}</span></p>}
            </section>
          )}

          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <AlignLeft size={16} aria-hidden="true" />识别段落：{preview.paragraphCount}
          </p>
        </div>
      )}
      <DialogFooter className="flex-wrap justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onCancel} disabled={isBusy}>
          <X size={16} className="mr-1.5" />取消
        </Button>
        <Button type="button" onClick={onConfirm} disabled={isBusy} aria-busy={isBusy}>
          {isBusy && <Loader2 size={16} className="mr-1.5 animate-spin" />}
          确认并导入
        </Button>
      </DialogFooter>
    </Dialog>
  );
}

import { Copy, ExternalLink, Replace, X } from "lucide-react";

import { Button } from "../../components/ui/button";
import { Dialog, DialogFooter } from "../../components/ui/dialog";
import type { DuplicateMatch, DuplicateResolutionAction } from "./materialManagement";

interface DuplicateResolutionDialogProps {
  isOpen: boolean;
  duplicate: DuplicateMatch | null;
  onResolve: (action: DuplicateResolutionAction) => void;
}

export function DuplicateResolutionDialog({ isOpen, duplicate, onResolve }: DuplicateResolutionDialogProps) {
  return (
    <Dialog
      isOpen={isOpen}
      onClose={() => onResolve("cancel")}
      title="发现重复素材"
      className="max-w-xl"
    >
      <div className="min-w-0 space-y-3">
        <p className="text-sm text-muted-foreground">导入内容与已有素材匹配。请选择本次导入的处理方式。</p>
        {duplicate && (
          <div className="min-w-0 border-y border-border py-3">
            <p className="truncate text-sm font-medium" title={duplicate.title || duplicate.materialId}>{duplicate.title || duplicate.materialId}</p>
            <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
              <span className="break-all">素材 ID：{duplicate.materialId}</span>
              {duplicate.sourceType && <span>类型：{duplicate.sourceType}</span>}
              {duplicate.sourceUrl && <span className="break-all">来源：{duplicate.sourceUrl}</span>}
              {duplicate.matchedBy.length > 0 && <span>匹配依据：{duplicate.matchedBy.join("、")}</span>}
            </div>
          </div>
        )}
      </div>
      <DialogFooter className="flex-wrap justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={() => onResolve("cancel")}>
          <X size={15} className="mr-1.5" />取消
        </Button>
        <Button type="button" variant="outline" size="sm" onClick={() => onResolve("open_existing")}>
          <ExternalLink size={15} className="mr-1.5" />打开已有
        </Button>
        <Button type="button" variant="outline" size="sm" onClick={() => onResolve("replace")}>
          <Replace size={15} className="mr-1.5" />替换
        </Button>
        <Button type="button" size="sm" onClick={() => onResolve("keep_copy")}>
          <Copy size={15} className="mr-1.5" />保留副本
        </Button>
      </DialogFooter>
    </Dialog>
  );
}

import { createContext, type ReactNode, useCallback, useContext, useRef, useState } from "react";

import type { DuplicateMatch, DuplicateResolutionAction } from "./materialManagement";
import { DuplicateResolutionDialog } from "./DuplicateResolutionDialog";
import { ImportPreviewDialog } from "./ImportPreviewDialog";
import {
  cancelMaterialImportJob,
  previewMaterialImport,
  type PreviewMaterialImportRequest,
  type PreviewMaterialImportResult,
} from "./api";

interface UseMaterialImportPreviewOptions<T> {
  commit: (jobId: string, duplicatePolicy: Exclude<DuplicateResolutionAction, "cancel">) => Promise<T>;
  onSuccess?: (result: T) => void;
  onError: (error: unknown) => void;
}

const MaterialImportActivityContext = createContext<(busy: boolean) => void>(() => {});

export function MaterialImportActivityProvider({ children, onBusyChange }: { children: ReactNode; onBusyChange: (busy: boolean) => void }) {
  return <MaterialImportActivityContext.Provider value={onBusyChange}>{children}</MaterialImportActivityContext.Provider>;
}

export function useMaterialImportPreview<T>({ commit, onSuccess, onError }: UseMaterialImportPreviewOptions<T>) {
  const [preview, setPreview] = useState<PreviewMaterialImportResult | null>(null);
  const [duplicate, setDuplicate] = useState<DuplicateMatch | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  const operationRef = useRef<"idle" | "previewing" | "committing" | "cancelling">("idle");
  const setParentBusy = useContext(MaterialImportActivityContext);

  const reset = useCallback(() => {
    setPreview(null);
    setDuplicate(null);
  }, []);

  const startPreview = useCallback(async (request: PreviewMaterialImportRequest) => {
    if (operationRef.current !== "idle") return;
    operationRef.current = "previewing";
    setParentBusy(true);
    setIsBusy(true);
    try {
      setPreview(await previewMaterialImport(request));
    } catch (error) {
      onError(error);
    } finally {
      operationRef.current = "idle";
      setParentBusy(false);
      setIsBusy(false);
    }
  }, [onError, setParentBusy]);

  const cancelJob = useCallback(async (jobId: string) => {
    if (operationRef.current !== "idle") return;
    operationRef.current = "cancelling";
    setParentBusy(true);
    setIsBusy(true);
    try {
      await cancelMaterialImportJob(jobId);
      reset();
    } catch (error) {
      onError(error);
    } finally {
      operationRef.current = "idle";
      setParentBusy(false);
      setIsBusy(false);
    }
  }, [onError, reset, setParentBusy]);

  const commitImport = useCallback(async (jobId: string, duplicatePolicy: Exclude<DuplicateResolutionAction, "cancel">) => {
    if (operationRef.current !== "idle") return;
    operationRef.current = "committing";
    setParentBusy(true);
    setIsBusy(true);
    try {
      const result = await commit(jobId, duplicatePolicy);
      reset();
      onSuccess?.(result);
    } catch (error) {
      onError(error);
    } finally {
      operationRef.current = "idle";
      setParentBusy(false);
      setIsBusy(false);
    }
  }, [commit, onError, onSuccess, reset, setParentBusy]);

  const confirmPreview = useCallback(async () => {
    if (!preview) return;
    if (preview.duplicates[0]) {
      setDuplicate(preview.duplicates[0]);
      return;
    }
    await commitImport(preview.jobId, "keep_copy");
  }, [commitImport, preview]);

  const cancelPreview = useCallback(async () => {
    if (preview) await cancelJob(preview.jobId);
  }, [cancelJob, preview]);

  const resolveDuplicate = useCallback(async (action: DuplicateResolutionAction) => {
    if (!preview || !duplicate) return;
    const jobId = preview?.jobId;
    if (!jobId) return;
    if (action === "cancel") {
      await cancelJob(jobId);
      return;
    }
    setDuplicate(null);
    await commitImport(jobId, action);
  }, [cancelJob, commitImport, duplicate, preview]);

  return { preview, duplicate, isBusy, startPreview, confirmPreview, cancelPreview, resolveDuplicate };
}

interface MaterialImportPreviewDialogsProps {
  preview: PreviewMaterialImportResult | null;
  duplicate: DuplicateMatch | null;
  isBusy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  onResolve: (action: DuplicateResolutionAction) => void;
}

export function MaterialImportPreviewDialogs({ preview, duplicate, isBusy = false, onConfirm, onCancel, onResolve }: MaterialImportPreviewDialogsProps) {
  return (
    <>
      <ImportPreviewDialog isOpen={Boolean(preview) && !duplicate} preview={preview} isBusy={isBusy} onConfirm={onConfirm} onCancel={onCancel} />
      <DuplicateResolutionDialog isOpen={Boolean(duplicate)} duplicate={duplicate} onResolve={onResolve} />
    </>
  );
}

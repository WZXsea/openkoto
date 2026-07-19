import { AlertTriangle, CheckCircle2, Clock3, Eye, FileWarning, Loader2, RotateCcw, Upload, XCircle } from "lucide-react";
import { useMemo, useState } from "react";

import { Button } from "../../components/ui/button";
import {
  formatImportJobTime,
  getImportJobProgress,
  getImportJobStatusLabel,
  type MaterialImportJob,
  type MaterialImportJobsApi,
} from "./materialManagement";

interface MaterialImportJobsPanelProps {
  jobs: MaterialImportJob[];
  api: MaterialImportJobsApi;
  isLoading?: boolean;
  error?: string | null;
  onRetry?: () => void;
  onResolveDuplicate?: (job: MaterialImportJob) => void;
}

function sourceKindLabel(value: string): string {
  return value.replace(/[_-]+/g, " ");
}

function StatusIcon({ status }: { status: MaterialImportJob["status"] }) {
  if (status === "succeeded") return <CheckCircle2 size={15} className="text-emerald-600" />;
  if (status === "failed_retryable" || status === "failed_terminal") return <FileWarning size={15} className="text-destructive" />;
  if (status === "cancelled") return <XCircle size={15} className="text-muted-foreground" />;
  if (status === "queued") return <Clock3 size={15} className="text-muted-foreground" />;
  return <Loader2 size={15} className="animate-spin text-primary" />;
}

function canCancel(status: MaterialImportJob["status"]): boolean {
  return ["queued", "validating", "parsing", "preview_ready"].includes(status);
}

export function MaterialImportJobsPanel({
  jobs,
  api,
  isLoading = false,
  error,
  onRetry,
  onResolveDuplicate,
}: MaterialImportJobsPanelProps) {
  const [expandedPreviewId, setExpandedPreviewId] = useState<string | null>(null);
  const [cancellingJobId, setCancellingJobId] = useState<string | null>(null);
  const [runningJobId, setRunningJobId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const recentJobs = useMemo(() => [...jobs].sort((left, right) => Date.parse(right.updatedAt || right.createdAt) - Date.parse(left.updatedAt || left.createdAt)), [jobs]);

  const runJobAction = async (jobId: string, operation: () => Promise<void>): Promise<boolean> => {
    setRunningJobId(jobId);
    setActionError(null);
    try {
      await operation();
      return true;
    } catch (caught) {
      setActionError(caught instanceof Error ? caught.message : typeof caught === "string" ? caught : "导入任务操作未完成");
      return false;
    } finally {
      setRunningJobId(null);
      setCancellingJobId(null);
    }
  };

  return (
    <section className="min-w-0 space-y-3" aria-label="最近导入任务" aria-busy={isLoading}>
      <header className="flex flex-wrap items-start justify-between gap-2 border-b border-border pb-3">
        <div>
          <h2 className="text-sm font-semibold">最近导入任务</h2>
          <p className="mt-1 text-xs text-muted-foreground">保留最近 {recentJobs.length} 个任务</p>
        </div>
      </header>

      {(error || actionError) && <div className="flex flex-wrap items-center gap-2 text-sm text-destructive" role="alert"><AlertTriangle size={16} /><span>{actionError || error}</span>{error && onRetry && <Button type="button" variant="ghost" size="sm" onClick={onRetry}>重试</Button>}</div>}

      <div className="overflow-x-auto border-y border-border">
        <div className="min-w-[560px] divide-y divide-border">
          <div className="grid grid-cols-[minmax(120px,1fr)_130px_minmax(112px,0.8fr)_96px_auto] gap-3 px-3 py-2 text-xs text-muted-foreground">
            <span>来源</span><span>状态</span><span>进度</span><span>时间</span><span>操作</span>
          </div>
          {recentJobs.map((job) => {
            const progress = getImportJobProgress(job.progress);
            const isFailed = job.status === "failed_retryable" || job.status === "failed_terminal";
            const previewOpen = expandedPreviewId === job.id;
            const isRunning = runningJobId === job.id;
            const hasDuplicates = (job.preview?.duplicateMatches?.length ?? 0) > 0;
            const canResumePreview = job.status === "preview_ready" && !hasDuplicates && Boolean(api.resolveJob);
            const canRecoverCommit = job.status === "committing"
              && Date.now() - Date.parse(job.updatedAt || job.createdAt) >= 5 * 60 * 1000;
            return (
              <div key={job.id} data-status={job.status}>
                <div className="grid grid-cols-[minmax(120px,1fr)_130px_minmax(112px,0.8fr)_96px_auto] items-center gap-3 px-3 py-2.5">
                  <span className="truncate text-sm font-medium" title={job.sourceKind}>{sourceKindLabel(job.sourceKind)}</span>
                  <span className="flex items-center gap-1.5 text-xs"><StatusIcon status={job.status} />{getImportJobStatusLabel(job.status)}</span>
                  <span className="flex min-w-0 items-center gap-2">
                    {progress === null ? <span className="text-xs text-muted-foreground">-</span> : <><span className="h-1.5 min-w-12 flex-1 overflow-hidden rounded bg-muted"><span className="block h-full bg-primary" style={{ width: `${progress}%` }} /></span><span className="w-8 text-right text-xs tabular-nums text-muted-foreground">{progress}%</span></>}
                  </span>
                  <time className="text-xs text-muted-foreground" dateTime={job.updatedAt || job.createdAt}>{formatImportJobTime(job.updatedAt || job.createdAt)}</time>
                  <span className="flex items-center justify-end gap-1">
                    {job.status === "preview_ready" && <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={`查看 ${job.sourceKind} 预览`} title="查看预览" disabled={isRunning} onClick={() => { setExpandedPreviewId(previewOpen ? null : job.id); api.openPreview?.(job); }}><Eye size={15} /></Button>}
                    {canResumePreview && <Button type="button" variant="outline" size="sm" aria-label={`提交 ${job.sourceKind}`} title="提交导入" disabled={isRunning} onClick={() => { void runJobAction(job.id, async () => { await api.resolveJob!(job.id, "keep_copy"); }).then((completed) => { if (completed) onRetry?.(); }); }}>{isRunning ? <Loader2 size={15} className="mr-1.5 animate-spin" /> : <Upload size={15} className="mr-1.5" />}提交</Button>}
                    {canRecoverCommit && <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={`恢复 ${job.sourceKind}`} title="恢复中断的提交" disabled={isRunning} onClick={() => runJobAction(job.id, () => api.retryJob(job.id))}>{isRunning ? <Loader2 size={15} className="animate-spin" /> : <RotateCcw size={15} />}</Button>}
                    {job.status === "failed_retryable" && <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={`重试 ${job.sourceKind}`} title="重试" disabled={isRunning} onClick={() => runJobAction(job.id, () => api.retryJob(job.id))}>{isRunning ? <Loader2 size={15} className="animate-spin" /> : <RotateCcw size={15} />}</Button>}
                    {canCancel(job.status) && <Button type="button" variant="ghost" size="icon" className="h-8 w-8 text-destructive hover:text-destructive" aria-label={`取消 ${job.sourceKind}`} title="取消任务" disabled={isRunning} onClick={() => setCancellingJobId(job.id)}><XCircle size={15} /></Button>}
                  </span>
                </div>
                {isFailed && <div className="grid grid-cols-[minmax(120px,1fr)_130px_minmax(112px,0.8fr)_96px_auto] gap-3 border-t border-dashed border-border px-3 py-2 text-xs text-destructive"><span className="col-span-4 min-w-0 break-words"><span className="font-medium">{job.errorCode || "IMPORT_FAILED"}</span>{job.errorMessage ? `: ${job.errorMessage}` : ""}</span></div>}
                {job.status === "preview_ready" && previewOpen && (
                  <div className="border-t border-dashed border-border px-3 py-3">
                    <p className="text-sm font-medium">{job.preview?.title || "导入预览"}</p>
                    {job.preview?.summary && <p className="mt-1 text-xs text-muted-foreground">{job.preview.summary}</p>}
                    {hasDuplicates && (
                      <div className="mt-2 space-y-1">
                        <p className="text-xs font-medium">重复匹配</p>
                        {job.preview?.duplicateMatches?.map((match) => (
                          <div key={match.materialId} className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                            <span className="truncate">{match.title || match.materialId}</span>
                            {match.sourceType && <span className="shrink-0">{match.sourceType}</span>}
                            {match.matchedBy.length > 0 && <span className="shrink-0">{match.matchedBy.join("、")}</span>}
                          </div>
                        ))}
                        {onResolveDuplicate && <Button type="button" variant="outline" size="sm" className="mt-1" onClick={() => onResolveDuplicate(job)}>处理重复项</Button>}
                      </div>
                    )}
                  </div>
                )}
                {cancellingJobId === job.id && <div className="flex items-center justify-end gap-2 border-t border-dashed border-border px-3 py-2" role="alertdialog" aria-label="确认取消导入"><span className="mr-auto text-xs text-muted-foreground">取消后当前导入不会写入素材库。</span><Button type="button" variant="ghost" size="sm" onClick={() => setCancellingJobId(null)} disabled={isRunning}>返回</Button><Button type="button" variant="danger" size="sm" onClick={() => runJobAction(job.id, () => api.cancelJob(job.id))} disabled={isRunning}>{isRunning && <Loader2 size={14} className="mr-1.5 animate-spin" />}确认取消</Button></div>}
              </div>
            );
          })}
          {!isLoading && recentJobs.length === 0 && <p className="px-3 py-8 text-center text-sm text-muted-foreground">暂无导入任务</p>}
          {isLoading && <p className="flex items-center justify-center gap-2 px-3 py-8 text-sm text-muted-foreground"><Loader2 size={16} className="animate-spin" />正在加载导入任务</p>}
        </div>
      </div>
    </section>
  );
}

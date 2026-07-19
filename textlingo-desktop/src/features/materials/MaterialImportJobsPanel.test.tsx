import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { MaterialImportJobsPanel } from "./MaterialImportJobsPanel";
import type { MaterialImportJob, MaterialImportJobsApi } from "./materialManagement";

const jobs: MaterialImportJob[] = [
  { id: "queued", sourceKind: "text_file", status: "queued", progress: 0, createdAt: "2026-07-10T08:00:00Z" },
  { id: "validating", sourceKind: "web", status: "validating", progress: 0.2, createdAt: "2026-07-10T08:01:00Z" },
  { id: "parsing", sourceKind: "pdf", status: "parsing", progress: 0.4, createdAt: "2026-07-10T08:02:00Z" },
  { id: "preview", sourceKind: "epub", status: "preview_ready", progress: 1, createdAt: "2026-07-10T08:03:00Z", preview: { title: "Imported chapter", duplicateMatches: [{ materialId: "existing-1", title: "Existing chapter", matchedBy: ["content_sha256"] }] } },
  { id: "preview-clean", sourceKind: "article", status: "preview_ready", progress: 1, createdAt: "2026-07-10T08:03:30Z", preview: { title: "Fresh import", duplicateMatches: [] } },
  { id: "committing", sourceKind: "audio", status: "committing", progress: 0.8, createdAt: "2026-07-10T08:04:00Z" },
  { id: "succeeded", sourceKind: "book", status: "succeeded", progress: 1, createdAt: "2026-07-10T08:05:00Z" },
  { id: "retryable", sourceKind: "web", status: "failed_retryable", createdAt: "2026-07-10T08:06:00Z", errorCode: "FETCH_TIMEOUT", errorMessage: "Timed out" },
  { id: "terminal", sourceKind: "pdf", status: "failed_terminal", createdAt: "2026-07-10T08:07:00Z", errorCode: "UNSUPPORTED", errorMessage: "Unsupported format" },
  { id: "cancelled", sourceKind: "text_file", status: "cancelled", createdAt: "2026-07-10T08:08:00Z" },
];

function createApi(): MaterialImportJobsApi {
  return { retryJob: vi.fn().mockResolvedValue(undefined), cancelJob: vi.fn().mockResolvedValue(undefined), openPreview: vi.fn() };
}

describe("MaterialImportJobsPanel", () => {
  afterEach(cleanup);

  it("renders every import status, progress, preview, and duplicate match", async () => {
    const api = createApi();
    const onResolveDuplicate = vi.fn();
    const user = userEvent.setup();
    render(<MaterialImportJobsPanel jobs={jobs} api={api} onResolveDuplicate={onResolveDuplicate} />);

    for (const label of ["等待中", "校验中", "解析中", "待确认", "正在写入", "已完成", "可重试失败", "导入失败", "已取消"]) expect(screen.getAllByText(label).length).toBeGreaterThan(0);
    expect(screen.getByText("FETCH_TIMEOUT")).toBeInTheDocument();
    expect(screen.getByText(/Timed out/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看 epub 预览" }));
    expect(screen.getByText("Existing chapter")).toBeInTheDocument();
    expect(screen.getByText("content_sha256")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "处理重复项" }));
    expect(onResolveDuplicate).toHaveBeenCalledWith(expect.objectContaining({ id: "preview" }));
    expect(api.openPreview).toHaveBeenCalledWith(expect.objectContaining({ id: "preview" }));
  });

  it("retries only retryable failures and confirms cancellation", async () => {
    const api = createApi();
    const user = userEvent.setup();
    render(<MaterialImportJobsPanel jobs={jobs} api={api} />);

    await user.click(screen.getByRole("button", { name: "重试 web" }));
    expect(api.retryJob).toHaveBeenCalledWith("retryable");
    expect(screen.queryByRole("button", { name: "重试 pdf" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "取消 text_file" }));
    await user.click(screen.getByRole("button", { name: "确认取消" }));
    expect(api.cancelJob).toHaveBeenCalledWith("queued");
    expect(screen.queryByRole("button", { name: "取消 audio" })).not.toBeInTheDocument();
  });

  it("submits an unconfirmed non-duplicate preview with keep_copy and refreshes jobs", async () => {
    const api = createApi();
    api.resolveJob = vi.fn().mockResolvedValue({ id: "imported-1" });
    const onRetry = vi.fn();
    const user = userEvent.setup();
    render(<MaterialImportJobsPanel jobs={jobs} api={api} onRetry={onRetry} />);

    await user.click(screen.getByRole("button", { name: "提交 article" }));

    expect(api.resolveJob).toHaveBeenCalledWith("preview-clean", "keep_copy");
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("does not offer direct submission for a preview with duplicate matches", () => {
    const api = createApi();
    api.resolveJob = vi.fn().mockResolvedValue({ id: "imported-1" });
    render(<MaterialImportJobsPanel jobs={jobs} api={api} />);

    expect(screen.queryByRole("button", { name: "提交 epub" })).not.toBeInTheDocument();
  });
});

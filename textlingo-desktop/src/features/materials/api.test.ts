import { describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { createMaterialsApi, previewMaterialImport } from "./api";

describe("materials Tauri adapter", () => {
  it("uses bulk material commands and preserves their request shapes", async () => {
    invoke.mockResolvedValueOnce({ affected: 2 }).mockResolvedValueOnce({ affected: 2 });
    const api = createMaterialsApi();

    await api.archive(["a", "b"]);
    await api.remove(["a", "b"]);

    expect(invoke).toHaveBeenNthCalledWith(1, "material_library_bulk_archive_cmd", { request: { ids: ["a", "b"] } });
    expect(invoke).toHaveBeenNthCalledWith(2, "material_library_bulk_delete_cmd", { request: { ids: ["a", "b"] } });
  });

  it("resumes a retryable import job through the dedicated command", async () => {
    invoke.mockResolvedValueOnce(undefined);

    await createMaterialsApi().jobs.retryJob("job-retry");

    expect(invoke).toHaveBeenCalledWith("material_library_resume_import_job_cmd", {
      id: "job-retry",
      duplicatePolicy: null,
    });
  });

  it("resolves a preview-ready job through the resume command with its duplicate policy", async () => {
    invoke.mockResolvedValueOnce(undefined);
    const resolveJob = createMaterialsApi().jobs.resolveJob;

    expect(resolveJob).toBeDefined();
    await resolveJob!("job-preview", "replace");

    expect(invoke).toHaveBeenCalledWith("material_library_resume_import_job_cmd", {
      id: "job-preview",
      duplicatePolicy: "replace",
    });
  });

  it("maps import jobs and preview duplicate matches into the view model", async () => {
    invoke
      .mockResolvedValueOnce([{
        id: "job-1",
        source_kind: "video",
        status: "preview_ready",
        progress: 0.75,
        created_at: "2026-07-11T00:00:00Z",
      preview: { title: "Lesson", duplicates: { duplicate: true, matches: [{ material_id: "existing-1", title: "Existing lesson", source_type: "video", source_url: "file:///tmp/lesson.mp4", matched_by: ["file_sha256"] }] } },
      }])
      .mockResolvedValueOnce({
      title: "Remote lesson",
      source_uri: "https://example.com",
      content_snippet: "A remote preview snippet.",
      file: { file_path: null, file_id: "file-2", file_name: null, byte_size: null, sha256: null },
      paragraph_count: 3,
      job: { id: "job-2" },
      duplicates: { duplicate: true, matches: [{ material_id: "existing-2", title: "Existing URL", source_type: "web", source_url: "https://example.com", matched_by: ["source_url"] }] },
      });
    const api = createMaterialsApi();

    await expect(api.listImportJobs()).resolves.toEqual([expect.objectContaining({
      sourceKind: "video",
      preview: expect.objectContaining({ duplicateMatches: [expect.objectContaining({ materialId: "existing-1", title: "Existing lesson", sourceType: "video", matchedBy: ["file_sha256"] })] }),
    })]);
    await expect(previewMaterialImport({ sourceKind: "youtube", sourceUri: "https://youtu.be/example" })).resolves.toEqual({
      jobId: "job-2",
      title: "Remote lesson",
      sourceUri: "https://example.com",
      contentSnippet: "A remote preview snippet.",
      paragraphCount: 3,
      file: { fileId: "file-2" },
      duplicates: [expect.objectContaining({ materialId: "existing-2", title: "Existing URL", sourceUrl: "https://example.com", matchedBy: ["source_url"] })],
    });
    expect(invoke).toHaveBeenLastCalledWith("preview_material_import_cmd", {
      request: { source_kind: "youtube", source_uri: "https://youtu.be/example", content: undefined, file_path: undefined, title: undefined },
    });
  });

  it("writes every reader update through the upsert command", async () => {
    invoke.mockResolvedValueOnce({
      reader_kind: "epub",
      locator: { kind: "epub_cfi", cfi: "epubcfi(/6/2)" },
      progress_ratio: 0.4,
      status: "reading",
    }).mockResolvedValueOnce(undefined);
    const api = createMaterialsApi();
    const update = {
      reader_kind: "txt" as const,
      locator: { kind: "page" as const, page: 3, total_pages: 10 },
      progress_ratio: 0.3,
      status: "reading" as const,
    };

    await expect(api.getReadingProgress("material-1")).resolves.toEqual(expect.objectContaining({
      reader_kind: "epub",
      locator: { kind: "epub_cfi", cfi: "epubcfi(/6/2)" },
    }));
    await api.upsertReadingProgress("material-1", update);

    expect(invoke).toHaveBeenCalledWith("material_library_get_reading_progress_cmd", { materialId: "material-1" });
    expect(invoke).toHaveBeenCalledWith("material_library_upsert_reading_progress_cmd", {
      materialId: "material-1",
      request: update,
    });
  });

  it("ignores malformed saved progress instead of passing a scalar locator to readers", async () => {
    invoke.mockResolvedValueOnce({
      reader_kind: "txt",
      locator: "page=3",
      progress_ratio: 0.3,
      status: "reading",
    });

    await expect(createMaterialsApi().getReadingProgress("material-1")).resolves.toBeUndefined();
  });
});

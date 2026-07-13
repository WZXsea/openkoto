import { afterEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { getExtension, isSupportedDropPath, importDroppedPath } from "./dropImport";

describe("dropImport", () => {
  afterEach(() => invokeMock.mockReset());

  it("extracts lowercased extension from path", () => {
    expect(getExtension("/a/b/My File.MP4")).toBe("mp4");
    expect(getExtension("C:\\x\\book.EPUB")).toBe("epub");
    expect(getExtension("/no/ext")).toBe("");
  });

  it("recognizes supported extensions", () => {
    expect(isSupportedDropPath("/x/a.pdf")).toBe(true);
    expect(isSupportedDropPath("/x/a.md")).toBe(true);
    expect(isSupportedDropPath("/x/a.docx")).toBe(true);
    expect(isSupportedDropPath("/x/a.mp3")).toBe(true);
    expect(isSupportedDropPath("/x/a.srt")).toBe(true);
    expect(isSupportedDropPath("/x/a.png")).toBe(false);
  });

  it("routes by extension to the right import command", async () => {
    let jobCount = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_material_import_cmd") {
        jobCount += 1;
        return Promise.resolve({ job: { id: `job-${jobCount}` }, duplicates: { duplicate: false, matches: [] } });
      }
      return Promise.resolve({ id: "1" });
    });

    await expect(importDroppedPath("/x/book.pdf")).resolves.toEqual({ kind: "imported", article: { id: "1" } });
    expect(invokeMock).toHaveBeenCalledWith("import_book_cmd", { filePath: "/x/book.pdf", title: null, importJobId: "job-1", duplicatePolicy: "keep_copy" });

    await importDroppedPath("/x/notes.md");
    expect(invokeMock).toHaveBeenCalledWith("import_text_file_cmd", { filePath: "/x/notes.md", title: null, importJobId: "job-2", duplicatePolicy: "keep_copy" });

    await importDroppedPath("/x/report.docx");
    expect(invokeMock).toHaveBeenCalledWith("import_text_file_cmd", { filePath: "/x/report.docx", title: null, importJobId: "job-3", duplicatePolicy: "keep_copy" });

    await importDroppedPath("/x/movie.mkv");
    expect(invokeMock).toHaveBeenCalledWith("import_local_video_cmd", { filePath: "/x/movie.mkv", importJobId: "job-4", duplicatePolicy: "keep_copy" });

    await importDroppedPath("/x/song.m4a");
    expect(invokeMock).toHaveBeenCalledWith("import_local_video_cmd", { filePath: "/x/song.m4a", importJobId: "job-5", duplicatePolicy: "keep_copy" });

    await importDroppedPath("/x/subs.srt");
    expect(invokeMock).toHaveBeenCalledWith("import_srt_file_cmd", { filePath: "/x/subs.srt", title: null, importJobId: "job-6", duplicatePolicy: "keep_copy" });
    expect(invokeMock).toHaveBeenCalledTimes(12);
  });

  it("returns a conflict without importing or opening an existing dropped duplicate", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_material_import_cmd") {
        return Promise.resolve({
          job: { id: "duplicate-job" },
          duplicates: { duplicate: true, matches: [{ material_id: "existing", matched_by: ["file_hash"] }] },
        });
      }
      return Promise.resolve({ id: "existing" });
    });

    await expect(importDroppedPath("/x/book.pdf")).resolves.toEqual(expect.objectContaining({
      kind: "conflict",
      preview: expect.objectContaining({
        jobId: "duplicate-job",
        duplicates: [expect.objectContaining({ materialId: "existing", matchedBy: ["file_hash"] })],
      }),
    }));
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalledWith("import_book_cmd", expect.anything());
  });

  it("throws unsupported:<name> for unknown types", async () => {
    await expect(importDroppedPath("/x/image.png")).rejects.toThrow("unsupported:image.png");
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { LocalAudioImportForm } from "./LocalAudioImportForm";
import { LocalVideoImportForm } from "./LocalVideoImportForm";

const invokeMock = vi.fn();
const openMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
}));

describe("Local media import forms", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    openMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_material_import_cmd") {
        return Promise.resolve({ job: { id: "job-1" }, duplicates: { duplicate: false, matches: [] } });
      }
      if (command === "import_local_video_cmd") return Promise.resolve({ id: "article-1" });
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("passes an optional subtitle path during local video import", async () => {
    openMock
      .mockResolvedValueOnce("/tmp/lesson.mp4")
      .mockResolvedValueOnce("/tmp/lesson.en.srt");

    render(<LocalVideoImportForm onCancel={() => {}} />);

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /select video file|选择视频文件/i }));
    await user.click(screen.getByRole("button", { name: /select subtitle file|选择字幕文件/i }));
    await user.click(screen.getByRole("button", { name: /import|导入/i }));
    await user.click(screen.getByRole("button", { name: "确认并导入" }));

    expect(invokeMock).toHaveBeenCalledWith("import_local_video_cmd", {
      filePath: "/tmp/lesson.mp4",
      subtitlePath: "/tmp/lesson.en.srt",
      importJobId: "job-1",
      duplicatePolicy: "keep_copy",
    });
  });

  it("passes an optional subtitle path during local audio import", async () => {
    openMock
      .mockResolvedValueOnce("/tmp/lesson.mp3")
      .mockResolvedValueOnce("/tmp/lesson.en.srt");

    render(<LocalAudioImportForm onCancel={() => {}} />);

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /select audio file|选择音频文件/i }));
    await user.click(screen.getByRole("button", { name: /select subtitle file|选择字幕文件/i }));
    await user.click(screen.getByRole("button", { name: /import|导入/i }));
    await user.click(screen.getByRole("button", { name: "确认并导入" }));

    expect(invokeMock).toHaveBeenCalledWith("import_local_video_cmd", {
      filePath: "/tmp/lesson.mp3",
      subtitlePath: "/tmp/lesson.en.srt",
      importJobId: "job-1",
      duplicatePolicy: "keep_copy",
    });
  });
});

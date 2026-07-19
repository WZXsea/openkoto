import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { YouTubeImportForm } from "./YouTubeImportForm";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
}));

describe("YouTubeImportForm", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_material_import_cmd") {
        return Promise.resolve({
          title: "YouTube lesson",
          source_uri: "https://youtu.be/example",
          content_snippet: null,
          file: {},
          paragraph_count: 0,
          duplicates: { duplicate: false, matches: [] },
          job: { id: "job-youtube" },
        });
      }
      if (command === "import_youtube_video_cmd") return Promise.resolve({ id: "article-1" });
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
  });

  afterEach(cleanup);

  it("previews the URL and commits only after confirmation", async () => {
    render(<YouTubeImportForm onCancel={() => {}} />);
    const user = userEvent.setup();

    await user.type(screen.getByPlaceholderText("youtubeImport.urlPlaceholder"), "https://youtu.be/example");
    await user.click(screen.getByRole("button", { name: "youtubeImport.import" }));

    expect(invokeMock).not.toHaveBeenCalledWith("import_youtube_video_cmd", expect.anything());
    await user.click(screen.getByRole("button", { name: "确认并导入" }));

    expect(invokeMock).toHaveBeenCalledWith("import_youtube_video_cmd", {
      url: "https://youtu.be/example",
      importJobId: "job-youtube",
      duplicatePolicy: "keep_copy",
    });
  });
});

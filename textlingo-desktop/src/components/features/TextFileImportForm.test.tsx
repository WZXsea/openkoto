import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TextFileImportForm } from "./TextFileImportForm";

const invokeMock = vi.fn();
const openMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
}));

describe("Text file import form", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    openMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_material_import_cmd") return Promise.resolve({ job: { id: "job-1" }, duplicates: { duplicate: false, matches: [] } });
      if (command === "import_text_file_cmd") return Promise.resolve({ id: "article-1" });
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("imports a selected text file with an optional title", async () => {
    openMock.mockResolvedValue("/tmp/notes.docx");

    render(<TextFileImportForm onCancel={() => {}} />);

    const user = userEvent.setup();
    await user.click(screen.getByText(/select markdown|选择 markdown/i));
    await user.clear(screen.getByPlaceholderText(/leave empty to use the file name|留空则使用文件名/i));
    await user.type(
      screen.getByPlaceholderText(/leave empty to use the file name|留空则使用文件名/i),
      "Imported Notes"
    );
    await user.click(screen.getByRole("button", { name: /import text|导入文本/i }));

    expect(invokeMock).not.toHaveBeenCalledWith("import_text_file_cmd", expect.anything());
    await user.click(screen.getByRole("button", { name: "确认并导入" }));

    expect(openMock).toHaveBeenCalledWith({
      multiple: false,
      filters: [
        {
          name: "文本文件",
          extensions: ["md", "markdown", "txt", "docx"],
        },
      ],
    });
    expect(invokeMock).toHaveBeenCalledWith("import_text_file_cmd", {
      filePath: "/tmp/notes.docx",
      title: "Imported Notes",
      importJobId: "job-1",
      duplicatePolicy: "keep_copy",
    });
  });

  it("cancels the preview job without committing the import", async () => {
    openMock.mockResolvedValue("/tmp/notes.txt");
    render(<TextFileImportForm onCancel={() => {}} />);
    const user = userEvent.setup();

    await user.click(screen.getByText(/select markdown|选择 markdown/i));
    await user.click(screen.getByRole("button", { name: /import text|导入文本/i }));
    await user.click(screen.getByRole("button", { name: "取消" }));

    expect(invokeMock).toHaveBeenCalledWith("material_library_cancel_import_job_cmd", { id: "job-1" });
    expect(invokeMock).not.toHaveBeenCalledWith("import_text_file_cmd", expect.anything());
  });

  it("opens duplicate resolution only after confirmation and submits the selected policy", async () => {
    openMock.mockResolvedValue("/tmp/notes.txt");
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_material_import_cmd") {
        return Promise.resolve({
          title: "notes",
          source_uri: "file:///tmp/notes.txt",
          file: { file_name: "notes.txt", byte_size: 12, sha256: "abc123" },
          paragraph_count: 1,
          content_snippet: "preview text",
          job: { id: "job-duplicate" },
          duplicates: { duplicate: true, matches: [{ material_id: "existing", title: "Existing notes", matched_by: ["content_sha256"] }] },
        });
      }
      if (command === "import_text_file_cmd") return Promise.resolve({ id: "article-1" });
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
    render(<TextFileImportForm onCancel={() => {}} />);
    const user = userEvent.setup();

    await user.click(screen.getByText(/select markdown|选择 markdown/i));
    await user.click(screen.getByRole("button", { name: /import text|导入文本/i }));
    await user.click(screen.getByRole("button", { name: "确认并导入" }));
    expect(screen.getByText("发现重复素材")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "替换" }));

    expect(invokeMock).toHaveBeenCalledWith("import_text_file_cmd", {
      filePath: "/tmp/notes.txt",
      title: "notes",
      importJobId: "job-duplicate",
      duplicatePolicy: "replace",
    });
  });
});

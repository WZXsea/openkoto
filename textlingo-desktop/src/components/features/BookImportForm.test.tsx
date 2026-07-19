import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BookImportForm } from "./BookImportForm";

const invokeMock = vi.fn();
const openMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
}));

describe("BookImportForm theme styling", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    openMock.mockReset();
  });

  afterEach(cleanup);

  it("uses primary token styling for the import hint", () => {
    render(<BookImportForm />);

    const hint = screen
      .getByText("Supports papers, books, novels, etc...")
      .closest("div");

    expect(hint).not.toBeNull();
    expect(hint?.className).toContain("bg-primary/10");
    expect(hint?.className).toContain("border-primary/20");
    expect(hint?.className).not.toContain("bg-purple-500/10");
    expect(hint?.className).not.toContain("border-purple-500/20");
    expect(hint?.className).not.toContain("text-purple-200/90");
  });

  it("creates a preview job before submitting a book import", async () => {
    openMock.mockResolvedValue("/tmp/reference.epub");
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_material_import_cmd") return Promise.resolve({ job: { id: "job-book" }, duplicates: { duplicate: false, matches: [] } });
      if (command === "import_book_cmd") return Promise.resolve({ id: "article-1" });
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
    render(<BookImportForm />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /选择 EPUB 或 PDF 文件/i }));
    await user.click(screen.getByRole("button", { name: /导入书籍/i }));
    await user.click(screen.getByRole("button", { name: "确认并导入" }));

    expect(invokeMock).toHaveBeenCalledWith("import_book_cmd", {
      filePath: "/tmp/reference.epub",
      title: "reference",
      importJobId: "job-book",
      duplicatePolicy: "keep_copy",
    });
  });
});

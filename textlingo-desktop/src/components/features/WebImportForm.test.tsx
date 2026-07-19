import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { WebImportForm } from "./WebImportForm";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: string | Record<string, unknown>) => typeof options === "string" ? options : key,
  }),
}));

describe("WebImportForm", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "fetch_url_content") return Promise.resolve({ title: "Remote article", content: "A sufficiently long article body." });
      if (command === "preview_material_import_cmd") return Promise.resolve({ job: { id: "job-web" }, duplicates: { duplicate: false, matches: [] } });
      if (command === "import_web_material_cmd") return Promise.resolve({ id: "article-1" });
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
  });

  afterEach(cleanup);

  it("previews fetched content and submits the resulting import job", async () => {
    render(<WebImportForm onCancel={() => {}} />);
    const user = userEvent.setup();
    await user.type(screen.getAllByRole("textbox")[0], "https://example.com/article");
    await user.click(screen.getByRole("button", { name: "webImport.fetchPreview" }));
    await user.click(await screen.findByRole("button", { name: "webImport.import" }));
    await user.click(screen.getByRole("button", { name: "确认并导入" }));

    expect(invokeMock).toHaveBeenCalledWith("import_web_material_cmd", {
      url: "https://example.com/article",
      title: "Remote article",
      content: "A sufficiently long article body.",
      importJobId: "job-web",
      duplicatePolicy: "keep_copy",
    });
  });
});

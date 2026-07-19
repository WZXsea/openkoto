import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { NewArticleForm } from "./NewArticleForm";

const invokeMock = vi.fn();
const getApiClientMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("../../lib/api", () => ({
  getApiClient: (...args: unknown[]) => getApiClientMock(...args),
}));

describe("NewArticleForm URL fetching", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    getApiClientMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") return Promise.resolve(null);
      if (command === "fetch_url_content") {
        return Promise.resolve({
          title: "Fetched title",
          content: "Fetched article content.",
        });
      }
      return Promise.resolve(null);
    });
    getApiClientMock.mockReturnValue({
      isBackendConfigured: () => false,
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("enables local remote URL fetching", async () => {
    render(<NewArticleForm onCancel={() => {}} />);

    await userEvent.type(
      screen.getByPlaceholderText("newArticle.sourceUrlPlaceholder"),
      "https://example.com/article",
    );
    await userEvent.click(screen.getByRole("button", { name: "newArticle.fetch" }));

    expect(screen.getByRole("button", { name: "newArticle.fetch" })).toBeEnabled();
    expect(invokeMock).toHaveBeenCalledWith("fetch_url_content", {
      url: "https://example.com/article",
    });
    expect(getApiClientMock).toHaveBeenCalled();
  });

  it("creates a preview job and waits for confirmation before creating an article", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") return Promise.resolve(null);
      if (command === "preview_material_import_cmd") {
        return Promise.resolve({
          title: "Manual lesson",
          source_uri: "https://example.com/manual",
          content_snippet: "Manual article content for preview.",
          file: {},
          paragraph_count: 1,
          duplicates: { duplicate: false, matches: [] },
          job: { id: "job-manual" },
        });
      }
      if (command === "create_article") return Promise.resolve({ id: "article-1" });
      return Promise.resolve(null);
    });
    render(<NewArticleForm onCancel={() => {}} />);
    const user = userEvent.setup();

    await user.type(screen.getByPlaceholderText("newArticle.titlePlaceholder"), "Manual lesson");
    await user.type(screen.getByPlaceholderText("newArticle.sourceUrlPlaceholder"), "https://example.com/manual");
    await user.type(screen.getByPlaceholderText("newArticle.contentPlaceholder"), "Manual article content for preview.");
    await user.click(screen.getByRole("button", { name: "newArticle.createArticle" }));

    expect(invokeMock).toHaveBeenCalledWith("preview_material_import_cmd", {
      request: {
        source_kind: "article",
        source_uri: "https://example.com/manual",
        content: "Manual article content for preview.",
        file_path: undefined,
        title: "Manual lesson",
      },
    });
    expect(invokeMock).not.toHaveBeenCalledWith("create_article", expect.anything());

    await user.click(screen.getByRole("button", { name: "确认并导入" }));
    expect(invokeMock).toHaveBeenCalledWith("create_article", {
      title: "Manual lesson",
      content: "Manual article content for preview.",
      sourceUrl: "https://example.com/manual",
      importJobId: "job-manual",
      duplicatePolicy: "keep_copy",
    });
  });

  it("updates an existing article directly without creating an import job", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") return Promise.resolve(null);
      if (command === "update_article") return Promise.resolve({ id: "article-1" });
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
    render(
      <NewArticleForm
        onCancel={() => {}}
        initialArticle={{ id: "article-1", title: "Existing", content: "Existing content", created_at: "2026-07-11T00:00:00Z", translated: false }}
      />,
    );
    const user = userEvent.setup();

    await user.clear(screen.getByPlaceholderText("newArticle.contentPlaceholder"));
    await user.type(screen.getByPlaceholderText("newArticle.contentPlaceholder"), "Updated article content");
    await user.click(screen.getByRole("button", { name: "newArticle.createArticle" }));

    expect(invokeMock).toHaveBeenCalledWith("update_article", {
      id: "article-1",
      title: "Existing",
      content: "Updated article content",
      sourceUrl: undefined,
    });
    expect(invokeMock).not.toHaveBeenCalledWith("preview_material_import_cmd", expect.anything());
  });
});

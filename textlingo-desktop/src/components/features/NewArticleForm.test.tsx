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

describe("NewArticleForm phase 1 boundaries", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    getApiClientMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it("keeps remote URL fetching disabled during phase 1", async () => {
    render(<NewArticleForm onCancel={() => {}} />);

    await userEvent.type(
      screen.getByPlaceholderText("newArticle.sourceUrlPlaceholder"),
      "https://example.com/article",
    );

    expect(screen.getByRole("button", { name: "newArticle.fetch" })).toBeDisabled();
    expect(invokeMock).not.toHaveBeenCalledWith("fetch_url_content", expect.anything());
    expect(getApiClientMock).not.toHaveBeenCalled();
  });
});

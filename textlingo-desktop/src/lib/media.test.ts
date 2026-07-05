import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { buildMediaResourceUrl, clearResourceServerInfoCacheForTests } from "./media";

describe("media resource urls", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    clearResourceServerInfoCacheForTests();
  });

  it("builds tokenized resource server URLs and caches server info", async () => {
    invokeMock.mockResolvedValue({
      base_url: "http://127.0.0.1:19420/",
      token: "token value",
    });

    await expect(buildMediaResourceUrl("/tmp/sample video.mp4", "video")).resolves.toBe(
      "http://127.0.0.1:19420/resource/token%20value/video/sample%20video.mp4",
    );
    await expect(buildMediaResourceUrl("/tmp/book.pdf", "book")).resolves.toBe(
      "http://127.0.0.1:19420/resource/token%20value/book/book.pdf",
    );

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("get_resource_server_info_cmd");
  });

  it("keeps existing http URLs unchanged", async () => {
    await expect(buildMediaResourceUrl("https://example.com/video.mp4", "video")).resolves.toBe(
      "https://example.com/video.mp4",
    );

    expect(invokeMock).not.toHaveBeenCalled();
  });
});

import { beforeEach, describe, expect, it, vi } from "vitest";

import { createHomeActivityApi } from "./api";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

beforeEach(() => invokeMock.mockReset());

describe("home activity api", () => {
  it("invokes the PR-11 command and normalizes invalid counts", async () => {
    invokeMock.mockResolvedValue({
      start_date: "2026-04-22",
      end_date: "2026-07-14",
      days: [{ date: "2026-07-14", read_materials: 2, learning_actions: -1, activity_score: Number.NaN }],
    });
    const query = { start_date: "2026-04-22", end_date: "2026-07-14", timezone_offset_minutes: 480 };
    await expect(createHomeActivityApi().getActivityHeatmap(query)).resolves.toEqual(expect.objectContaining({
      days: [{ date: "2026-07-14", read_materials: 2, learning_actions: 0, activity_score: 0 }],
    }));
    expect(invokeMock).toHaveBeenCalledWith("get_learning_activity_heatmap_cmd", { query });
  });
});

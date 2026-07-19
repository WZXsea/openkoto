import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Annotation } from "../../types";
import { AnnotationWorkbench } from "./AnnotationWorkbench";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function createAnnotation(overrides: Partial<Annotation> = {}): Annotation {
  return {
    id: "annotation-1",
    material_id: "material-1",
    segment_id: "segment-1",
    kind: "vocabulary",
    locator: {
      version: 1,
      kind: "text_range",
      segment_id: "segment-1",
      start_offset: 5,
      end_offset: 13,
      quote: { exact: "mitigate" },
    },
    source_text: "mitigate",
    color: "#facc15",
    note: "初始笔记",
    tags: ["academic"],
    created_at: "2026-07-14T00:00:00Z",
    updated_at: "2026-07-14T00:00:00Z",
    ...overrides,
  };
}

describe("AnnotationWorkbench", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string, payload?: Record<string, unknown>) => {
      if (command === "list_annotations_cmd") return Promise.resolve([createAnnotation()]);
      if (command === "update_annotation_cmd") return Promise.resolve({ ...createAnnotation(), ...(payload?.payload as object) });
      if (command === "delete_annotation_cmd") return Promise.resolve(undefined);
      if (command === "convert_annotation_to_learning_item_cmd") {
        return Promise.resolve({
          annotation: createAnnotation({ learning_item_id: "learning-1" }),
          learning_item: { id: "learning-1", text: "mitigate" },
        });
      }
      return Promise.resolve(undefined);
    });
  });

  afterEach(cleanup);

  it("passes material, type, tag, keyword, and time filters to the list command", async () => {
    render(<AnnotationWorkbench materials={[{ id: "material-1", title: "Academic reading" }]} onNavigateToSource={() => {}} />);
    await screen.findByText("mitigate");
    const user = userEvent.setup();

    await user.selectOptions(screen.getByLabelText("筛选素材"), "material-1");
    await user.selectOptions(screen.getByLabelText("筛选类型"), "vocabulary");
    await user.selectOptions(screen.getByLabelText("筛选标签"), "academic");
    await user.type(screen.getByLabelText("筛选关键词"), "mitigate");
    fireEvent.change(screen.getByLabelText("筛选起始时间"), { target: { value: "2026-07-01T08:00" } });
    fireEvent.change(screen.getByLabelText("筛选截止时间"), { target: { value: "2026-07-31T20:00" } });

    await waitFor(() => {
      expect(invokeMock).toHaveBeenLastCalledWith("list_annotations_cmd", {
        query: {
          material_id: "material-1",
          kind: "vocabulary",
          tag: "academic",
          q: "mitigate",
          created_after: new Date("2026-07-01T08:00").toISOString(),
          created_before: new Date("2026-07-31T20:00").toISOString(),
          limit: 200,
          offset: 0,
        },
      });
    });
  });

  it("edits annotation fields, signals locator state, and invokes source navigation", async () => {
    const onNavigateToSource = vi.fn();
    render(<AnnotationWorkbench onNavigateToSource={onNavigateToSource} />);
    await screen.findByText("精确定位");
    const user = userEvent.setup();

    await user.clear(screen.getByLabelText("mitigate 笔记"));
    await user.type(screen.getByLabelText("mitigate 笔记"), "需要复习");
    await user.clear(screen.getByLabelText("mitigate 标签"));
    await user.type(screen.getByLabelText("mitigate 标签"), "academic, verbs");
    fireEvent.change(screen.getByLabelText("mitigate 颜色"), { target: { value: "#22c55e" } });
    await user.click(screen.getByLabelText("保存批注 mitigate"));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_annotation_cmd", {
        id: "annotation-1",
        payload: { note: "需要复习", color: "#22c55e", tags: ["academic", "verbs"] },
      });
    });
    await user.click(screen.getByLabelText("回到原文 mitigate"));
    expect(onNavigateToSource).toHaveBeenCalledWith(expect.objectContaining({ id: "annotation-1" }));
  });

  it("converts an annotation and deletes it from the workbench", async () => {
    const onConverted = vi.fn();
    render(<AnnotationWorkbench onNavigateToSource={() => {}} onConverted={onConverted} />);
    await screen.findByText("mitigate");
    const user = userEvent.setup();

    await user.click(screen.getByLabelText("转换为学习候选 mitigate"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("convert_annotation_to_learning_item_cmd", { id: "annotation-1" });
      expect(onConverted).toHaveBeenCalledWith(expect.objectContaining({ learning_item_id: "learning-1" }));
    });
    await user.click(screen.getByLabelText("删除批注 mitigate"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("delete_annotation_cmd", { id: "annotation-1" });
      expect(screen.queryByTestId("annotation-annotation-1")).not.toBeInTheDocument();
    });
  });

  it("shows invalid locators explicitly and disables source navigation", async () => {
    invokeMock.mockImplementation((command: string) => command === "list_annotations_cmd"
      ? Promise.resolve([createAnnotation({ locator: { kind: "unknown" } as unknown as Annotation["locator"] })])
      : Promise.resolve(undefined));

    render(<AnnotationWorkbench onNavigateToSource={() => {}} />);
    expect(await screen.findByText("定位失效")).toBeInTheDocument();
    expect(screen.getByLabelText("回到原文 mitigate")).toBeDisabled();
  });
});

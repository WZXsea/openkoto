import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { MaterialImportActivityProvider, useMaterialImportPreview } from "./useMaterialImportPreview";
import { cancelMaterialImportJob, previewMaterialImport } from "./api";

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    previewMaterialImport: vi.fn(),
    cancelMaterialImportJob: vi.fn(),
  };
});

describe("useMaterialImportPreview", () => {
  beforeEach(() => {
    vi.mocked(previewMaterialImport).mockReset();
    vi.mocked(cancelMaterialImportJob).mockReset();
  });

  it("serializes commit and cancel operations and reports parent activity", async () => {
    vi.mocked(previewMaterialImport).mockResolvedValue({
      jobId: "job-1",
      title: "Concurrency lesson",
      paragraphCount: 1,
      file: {},
      duplicates: [],
    });
    let resolveCommit!: (value: { id: string }) => void;
    const commit = vi.fn(() => new Promise<{ id: string }>((resolve) => { resolveCommit = resolve; }));
    const onSuccess = vi.fn();
    const onError = vi.fn();
    const onBusyChange = vi.fn();
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <MaterialImportActivityProvider onBusyChange={onBusyChange}>{children}</MaterialImportActivityProvider>
    );
    const { result } = renderHook(
      () => useMaterialImportPreview({ commit, onSuccess, onError }),
      { wrapper },
    );

    await act(async () => {
      await result.current.startPreview({ sourceKind: "article", content: "Test content" });
    });

    let firstCommit!: Promise<void>;
    act(() => {
      firstCommit = result.current.confirmPreview();
      void result.current.confirmPreview();
      void result.current.cancelPreview();
    });
    expect(commit).toHaveBeenCalledOnce();
    expect(cancelMaterialImportJob).not.toHaveBeenCalled();

    await act(async () => {
      resolveCommit({ id: "material-1" });
      await firstCommit;
    });
    expect(onSuccess).toHaveBeenCalledOnce();
    expect(onError).not.toHaveBeenCalled();
    expect(onBusyChange.mock.calls).toEqual([[true], [false], [true], [false]]);
  });
});

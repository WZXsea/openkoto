import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ImportPreviewDialog } from "./ImportPreviewDialog";
import { Dialog } from "../../components/ui/dialog";

describe("ImportPreviewDialog", () => {
  afterEach(cleanup);

  it("shows the preview fields and delegates confirmation", async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(
      <ImportPreviewDialog
        isOpen
        preview={{
          jobId: "job-1",
          title: "Remote lesson",
          sourceUri: "https://example.com/lesson",
          contentSnippet: "A focused preview of the imported lesson.",
          paragraphCount: 4,
          file: {},
          duplicates: [],
        }}
        onConfirm={onConfirm}
        onCancel={() => {}}
      />,
    );

    expect(screen.getByText("Remote lesson")).toBeInTheDocument();
    expect(screen.getByText("https://example.com/lesson")).toBeInTheDocument();
    expect(screen.getByText("A focused preview of the imported lesson.")).toBeInTheDocument();
    expect(screen.getByText("识别段落：4")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "确认并导入" }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("shows file metadata when no content snippet is available and delegates cancellation", async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <ImportPreviewDialog
        isOpen
        preview={{
          jobId: "job-file",
          title: "lecture.mp4",
          sourceUri: "file:///tmp/lecture.mp4",
          paragraphCount: 0,
          file: { fileName: "lecture.mp4", byteSize: 1_572_864, sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" },
          duplicates: [],
        }}
        onConfirm={() => {}}
        onCancel={onCancel}
      />,
    );

    expect(screen.getAllByText("lecture.mp4")).toHaveLength(2);
    expect(screen.getByText("大小：1.5 MB")).toBeInTheDocument();
    expect(screen.getByText(/SHA-256：0123456789ab/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("disables confirmation while an import is busy", async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <ImportPreviewDialog
        isOpen
        isBusy
        preview={{
          jobId: "job-busy",
          title: "Busy import",
          paragraphCount: 1,
          file: {},
          duplicates: [],
        }}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />,
    );

    const confirmButton = screen.getByRole("button", { name: "确认并导入" });
    expect(confirmButton).toBeDisabled();
    expect(confirmButton).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    await user.click(confirmButton);
    await user.keyboard("{Escape}");
    expect(onConfirm).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("lets only the topmost nested dialog handle Escape", async () => {
    const onOuterClose = vi.fn();
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <Dialog isOpen onClose={onOuterClose} title="Outer dialog">
        <ImportPreviewDialog
          isOpen
          preview={{ jobId: "job-nested", title: "Nested preview", paragraphCount: 1, file: {}, duplicates: [] }}
          onConfirm={() => {}}
          onCancel={onCancel}
        />
      </Dialog>,
    );

    await user.keyboard("{Escape}");
    expect(onCancel).toHaveBeenCalledOnce();
    expect(onOuterClose).not.toHaveBeenCalled();
  });
});

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DuplicateResolutionDialog } from "./DuplicateResolutionDialog";

describe("DuplicateResolutionDialog", () => {
  afterEach(cleanup);

  it("emits every explicit duplicate resolution action", async () => {
    const actions = [
      ["取消", "cancel"],
      ["打开已有", "open_existing"],
      ["替换", "replace"],
      ["保留副本", "keep_copy"],
    ] as const;

    for (const [label, action] of actions) {
      const onResolve = vi.fn();
      const user = userEvent.setup();
      const view = render(<DuplicateResolutionDialog isOpen duplicate={{ materialId: "existing", title: "Existing material", matchedBy: ["content_sha256"] }} onResolve={onResolve} />);
      await user.click(screen.getByRole("button", { name: label }));
      expect(onResolve).toHaveBeenCalledWith(action);
      view.unmount();
    }
  });
});

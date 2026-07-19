import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Article } from "../../../types";
import { ArticleReaderHeader } from "./ArticleReaderHeader";

afterEach(cleanup);

const ARTICLE: Article = {
  id: "article-1",
  title: "Accessible Reader",
  content: "Evidence.",
  source_type: "article",
  created_at: "2026-07-15T08:00:00Z",
  translated: false,
  segments: [{
    id: "segment-1",
    article_id: "article-1",
    order: 0,
    text: "Evidence.",
    created_at: "2026-07-15T08:00:00Z",
  }],
};

describe("ArticleReaderHeader", () => {
  it("keeps icon-only reader controls keyboard-addressable with stable names", async () => {
    const onBack = vi.fn();
    const onPrev = vi.fn();
    const onNext = vi.fn();
    const onFontSizeChange = vi.fn();
    const onToggleAssistant = vi.fn();

    render(
      <ArticleReaderHeader
        article={ARTICLE}
        segments={ARTICLE.segments ?? []}
        hasSegments
        onBack={onBack}
        onPrev={onPrev}
        onNext={onNext}
        hasPrev
        hasNext
        fontSize={18}
        onFontSizeChange={onFontSizeChange}
        viewMode="original"
        onViewModeChange={vi.fn()}
        isBatchTranslating={false}
        batchProgress={{ current: 0, total: 1 }}
        canUseAi
        aiUnavailableMessage="AI unavailable"
        onBatchTranslate={vi.fn()}
        isResegmenting={false}
        onResegment={vi.fn()}
        onOpenCandidateBox={vi.fn()}
        onArticleExport={vi.fn()}
        isEditing={false}
        onToggleEditing={vi.fn()}
        isTranslating={false}
        translationProgress={null}
        onTranslate={vi.fn()}
        showAssistant
        onToggleAssistant={onToggleAssistant}
      />,
    );

    expect(screen.getByRole("heading", { name: "Accessible Reader" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Previous Article" }));
    await userEvent.click(screen.getByRole("button", { name: "Next Article" }));
    await userEvent.click(screen.getByRole("button", { name: "Decrease font size" }));
    await userEvent.click(screen.getByRole("button", { name: "Hide Assistant" }));

    expect(onPrev).toHaveBeenCalledTimes(1);
    expect(onNext).toHaveBeenCalledTimes(1);
    expect(onFontSizeChange).toHaveBeenCalledWith(16);
    expect(onToggleAssistant).toHaveBeenCalledTimes(1);
  });
});

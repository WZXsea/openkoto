import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { AssistantArtifact } from "../../features/assistant";
import { AssistantArtifactViewer } from "./AssistantArtifactViewer";

afterEach(cleanup);

function artifact(overrides: Partial<AssistantArtifact>): AssistantArtifact {
  return {
    id: "artifact-1",
    task_id: "task-1",
    article_id: "article-1",
    artifact_type: "unknown",
    version: "1",
    content: null,
    metadata: {},
    created_at: "2026-07-15T08:00:00Z",
    updated_at: "2026-07-15T08:00:00Z",
    ...overrides,
  };
}

describe("AssistantArtifactViewer", () => {
  it("renders mind map trees and structured markdown", () => {
    const { rerender } = render(<AssistantArtifactViewer artifact={artifact({
      artifact_type: "mind_map",
      content: { root: { title: "Main claim", summary: "Evidence summary", children: [{ title: "Evidence A", children: [] }] } },
    })} />);
    expect(screen.getByText("Main claim")).toBeInTheDocument();
    expect(screen.getByText("Evidence A")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    rerender(<AssistantArtifactViewer artifact={artifact({ artifact_type: "structured_report", content: { markdown: "## Final report" } })} />);
    expect(screen.getByRole("heading", { name: "Final report" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps missing files and unknown JSON artifacts inspectable", () => {
    const { rerender } = render(<AssistantArtifactViewer artifact={artifact({
      artifact_type: "file",
      metadata: { file_name: "missing.md" },
      file_available: false,
    })} />);
    expect(screen.getByRole("alert")).toHaveTextContent("产物文件不可用");
    expect(screen.getByText(/missing\.md/)).toBeInTheDocument();

    rerender(<AssistantArtifactViewer artifact={artifact({ artifact_type: "custom_json", content: { confidence: 0.93 } })} />);
    expect(screen.getByText(/"confidence": 0\.93/)).toBeInTheDocument();
  });
});

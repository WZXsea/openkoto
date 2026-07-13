import { describe, expect, it } from "vitest";

import type { Article } from "../../types";
import {
  filterAndSortMaterials,
  getContinueReadingMaterials,
} from "./selectors";
import { DEFAULT_MATERIAL_FILTERS } from "./types";

const articles: Article[] = [
  {
    id: "web-in-progress",
    title: "Clinical trial overview",
    content: "Study protocol for oncology reading.",
    source_type: "web",
    source_url: "https://example.com/trial",
    created_at: "2026-07-01T08:00:00Z",
    translated: false,
    tags: ["oncology", "review"],
    reading_progress: { progress_ratio: 0.45, status: "reading", last_opened_at: "2026-07-08T08:00:00Z" },
  } as Article,
  {
    id: "book-completed",
    title: "English writing handbook",
    content: "A book about writing.",
    source_type: "book",
    created_at: "2026-07-05T08:00:00Z",
    translated: true,
    tags: ["writing"],
    reading_progress: { progress_ratio: 1, completed_at: "2026-07-09T08:00:00Z" },
  } as Article,
  {
    id: "text-recent",
    title: "Journal notes",
    content: "Recent notes on study design.",
    source_type: "text_file",
    created_at: "2026-07-09T09:00:00Z",
    translated: false,
    tags: [{ name: "oncology" }],
    progress_ratio: 0.2,
    status: "in_progress",
    last_opened_at: "2026-07-10T08:00:00Z",
  } as Article,
];

describe("material selectors", () => {
  it("searches titles, sources, content, and tags", () => {
    expect(filterAndSortMaterials(articles, { ...DEFAULT_MATERIAL_FILTERS, query: "oncology" })
      .map((article) => article.id)).toEqual(["text-recent", "web-in-progress"]);
  });

  it("combines type, reading status, and tag filters", () => {
    expect(filterAndSortMaterials(articles, {
      ...DEFAULT_MATERIAL_FILTERS,
      type: "web",
      readingStatus: "in_progress",
      tag: "oncology",
    }).map((article) => article.id)).toEqual(["web-in-progress"]);
  });

  it("sorts by selected material field", () => {
    expect(filterAndSortMaterials(articles, { ...DEFAULT_MATERIAL_FILTERS, sort: "title" })
      .map((article) => article.id)).toEqual(["web-in-progress", "book-completed", "text-recent"]);
    expect(filterAndSortMaterials(articles, { ...DEFAULT_MATERIAL_FILTERS, sort: "progress" })
      .map((article) => article.id)).toEqual(["book-completed", "web-in-progress", "text-recent"]);
  });

  it("filters by inclusive import date range", () => {
    expect(filterAndSortMaterials(articles, {
      ...DEFAULT_MATERIAL_FILTERS,
      createdFrom: "2026-07-05",
      createdTo: "2026-07-08",
    }).map((article) => article.id)).toEqual(["book-completed"]);
  });

  it("returns recently opened in-progress materials for continue reading", () => {
    expect(getContinueReadingMaterials(articles).map((article) => article.id))
      .toEqual(["text-recent", "web-in-progress"]);
  });
});

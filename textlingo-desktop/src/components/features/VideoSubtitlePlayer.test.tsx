import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { VideoSubtitlePlayer } from "./VideoSubtitlePlayer";
import type { ArticleSegment } from "../../types";

const saveMock = vi.fn();
const invokeMock = vi.fn();
const localStorageStore = new Map<string, string>();

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => saveMock(...args),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function createSegment(overrides: Partial<ArticleSegment> = {}): ArticleSegment {
  return {
    id: "seg-1",
    article_id: "article-1",
    order: 0,
    text: "Alpha",
    translation: "Beta",
    reading_text: "Alpha reading",
    start_time: 0,
    end_time: 2,
    created_at: "2026-03-30T00:00:00Z",
    is_new_paragraph: true,
    ...overrides,
  };
}

describe("VideoSubtitlePlayer", () => {
  beforeEach(() => {
    saveMock.mockReset();
    invokeMock.mockReset();
    localStorageStore.clear();

    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: (key: string) => localStorageStore.get(key) ?? null,
        setItem: (key: string, value: string) => {
          localStorageStore.set(key, value);
        },
        removeItem: (key: string) => {
          localStorageStore.delete(key);
        },
      },
      configurable: true,
    });

    Object.defineProperty(HTMLMediaElement.prototype, "play", {
      configurable: true,
      value: vi.fn().mockResolvedValue(undefined),
    });
    Object.defineProperty(HTMLMediaElement.prototype, "pause", {
      configurable: true,
      value: vi.fn(),
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("renders the view mode control next to the subtitle list actions", () => {
    render(
      <VideoSubtitlePlayer
        videoUrl="http://localhost/video.mp4"
        segments={[createSegment()]}
        selectedSegmentId={null}
        onSegmentClick={vi.fn()}
        fontSize={18}
        viewMode="original"
        onViewModeChange={vi.fn()}
      />
    );

    expect(screen.getByTestId("player-view-mode-trigger")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "videoPlayer.showAllSubtitles (1)" })).toBeInTheDocument();
  });

  it("allows switching the player view mode from the subtitle action row", async () => {
    const onViewModeChange = vi.fn();

    render(
      <VideoSubtitlePlayer
        videoUrl="http://localhost/video.mp4"
        segments={[createSegment()]}
        selectedSegmentId={null}
        onSegmentClick={vi.fn()}
        fontSize={18}
        viewMode="original"
        onViewModeChange={onViewModeChange}
      />
    );

    await userEvent.click(screen.getByTestId("player-view-mode-trigger"));
    await userEvent.click(screen.getByRole("menuitem", { name: "articleReader.viewMode.bilingual" }));

    expect(onViewModeChange).toHaveBeenCalledWith("bilingual");
  });

  it("shows bilingual subtitle content when the view mode is switched", () => {
    render(
      <VideoSubtitlePlayer
        videoUrl="http://localhost/video.mp4"
        segments={[createSegment()]}
        selectedSegmentId={null}
        onSegmentClick={vi.fn()}
        fontSize={18}
        viewMode="bilingual"
      />
    );

    const video = document.querySelector("video");
    expect(video).not.toBeNull();

    if (video) {
      Object.defineProperty(video, "currentTime", {
        configurable: true,
        value: 1,
        writable: true,
      });
      fireEvent.timeUpdate(video);
    }

    expect(screen.getByText("Beta")).toBeInTheDocument();
  });

  it("restores synced media progress ahead of legacy storage and reports a completed playback", () => {
    localStorageStore.set("textlingo_video_position_article-1", "2");
    const onProgressChange = vi.fn();
    render(
      <VideoSubtitlePlayer
        videoUrl="http://localhost/video.mp4"
        segments={[createSegment()]}
        selectedSegmentId={null}
        onSegmentClick={vi.fn()}
        fontSize={18}
        viewMode="original"
        articleId="article-1"
        initialProgress={{ reader_kind: "media", locator: { kind: "time", current_time: 12.5, duration: 100 }, progress_ratio: 0.125, status: "reading" }}
        onProgressChange={onProgressChange}
      />,
    );

    const video = document.querySelector("video") as HTMLVideoElement;
    Object.defineProperty(video, "duration", { configurable: true, value: 100 });
    Object.defineProperty(video, "currentTime", { configurable: true, writable: true, value: 0 });
    fireEvent.loadedMetadata(video);
    expect(video.currentTime).toBe(12.5);

    video.currentTime = 100;
    fireEvent.ended(video);
    expect(onProgressChange).toHaveBeenLastCalledWith(expect.objectContaining({
      reader_kind: "media",
      locator: { kind: "time", current_time: 100, duration: 100 },
      progress_ratio: 1,
      status: "completed",
    }));
  });
});

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
const saveMock = vi.fn();
const translations: Record<string, string> = {
  "ktvExport.title": "KTV Export",
  "ktvExport.back": "Back",
  "ktvExport.timelinePreview": "Timeline preview",
  "ktvExport.content": "Content",
  "ktvExport.display": "Display",
  "ktvExport.original": "Original",
  "ktvExport.bilingual": "Bilingual",
  "ktvExport.translation": "Translation",
  "ktvExport.showReadings": "Show readings",
  "ktvExport.readingsRequireTranslation": "Complete translation for the full video and make sure readings already exist before using readings.",
  "ktvExport.typography": "Typography",
  "ktvExport.originalFont": "Original font",
  "ktvExport.translationFont": "Translation font",
  "ktvExport.readingFont": "Reading font",
  "ktvExport.originalFontSize": "Original font size",
  "ktvExport.readingScale": "Reading scale",
  "ktvExport.lineGap": "Line gap",
  "ktvExport.bilingualGap": "Bilingual gap",
  "ktvExport.style": "Style",
  "ktvExport.originalColor": "Original color",
  "ktvExport.translationColor": "Translation color",
  "ktvExport.readingColor": "Reading color",
  "ktvExport.outlineColor": "Outline color",
  "ktvExport.outlineWidth": "Outline width",
  "ktvExport.enableShadow": "Enable shadow",
  "ktvExport.shadowColor": "Shadow color",
  "ktvExport.shadowBlur": "Shadow blur",
  "ktvExport.shadowOffsetX": "Shadow offset X",
  "ktvExport.shadowOffsetY": "Shadow offset Y",
  "ktvExport.layout": "Layout",
  "ktvExport.subtitlePosition": "Subtitle position",
  "ktvExport.positionBottom": "Bottom",
  "ktvExport.positionLowerThird": "Lower third",
  "ktvExport.positionCenterLower": "Center lower",
  "ktvExport.bottomMargin": "Bottom margin",
  "ktvExport.horizontalMargin": "Horizontal margin",
  "ktvExport.exportSection": "Export",
  "ktvExport.exportHint": "MP4 hard subtitle output via FFmpeg",
  "ktvExport.export": "Export",
  "ktvExport.exporting": "Exporting...",
  "ktvExport.exportRequiresTimedSegments": "Timed subtitle segments are required before exporting.",
  "ktvExport.exportCompleted": "Export completed",
  "ktvExport.exportFailed": "Export failed",
  "ktvExport.noVideoSource": "No video source available",
  "ktvExport.noSubtitleSegments": "No subtitle segments available",
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => saveMock(...args),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => translations[key] ?? key,
  }),
}));

import { KtvExportPage } from "./KtvExportPage";

describe("KtvExportPage", () => {
  beforeEach(() => {
    const storage = new Map<string, string>();
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      value: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => {
          storage.set(key, value);
        },
        removeItem: (key: string) => {
          storage.delete(key);
        },
        clear: () => {
          storage.clear();
        },
      },
    });
    invokeMock.mockReset();
    saveMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_resource_server_info_cmd") {
        return Promise.resolve({
          base_url: "http://127.0.0.1:19420",
          token: "test-token",
        });
      }
      return Promise.resolve(undefined);
    });
    saveMock.mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
  });

  it("updates the preview when display mode and reading visibility change", async () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          explanation: {
            translation: "你好",
            explanation: "Greeting",
            reading_text: "コンニチハ",
          },
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    render(<KtvExportPage article={article} onBack={() => {}} />);

    expect(screen.getAllByText("こんにちは").length).toBeGreaterThan(0);
    expect(screen.getByText("（コンニチハ）")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("Show readings"));
    expect(screen.queryByText("（コンニチハ）")).not.toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("Bilingual"));
    expect(screen.getAllByText("你好").length).toBeGreaterThan(0);
  });

  it("supports translation-only mode and hides readings controls", async () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          explanation: {
            translation: "你好",
            explanation: "Greeting",
            reading_text: "コンニチハ",
          },
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    render(<KtvExportPage article={article} onBack={() => {}} />);

    await userEvent.click(screen.getByLabelText("Translation"));

    expect(screen.getAllByText("你好").length).toBeGreaterThan(0);
    expect(screen.queryByText("（コンニチハ）")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Show readings")).not.toBeInTheDocument();
  });

  it("reuses the tokenized resource server playback url and restores saved position", async () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/folder/sample video.mp4",
      media_path: "/tmp/folder/sample video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          explanation: {
            translation: "你好",
            explanation: "Greeting",
            reading_text: "コンニチハ",
          },
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    localStorage.setItem(
      "textlingo_video_position_video-1",
      "12.5",
    );

    render(<KtvExportPage article={article} onBack={() => {}} />);

    const video = await screen.findByLabelText("KTV video preview") as HTMLVideoElement;
    await waitFor(() => {
      expect(video.getAttribute("src")).toBe("http://127.0.0.1:19420/resource/test-token/video/sample%20video.mp4");
    });

    fireEvent.loadedMetadata(video);
    expect(video.currentTime).toBe(12.5);

    video.currentTime = 15;
    fireEvent.pause(video);
    expect(localStorage.getItem("textlingo_video_position_video-1")).toBe("15");
  });

  it("prepares ktv segments on mount and renders returned readings", async () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: undefined,
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    invokeMock.mockResolvedValue({
      ...article,
      segments: [
        {
          ...article.segments[0],
          reading_text: "こんにちは",
        },
      ],
    });

    render(<KtvExportPage article={article} onBack={() => {}} />);

    expect(invokeMock).toHaveBeenCalledWith("prepare_ktv_segments_cmd", {
      articleId: "video-1",
      languageHint: "ja",
    });

    await waitFor(() => {
      expect(screen.getAllByText("こんにちは").length).toBeGreaterThan(1);
    });
  });

  it("disables readings until every segment is translated", () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          translation: undefined,
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    render(<KtvExportPage article={article} onBack={() => {}} />);

    expect(screen.getByLabelText("Show readings")).toBeDisabled();
    expect(
      screen.getByText("Complete translation for the full video and make sure readings already exist before using readings."),
    ).toBeInTheDocument();
    expect(screen.queryByText("（コンニチハ）")).not.toBeInTheDocument();
  });

  it("enables readings when every segment is translated and has reading text", () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: true,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    render(<KtvExportPage article={article} onBack={() => {}} />);

    expect(screen.getByLabelText("Show readings")).not.toBeDisabled();
    expect(screen.queryByText("Complete translation for the full video and make sure readings already exist before using readings.")).not.toBeInTheDocument();
    expect(screen.getByText("（コンニチハ）")).toBeInTheDocument();
  });

  it("allows readings when untranslated annotation is unnecessary for kana-only segments", () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: true,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "そうだろう。",
          reading_text: "そうだろう。",
          translation: "是那样吧。",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    render(<KtvExportPage article={article} onBack={() => {}} />);

    expect(screen.getByLabelText("Show readings")).not.toBeDisabled();
    expect(screen.queryByText("Complete translation for the full video and make sure readings already exist before using readings.")).not.toBeInTheDocument();
  });

  it("renders inline readings from vocabulary entries when sentence reading is unavailable", () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: true,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "氷は全部溶けた",
          reading_text: "氷は全部溶けた",
          translation: "冰都融化了",
          explanation: {
            translation: "冰都融化了",
            explanation: "Line explanation",
            reading_text: undefined,
            vocabulary: [
              { word: "氷", meaning: "ice", usage: "", example: undefined, reading: "こおり" },
              { word: "溶けた", meaning: "melted", usage: "", example: undefined, reading: "とけた" },
            ],
            grammar_points: [],
          },
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    render(<KtvExportPage article={article} onBack={() => {}} />);

    expect(screen.getByLabelText("Show readings")).not.toBeDisabled();
    expect(screen.getByText("（こおり）")).toBeInTheDocument();
    expect(screen.getByText("（とけた）")).toBeInTheDocument();
  });

  it("exports the current video with the current config", async () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          explanation: {
            translation: "你好",
            explanation: "Greeting",
            reading_text: "コンニチハ",
          },
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    saveMock.mockResolvedValue("/tmp/output.mp4");
    invokeMock.mockImplementation((command: string) => {
      if (command === "prepare_ktv_segments_cmd") {
        return Promise.resolve(article);
      }

      if (command === "export_ktv_video_cmd") {
        return Promise.resolve({ outputPath: "/tmp/output.mp4" });
      }

      return Promise.resolve(undefined);
    });

    render(<KtvExportPage article={article} onBack={() => {}} />);

    const video = await screen.findByLabelText("KTV video preview") as HTMLVideoElement;
    Object.defineProperty(video, "videoWidth", {
      configurable: true,
      value: 640,
    });
    Object.defineProperty(video, "videoHeight", {
      configurable: true,
      value: 360,
    });
    fireEvent.loadedMetadata(video);

    await userEvent.click(screen.getByLabelText("Bilingual"));
    await userEvent.clear(screen.getByLabelText("Original font size"));
    await userEvent.type(screen.getByLabelText("Original font size"), "56");
    await userEvent.selectOptions(screen.getByLabelText("Subtitle position"), "center_lower");
    await userEvent.clear(screen.getByLabelText("Bottom margin"));
    await userEvent.type(screen.getByLabelText("Bottom margin"), "72");
    await userEvent.clear(screen.getByLabelText("Original color"));
    await userEvent.type(screen.getByLabelText("Original color"), "#00FF88");
    await userEvent.click(screen.getByLabelText("Enable shadow"));

    await userEvent.click(screen.getByRole("button", { name: "Export" }));

    expect(saveMock).toHaveBeenCalled();
    expect(invokeMock).toHaveBeenLastCalledWith(
      "export_ktv_video_cmd",
      expect.objectContaining({
        articleId: "video-1",
        outputPath: "/tmp/output.mp4",
        config: expect.objectContaining({
          displayMode: "bilingual",
          showReading: true,
          fontSize: 56,
          positionPreset: "center_lower",
          bottomMargin: 72,
          originalColor: "#00FF88",
          shadowEnabled: false,
          videoWidth: 640,
          videoHeight: 360,
        }),
      }),
    );

    expect(await screen.findByText("Export completed")).toBeInTheDocument();
    expect(screen.getByText("/tmp/output.mp4")).toBeInTheDocument();
  });

  it("exports translation-only mode without readings", async () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          explanation: {
            translation: "你好",
            explanation: "Greeting",
            reading_text: "コンニチハ",
          },
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    saveMock.mockResolvedValue("/tmp/output.mp4");
    invokeMock.mockImplementation((command: string) => {
      if (command === "prepare_ktv_segments_cmd") {
        return Promise.resolve(article);
      }

      if (command === "export_ktv_video_cmd") {
        return Promise.resolve({ outputPath: "/tmp/output.mp4" });
      }

      return Promise.resolve(undefined);
    });

    render(<KtvExportPage article={article} onBack={() => {}} />);

    await userEvent.click(screen.getByLabelText("Translation"));
    await userEvent.click(screen.getByRole("button", { name: "Export" }));

    expect(invokeMock).toHaveBeenCalledWith(
      "export_ktv_video_cmd",
      expect.objectContaining({
        articleId: "video-1",
        outputPath: "/tmp/output.mp4",
        config: expect.objectContaining({
          displayMode: "translation",
          showReading: false,
        }),
      }),
    );
  });

  it("shows an export error when ffmpeg export fails", async () => {
    const article = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video" as const,
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: undefined,
      book_type: undefined,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: undefined,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    saveMock.mockResolvedValue("/tmp/output.mp4");
    invokeMock.mockImplementation((command: string) => {
      if (command === "prepare_ktv_segments_cmd") {
        return Promise.resolve(article);
      }

      if (command === "export_ktv_video_cmd") {
        return Promise.reject(new Error("ffmpeg failed"));
      }

      return Promise.resolve(undefined);
    });

    render(<KtvExportPage article={article} onBack={() => {}} />);

    await userEvent.click(screen.getByRole("button", { name: "Export" }));

    expect(await screen.findByText("Export failed")).toBeInTheDocument();
    expect(screen.getByText("ffmpeg failed")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Export" })).toBeEnabled();
    });
  });
});

import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Fragment, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Label } from "../ui/label";
import { Select } from "../ui/select";
import { cn } from "../../lib/utils";
import { buildMediaResourceUrl, buildPlaybackPositionKey } from "../../lib/media";
import type { Article, ArticleSegment } from "../../types";
import type { KtvExportConfig, KtvExportResult, KtvPositionPreset } from "../../lib/tauri";

interface KtvExportPageProps {
  article: Article;
  onBack?: () => void;
}

type ExportState =
  | { status: "idle" }
  | { status: "success"; outputPath: string }
  | { status: "error"; message: string };

const FONT_OPTIONS = [
  "Noto Sans CJK JP",
  "Noto Sans CJK KR",
  "Noto Sans CJK SC",
  "Noto Serif CJK JP",
];

export function KtvExportPage({ article, onBack }: KtvExportPageProps) {
  const { t } = useTranslation();
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const [workingArticle, setWorkingArticle] = useState(article);
  const [displayMode, setDisplayMode] = useState<"original" | "bilingual" | "translation">("original");
  const [showReadings, setShowReadings] = useState(true);
  const [fontSizeInput, setFontSizeInput] = useState("48");
  const [readingScaleInput, setReadingScaleInput] = useState("0.7");
  const [lineGapInput, setLineGapInput] = useState("8");
  const [bilingualGapInput, setBilingualGapInput] = useState("12");
  const [originalFontFamily, setOriginalFontFamily] = useState("Noto Sans CJK JP");
  const [translationFontFamily, setTranslationFontFamily] = useState("Noto Sans CJK SC");
  const [readingFontFamily, setReadingFontFamily] = useState("Noto Sans CJK JP");
  const [originalColorInput, setOriginalColorInput] = useState("#FFFFFF");
  const [translationColorInput, setTranslationColorInput] = useState("#FACC15");
  const [readingColorInput, setReadingColorInput] = useState("#D1D5DB");
  const [outlineColorInput, setOutlineColorInput] = useState("#000000");
  const [outlineWidthInput, setOutlineWidthInput] = useState("2");
  const [shadowEnabled, setShadowEnabled] = useState(true);
  const [shadowColorInput, setShadowColorInput] = useState("#000000");
  const [shadowOffsetXInput, setShadowOffsetXInput] = useState("0");
  const [shadowOffsetYInput, setShadowOffsetYInput] = useState("2");
  const [shadowBlurInput, setShadowBlurInput] = useState("4");
  const [positionPreset, setPositionPreset] = useState<KtvPositionPreset>("bottom");
  const [bottomMarginInput, setBottomMarginInput] = useState("48");
  const [horizontalMarginInput, setHorizontalMarginInput] = useState("32");
  const [previewTime, setPreviewTime] = useState(0);
  const [videoDimensions, setVideoDimensions] = useState<{ width: number; height: number } | null>(null);
  const [isExporting, setIsExporting] = useState(false);
  const [exportState, setExportState] = useState<ExportState>({ status: "idle" });
  const [mediaUrl, setMediaUrl] = useState("");

  useEffect(() => {
    const initialConfig = createInitialConfig(article);
    setWorkingArticle(article);
    setDisplayMode(initialConfig.displayMode);
    setShowReadings(initialConfig.showReading);
    setFontSizeInput(initialConfig.fontSize.toString());
    setReadingScaleInput(initialConfig.readingScale.toString());
    setLineGapInput(initialConfig.lineGap.toString());
    setBilingualGapInput(initialConfig.bilingualGap.toString());
    setOriginalFontFamily(initialConfig.originalFontFamily);
    setTranslationFontFamily(initialConfig.translationFontFamily);
    setReadingFontFamily(initialConfig.readingFontFamily);
    setOriginalColorInput(initialConfig.originalColor);
    setTranslationColorInput(initialConfig.translationColor);
    setReadingColorInput(initialConfig.readingColor);
    setOutlineColorInput(initialConfig.outlineColor);
    setOutlineWidthInput(initialConfig.outlineWidth.toString());
    setShadowEnabled(initialConfig.shadowEnabled);
    setShadowColorInput(initialConfig.shadowColor);
    setShadowOffsetXInput(initialConfig.shadowOffsetX.toString());
    setShadowOffsetYInput(initialConfig.shadowOffsetY.toString());
    setShadowBlurInput(initialConfig.shadowBlur.toString());
    setPositionPreset(initialConfig.positionPreset);
    setBottomMarginInput(initialConfig.bottomMargin.toString());
    setHorizontalMarginInput(initialConfig.horizontalMargin.toString());
    setPreviewTime(0);
    setVideoDimensions(null);
    setExportState({ status: "idle" });
  }, [article]);

  useEffect(() => {
    const languageHint = detectLanguageHint(article);
    if (!languageHint) {
      return;
    }

    void invoke<Article>("prepare_ktv_segments_cmd", {
      articleId: article.id,
      languageHint,
    })
      .then((preparedArticle) => {
        if (preparedArticle) {
          setWorkingArticle(preparedArticle);
        }
      })
      .catch((error) => {
        console.error("[KtvExportPage] Failed to prepare KTV segments:", error);
      });
  }, [article]);

  const sortedSegments = useMemo(
    () => [...(workingArticle.segments ?? [])].sort((left, right) => left.order - right.order),
    [workingArticle.segments],
  );

  const timedSegments = useMemo(
    () => sortedSegments.filter((segment) => segment.start_time != null && segment.end_time != null),
    [sortedSegments],
  );

  const activeSegment = useMemo(() => {
    const playingSegment = timedSegments.find((segment) => {
      const start = segment.start_time ?? 0;
      const end = segment.end_time ?? Number.MAX_SAFE_INTEGER;
      return previewTime >= start && previewTime <= end;
    });

    return playingSegment ?? timedSegments[0] ?? sortedSegments[0] ?? null;
  }, [previewTime, sortedSegments, timedSegments]);

  const activeSegmentReadingParts = useMemo(
    () => buildInlineReadingParts(activeSegment),
    [activeSegment],
  );

  const readingsReady = useMemo(
    () =>
      sortedSegments.length > 0 &&
      sortedSegments.every((segment) => {
        const translation = segment.translation?.trim();
        return Boolean(translation && hasInlineReadingSource(segment));
      }),
    [sortedSegments],
  );

  useEffect(() => {
    if (!readingsReady && showReadings) {
      setShowReadings(false);
    }
  }, [readingsReady, showReadings]);

  const readingsVisible = displayMode !== "translation" && readingsReady && showReadings;

  const exportConfig = useMemo<KtvExportConfig>(() => ({
    displayMode,
    showReading: readingsVisible,
    originalFontFamily,
    translationFontFamily,
    readingFontFamily,
    fontSize: parseNumber(fontSizeInput, 48),
    readingScale: parseFloatNumber(readingScaleInput, 0.7),
    lineGap: parseNumber(lineGapInput, 8),
    bilingualGap: parseNumber(bilingualGapInput, 12),
    originalColor: normalizeColor(originalColorInput, "#FFFFFF"),
    translationColor: normalizeColor(translationColorInput, "#FACC15"),
    readingColor: normalizeColor(readingColorInput, "#D1D5DB"),
    outlineColor: normalizeColor(outlineColorInput, "#000000"),
    outlineWidth: parseNumber(outlineWidthInput, 2),
    shadowEnabled,
    shadowColor: normalizeColor(shadowColorInput, "#000000"),
    shadowOffsetX: parseSignedNumber(shadowOffsetXInput, 0),
    shadowOffsetY: parseSignedNumber(shadowOffsetYInput, 2),
    shadowBlur: parseNumber(shadowBlurInput, 4),
    positionPreset,
    bottomMargin: parseNumber(bottomMarginInput, 48),
    horizontalMargin: parseNumber(horizontalMarginInput, 32),
    videoWidth: videoDimensions?.width,
    videoHeight: videoDimensions?.height,
  }), [
    displayMode,
    bilingualGapInput,
    bottomMarginInput,
    fontSizeInput,
    horizontalMarginInput,
    lineGapInput,
    originalColorInput,
    originalFontFamily,
    outlineColorInput,
    outlineWidthInput,
    positionPreset,
    readingColorInput,
    readingFontFamily,
    readingScaleInput,
    shadowBlurInput,
    shadowColorInput,
    shadowEnabled,
    shadowOffsetXInput,
    shadowOffsetYInput,
    readingsVisible,
    translationColorInput,
    translationFontFamily,
    videoDimensions,
  ]);

  const previewStyle = useMemo(() => createPreviewStyle(exportConfig), [exportConfig]);
  const canExport = Boolean(workingArticle.media_path && timedSegments.length > 0);
  useEffect(() => {
    let cancelled = false;

    const loadMediaUrl = async () => {
      try {
        const url = await buildMediaResourceUrl(workingArticle.media_path, "video");
        if (!cancelled) setMediaUrl(url);
      } catch (error) {
        console.warn("[KtvExportPage] Failed to build media URL:", error);
        if (!cancelled) setMediaUrl("");
      }
    };

    void loadMediaUrl();

    return () => {
      cancelled = true;
    };
  }, [workingArticle.media_path]);
  const playbackStorageKey = useMemo(
    () => buildPlaybackPositionKey(workingArticle.id, mediaUrl),
    [mediaUrl, workingArticle.id],
  );

  useEffect(() => {
    const media = videoRef.current;
    if (!media || !mediaUrl) {
      return;
    }

    const restorePlaybackPosition = () => {
      try {
        const savedTime = window.localStorage?.getItem(playbackStorageKey);
        if (!savedTime) {
          return;
        }

        const time = Number.parseFloat(savedTime);
        if (Number.isFinite(time) && time > 0) {
          media.currentTime = time;
          setPreviewTime(time);
        }
      } catch (error) {
        console.warn("[KtvExportPage] Failed to restore playback position:", error);
      }
    };

    media.addEventListener("loadedmetadata", restorePlaybackPosition);
    if (media.readyState >= 1) {
      restorePlaybackPosition();
    }

    return () => {
      media.removeEventListener("loadedmetadata", restorePlaybackPosition);
    };
  }, [mediaUrl, playbackStorageKey]);

  useEffect(() => {
    if (!mediaUrl) {
      return;
    }

    const interval = window.setInterval(() => {
      const media = videoRef.current;
      if (media && !media.paused) {
        savePlaybackPosition(playbackStorageKey, media.currentTime);
      }
    }, 5000);

    return () => {
      window.clearInterval(interval);
    };
  }, [mediaUrl, playbackStorageKey]);

  useEffect(() => () => {
    if (videoRef.current) {
      savePlaybackPosition(playbackStorageKey, videoRef.current.currentTime);
    }
  }, [playbackStorageKey]);

  const handleExport = async () => {
    const outputPath = await save({
      filters: [{ name: "MP4 Video", extensions: ["mp4"] }],
      defaultPath: `${workingArticle.title}-ktv.mp4`,
    });

    if (typeof outputPath !== "string") {
      return;
    }

    setExportState({ status: "idle" });
    setIsExporting(true);

    try {
      const result = await invoke<KtvExportResult>("export_ktv_video_cmd", {
        articleId: workingArticle.id,
        outputPath,
        config: exportConfig,
      });

      setExportState({
        status: "success",
        outputPath: result?.outputPath ?? outputPath,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setExportState({ status: "error", message });
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto px-4 py-6 md:px-8 lg:px-12">
      <div className="mx-auto flex max-w-7xl flex-col gap-4">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">{t("ktvExport.title")}</h1>
            <p className="text-sm text-muted-foreground">{workingArticle.title}</p>
          </div>
          <Button type="button" variant="outline" onClick={onBack}>
            {t("ktvExport.back")}
          </Button>
        </div>

        <div className="grid gap-6 xl:grid-cols-[1.3fr_0.9fr_0.8fr]">
          <section className="space-y-4 rounded-2xl border border-border bg-card p-6">
            <div className="relative aspect-video overflow-hidden rounded-xl bg-black">
              {mediaUrl ? (
                <video
                  ref={videoRef}
                  aria-label="KTV video preview"
                  className="h-full w-full object-contain"
                  controls
                  playsInline
                  preload="auto"
                  src={mediaUrl}
                  onLoadedMetadata={(event) => {
                    setVideoDimensions({
                      width: event.currentTarget.videoWidth,
                      height: event.currentTarget.videoHeight,
                    });
                  }}
                  onTimeUpdate={(event) => setPreviewTime(event.currentTarget.currentTime)}
                  onSeeked={(event) => setPreviewTime(event.currentTarget.currentTime)}
                  onPause={(event) => savePlaybackPosition(playbackStorageKey, event.currentTarget.currentTime)}
                  onError={(event) => {
                    console.error("[KtvExportPage] Video playback error:", event);
                  }}
                />
              ) : (
                <div className="flex h-full items-center justify-center text-sm text-white/70">
                  {t("ktvExport.noVideoSource")}
                </div>
              )}

              <div
                className={cn(
                  "pointer-events-none absolute inset-x-0 flex px-6",
                  positionPreset === "center_lower" && "items-center",
                  positionPreset !== "center_lower" && "items-end",
                )}
                style={{
                  bottom: positionPreset === "center_lower" ? "22%" : `${exportConfig.bottomMargin}px`,
                  left: `${exportConfig.horizontalMargin}px`,
                  right: `${exportConfig.horizontalMargin}px`,
                }}
              >
                {activeSegment ? (
                  <div className="w-full text-center">
                    {displayMode !== "translation" ? (
                      <p className="leading-tight" style={previewStyle.original}>
                        {readingsVisible && activeSegmentReadingParts.length > 0
                          ? activeSegmentReadingParts.map((part, index) => (
                            <Fragment key={`${part.text}-${part.reading ?? "plain"}-${index}`}>
                              <span>{part.text}</span>
                              {part.reading ? (
                                <span
                                  className="inline-block align-baseline"
                                  style={{
                                    ...previewStyle.readingInline,
                                    marginLeft: `${Math.max(Math.round(exportConfig.lineGap * 0.35), 4)}px`,
                                  }}
                                >
                                  {part.reading}
                                </span>
                              ) : null}
                            </Fragment>
                          ))
                          : <span>{activeSegment.text}</span>}
                      </p>
                    ) : null}

                    {displayMode !== "original" && activeSegment.translation ? (
                      <p
                        className="leading-tight"
                        style={{
                          ...previewStyle.translation,
                          marginTop: displayMode === "bilingual" ? `${exportConfig.bilingualGap}px` : "0px",
                        }}
                      >
                        {activeSegment.translation}
                      </p>
                    ) : null}
                  </div>
                ) : (
                  <div className="w-full text-center text-sm text-white/70">
                    {t("ktvExport.noSubtitleSegments")}
                  </div>
                )}
              </div>
            </div>

            <div className="rounded-xl border border-border/80 bg-muted/30 p-4">
              <div className="mb-3 flex items-center justify-between text-sm">
                <span className="font-medium">{t("ktvExport.timelinePreview")}</span>
                <span className="text-muted-foreground">{formatSeconds(previewTime)}</span>
              </div>
              <div className="space-y-2">
                {timedSegments.slice(0, 6).map((segment) => (
                  <div
                    key={segment.id}
                    className={cn(
                      "rounded-lg border px-3 py-2 text-sm",
                      segment.id === activeSegment?.id
                        ? "border-primary bg-primary/10"
                        : "border-border/70 bg-background/80",
                    )}
                  >
                    <div className="mb-1 flex items-center justify-between gap-3 text-xs text-muted-foreground">
                      <span>{formatSeconds(segment.start_time ?? 0)}</span>
                      <span>{formatSeconds(segment.end_time ?? 0)}</span>
                    </div>
                    <p className="line-clamp-2 font-medium">
                      {displayMode === "translation" && segment.translation ? segment.translation : segment.text}
                    </p>
                    {displayMode === "bilingual" && segment.translation ? (
                      <p className="mt-1 line-clamp-2 text-muted-foreground">{segment.translation}</p>
                    ) : null}
                  </div>
                ))}
              </div>
            </div>
          </section>

          <aside className="space-y-5 rounded-2xl border border-border bg-card p-6">
            <section className="space-y-3">
              <h2 className="text-sm font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                {t("ktvExport.content")}
              </h2>

              <fieldset className="space-y-2">
                <legend className="text-sm font-medium">{t("ktvExport.display")}</legend>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    aria-label={t("ktvExport.original")}
                    checked={displayMode === "original"}
                    name="display-mode"
                    type="radio"
                    onChange={() => setDisplayMode("original")}
                  />
                  {t("ktvExport.original")}
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    aria-label={t("ktvExport.bilingual")}
                    checked={displayMode === "bilingual"}
                    name="display-mode"
                    type="radio"
                    onChange={() => setDisplayMode("bilingual")}
                  />
                  {t("ktvExport.bilingual")}
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    aria-label={t("ktvExport.translation")}
                    checked={displayMode === "translation"}
                    name="display-mode"
                    type="radio"
                    onChange={() => setDisplayMode("translation")}
                  />
                  {t("ktvExport.translation")}
                </label>
              </fieldset>

              {displayMode !== "translation" ? (
                <div className="space-y-2">
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      aria-label={t("ktvExport.showReadings")}
                      checked={readingsVisible}
                      disabled={!readingsReady}
                      type="checkbox"
                      onChange={(event) => setShowReadings(event.target.checked)}
                    />
                    {t("ktvExport.showReadings")}
                  </label>
                  {!readingsReady ? (
                    <p className="text-xs text-muted-foreground">
                      {t("ktvExport.readingsRequireTranslation")}
                    </p>
                  ) : null}
                </div>
              ) : null}
            </section>

            <section className="space-y-3">
              <h2 className="text-sm font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                {t("ktvExport.typography")}
              </h2>

              <Field label={t("ktvExport.originalFont")} htmlFor="original-font">
                <Select
                  aria-label={t("ktvExport.originalFont")}
                  id="original-font"
                  value={originalFontFamily}
                  onChange={(event) => setOriginalFontFamily(event.target.value)}
                >
                  {FONT_OPTIONS.map((font) => (
                    <option key={font} value={font}>{font}</option>
                  ))}
                </Select>
              </Field>

              <Field label={t("ktvExport.translationFont")} htmlFor="translation-font">
                <Select
                  aria-label={t("ktvExport.translationFont")}
                  id="translation-font"
                  value={translationFontFamily}
                  onChange={(event) => setTranslationFontFamily(event.target.value)}
                >
                  {FONT_OPTIONS.map((font) => (
                    <option key={font} value={font}>{font}</option>
                  ))}
                </Select>
              </Field>

              <Field label={t("ktvExport.readingFont")} htmlFor="reading-font">
                <Select
                  aria-label={t("ktvExport.readingFont")}
                  id="reading-font"
                  value={readingFontFamily}
                  onChange={(event) => setReadingFontFamily(event.target.value)}
                >
                  {FONT_OPTIONS.map((font) => (
                    <option key={font} value={font}>{font}</option>
                  ))}
                </Select>
              </Field>

              <Field label={t("ktvExport.originalFontSize")} htmlFor="original-font-size">
                <Input
                  aria-label={t("ktvExport.originalFontSize")}
                  id="original-font-size"
                  min={20}
                  step={1}
                  type="number"
                  value={fontSizeInput}
                  onChange={(event) => setFontSizeInput(event.target.value)}
                />
              </Field>

              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t("ktvExport.readingScale")} htmlFor="reading-scale">
                  <Input
                    aria-label={t("ktvExport.readingScale")}
                    id="reading-scale"
                    min={0.4}
                    step={0.1}
                    type="number"
                    value={readingScaleInput}
                    onChange={(event) => setReadingScaleInput(event.target.value)}
                  />
                </Field>

                <Field label={t("ktvExport.lineGap")} htmlFor="line-gap">
                  <Input
                    aria-label={t("ktvExport.lineGap")}
                    id="line-gap"
                    min={0}
                    step={1}
                    type="number"
                    value={lineGapInput}
                    onChange={(event) => setLineGapInput(event.target.value)}
                  />
                </Field>
              </div>

              <Field label={t("ktvExport.bilingualGap")} htmlFor="bilingual-gap">
                <Input
                  aria-label={t("ktvExport.bilingualGap")}
                  id="bilingual-gap"
                  min={0}
                  step={1}
                  type="number"
                  value={bilingualGapInput}
                  onChange={(event) => setBilingualGapInput(event.target.value)}
                />
              </Field>
            </section>
          </aside>

          <aside className="space-y-5 rounded-2xl border border-border bg-card p-6">
            <section className="space-y-3">
              <h2 className="text-sm font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                {t("ktvExport.style")}
              </h2>

              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t("ktvExport.originalColor")} htmlFor="original-color">
                  <Input
                    aria-label={t("ktvExport.originalColor")}
                    id="original-color"
                    value={originalColorInput}
                    onChange={(event) => setOriginalColorInput(event.target.value)}
                  />
                </Field>

                <Field label={t("ktvExport.translationColor")} htmlFor="translation-color">
                  <Input
                    aria-label={t("ktvExport.translationColor")}
                    id="translation-color"
                    value={translationColorInput}
                    onChange={(event) => setTranslationColorInput(event.target.value)}
                  />
                </Field>

                <Field label={t("ktvExport.readingColor")} htmlFor="reading-color">
                  <Input
                    aria-label={t("ktvExport.readingColor")}
                    id="reading-color"
                    value={readingColorInput}
                    onChange={(event) => setReadingColorInput(event.target.value)}
                  />
                </Field>

                <Field label={t("ktvExport.outlineColor")} htmlFor="outline-color">
                  <Input
                    aria-label={t("ktvExport.outlineColor")}
                    id="outline-color"
                    value={outlineColorInput}
                    onChange={(event) => setOutlineColorInput(event.target.value)}
                  />
                </Field>
              </div>

              <Field label={t("ktvExport.outlineWidth")} htmlFor="outline-width">
                <Input
                  aria-label={t("ktvExport.outlineWidth")}
                  id="outline-width"
                  min={0}
                  step={1}
                  type="number"
                  value={outlineWidthInput}
                  onChange={(event) => setOutlineWidthInput(event.target.value)}
                />
              </Field>

              <label className="flex items-center gap-2 text-sm">
                <input
                  aria-label={t("ktvExport.enableShadow")}
                  checked={shadowEnabled}
                  type="checkbox"
                  onChange={(event) => setShadowEnabled(event.target.checked)}
                />
                {t("ktvExport.enableShadow")}
              </label>

              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t("ktvExport.shadowColor")} htmlFor="shadow-color">
                  <Input
                    aria-label={t("ktvExport.shadowColor")}
                    id="shadow-color"
                    value={shadowColorInput}
                    onChange={(event) => setShadowColorInput(event.target.value)}
                  />
                </Field>

                <Field label={t("ktvExport.shadowBlur")} htmlFor="shadow-blur">
                  <Input
                    aria-label={t("ktvExport.shadowBlur")}
                    id="shadow-blur"
                    min={0}
                    step={1}
                    type="number"
                    value={shadowBlurInput}
                    onChange={(event) => setShadowBlurInput(event.target.value)}
                  />
                </Field>

                <Field label={t("ktvExport.shadowOffsetX")} htmlFor="shadow-offset-x">
                  <Input
                    aria-label={t("ktvExport.shadowOffsetX")}
                    id="shadow-offset-x"
                    step={1}
                    type="number"
                    value={shadowOffsetXInput}
                    onChange={(event) => setShadowOffsetXInput(event.target.value)}
                  />
                </Field>

                <Field label={t("ktvExport.shadowOffsetY")} htmlFor="shadow-offset-y">
                  <Input
                    aria-label={t("ktvExport.shadowOffsetY")}
                    id="shadow-offset-y"
                    step={1}
                    type="number"
                    value={shadowOffsetYInput}
                    onChange={(event) => setShadowOffsetYInput(event.target.value)}
                  />
                </Field>
              </div>
            </section>

            <section className="space-y-3">
              <h2 className="text-sm font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                {t("ktvExport.layout")}
              </h2>

              <Field label={t("ktvExport.subtitlePosition")} htmlFor="subtitle-position">
                <Select
                  aria-label={t("ktvExport.subtitlePosition")}
                  id="subtitle-position"
                  value={positionPreset}
                  onChange={(event) => setPositionPreset(event.target.value as KtvPositionPreset)}
                >
                  <option value="bottom">{t("ktvExport.positionBottom")}</option>
                  <option value="lower_third">{t("ktvExport.positionLowerThird")}</option>
                  <option value="center_lower">{t("ktvExport.positionCenterLower")}</option>
                </Select>
              </Field>

              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t("ktvExport.bottomMargin")} htmlFor="bottom-margin">
                  <Input
                    aria-label={t("ktvExport.bottomMargin")}
                    id="bottom-margin"
                    min={0}
                    step={1}
                    type="number"
                    value={bottomMarginInput}
                    onChange={(event) => setBottomMarginInput(event.target.value)}
                  />
                </Field>

                <Field label={t("ktvExport.horizontalMargin")} htmlFor="horizontal-margin">
                  <Input
                    aria-label={t("ktvExport.horizontalMargin")}
                    id="horizontal-margin"
                    min={0}
                    step={1}
                    type="number"
                    value={horizontalMarginInput}
                    onChange={(event) => setHorizontalMarginInput(event.target.value)}
                  />
                </Field>
              </div>
            </section>

            <section className="space-y-3 rounded-xl border border-border/80 bg-muted/20 p-4">
              <div className="flex items-center justify-between">
                <div>
                  <h2 className="text-sm font-semibold">{t("ktvExport.exportSection")}</h2>
                  <p className="text-xs text-muted-foreground">{t("ktvExport.exportHint")}</p>
                </div>
                <Button type="button" onClick={() => void handleExport()} disabled={isExporting || !canExport}>
                  {isExporting ? t("ktvExport.exporting") : t("ktvExport.export")}
                </Button>
              </div>

              {!canExport ? (
                <p className="text-xs text-muted-foreground">
                  {t("ktvExport.exportRequiresTimedSegments")}
                </p>
              ) : null}

              {exportState.status === "success" ? (
                <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-3 py-2">
                  <p className="text-sm font-medium text-emerald-700 dark:text-emerald-300">{t("ktvExport.exportCompleted")}</p>
                  <p className="mt-1 break-all text-xs text-muted-foreground">{exportState.outputPath}</p>
                </div>
              ) : null}

              {exportState.status === "error" ? (
                <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2">
                  <p className="text-sm font-medium text-destructive">{t("ktvExport.exportFailed")}</p>
                  <p className="mt-1 break-all text-xs text-muted-foreground">{exportState.message}</p>
                </div>
              ) : null}
            </section>
          </aside>
        </div>
      </div>
    </div>
  );
}

interface FieldProps {
  label: string;
  htmlFor: string;
  children: ReactNode;
}

function Field({ label, htmlFor, children }: FieldProps) {
  return (
    <div className="space-y-2">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
    </div>
  );
}

function createInitialConfig(article: Article): KtvExportConfig {
  const languageHint = detectLanguageHint(article);
  const originalFontFamily = languageHint === "ko" ? "Noto Sans CJK KR" : "Noto Sans CJK JP";
  const readingFontFamily = languageHint === "ko" ? "Noto Sans CJK KR" : "Noto Sans CJK JP";

  return {
    displayMode: "original",
    showReading: true,
    originalFontFamily,
    translationFontFamily: "Noto Sans CJK SC",
    readingFontFamily,
    fontSize: 48,
    readingScale: 0.7,
    lineGap: 8,
    bilingualGap: 12,
    originalColor: "#FFFFFF",
    translationColor: "#FACC15",
    readingColor: "#D1D5DB",
    outlineColor: "#000000",
    outlineWidth: 2,
    shadowEnabled: true,
    shadowColor: "#000000",
    shadowOffsetX: 0,
    shadowOffsetY: 2,
    shadowBlur: 4,
    positionPreset: "bottom",
    bottomMargin: 48,
    horizontalMargin: 32,
  };
}

function createPreviewStyle(config: KtvExportConfig) {
  const textShadow = buildTextShadow(config);

  return {
    readingInline: {
      color: config.readingColor,
      fontFamily: config.readingFontFamily,
      fontSize: `${Math.round(config.fontSize * config.readingScale)}px`,
      textShadow,
    },
    original: {
      color: config.originalColor,
      fontFamily: config.originalFontFamily,
      fontSize: `${config.fontSize}px`,
      textShadow,
    },
    translation: {
      color: config.translationColor,
      fontFamily: config.translationFontFamily,
      fontSize: `${Math.max(config.fontSize - 4, 20)}px`,
      textShadow,
    },
  } as const;
}

type InlineReadingPart = {
  text: string;
  reading?: string;
};

function buildInlineReadingParts(segment?: ArticleSegment | null): InlineReadingPart[] {
  if (!segment?.text) {
    return [];
  }

  const vocabularyParts = buildVocabularyReadingParts(segment.text, segment.explanation?.vocabulary);
  if (vocabularyParts.length > 0) {
    return vocabularyParts;
  }

  const fallbackReading = formatInlineReading(segment.text, segment.reading_text);
  if (!fallbackReading) {
    return [];
  }

  return [{ text: segment.text, reading: fallbackReading }];
}

function hasInlineReadingSource(segment: ArticleSegment) {
  return isReadingAnnotationOptional(segment.text)
    || buildVocabularyReadingParts(segment.text, segment.explanation?.vocabulary).length > 0
    || Boolean(formatInlineReading(segment.text, segment.reading_text));
}

function buildVocabularyReadingParts(
  text: string,
  vocabulary?: Array<{ word?: string; reading?: string }>,
): InlineReadingPart[] {
  type ReadingCandidate = {
    word: string;
    reading: string;
  };

  const candidates: ReadingCandidate[] = (vocabulary ?? [])
    .map((item) => ({
      word: item.word?.trim() ?? "",
      reading: item.reading?.trim() ?? "",
    }))
    .filter((item): item is ReadingCandidate => Boolean(item.word && item.reading && item.word !== item.reading));

  if (candidates.length === 0) {
    return [];
  }

  const remaining = [...candidates];
  const parts: InlineReadingPart[] = [];
  let cursor = 0;

  while (cursor < text.length) {
    let bestMatch: { index: number; start: number; word: string; reading: string } | null = null;

    for (const [index, candidate] of remaining.entries()) {
      const start = text.indexOf(candidate.word, cursor);
      if (start === -1) {
        continue;
      }

      if (
        !bestMatch
        || start < bestMatch.start
        || (start === bestMatch.start && candidate.word.length > bestMatch.word.length)
      ) {
        bestMatch = { index, start, word: candidate.word, reading: candidate.reading };
      }
    }

    if (!bestMatch) {
      break;
    }

    if (bestMatch.start > cursor) {
      parts.push({ text: text.slice(cursor, bestMatch.start) });
    }

    parts.push({
      text: bestMatch.word,
      reading: `（${bestMatch.reading}）`,
    });

    cursor = bestMatch.start + bestMatch.word.length;
    remaining.splice(bestMatch.index, 1);
  }

  if (cursor < text.length) {
    parts.push({ text: text.slice(cursor) });
  }

  return parts.some((part) => part.reading) ? parts : [];
}

function formatInlineReading(text?: string, reading?: string) {
  const normalizedText = text?.trim();
  const normalizedReading = reading?.trim();

  if (!normalizedText || !normalizedReading || normalizedText === normalizedReading) {
    return null;
  }

  return `（${normalizedReading}）`;
}

function isReadingAnnotationOptional(text?: string) {
  const normalizedText = text?.trim();

  if (!normalizedText) {
    return false;
  }

  const contentOnly = normalizedText.replace(/[、。！？ー・「」（）\s]/g, "");
  return contentOnly.length > 0 && /^[\u3040-\u309F\u30A0-\u30FF]+$/.test(contentOnly);
}

function buildTextShadow(config: KtvExportConfig) {
  const outline = config.outlineWidth > 0
    ? [
        `${config.outlineWidth}px 0 0 ${config.outlineColor}`,
        `-${config.outlineWidth}px 0 0 ${config.outlineColor}`,
        `0 ${config.outlineWidth}px 0 ${config.outlineColor}`,
        `0 -${config.outlineWidth}px 0 ${config.outlineColor}`,
      ]
    : [];

  const shadow = config.shadowEnabled
    ? [`${config.shadowOffsetX}px ${config.shadowOffsetY}px ${config.shadowBlur}px ${config.shadowColor}`]
    : [];

  return [...outline, ...shadow].join(", ");
}

function savePlaybackPosition(storageKey: string, time: number) {
  if (time <= 0) {
    return;
  }

  try {
    window.localStorage?.setItem(storageKey, time.toString());
  } catch (error) {
    console.warn("[KtvExportPage] Failed to save playback position:", error);
  }
}

function detectLanguageHint(article: Article): "ja" | "ko" | null {
  const combinedText = (article.segments ?? [])
    .map((segment) => segment.text)
    .join(" ");

  if (/[ぁ-ゖァ-ヺ]/.test(combinedText)) {
    return "ja";
  }

  if (/[가-힣]/.test(combinedText)) {
    return "ko";
  }

  return null;
}

function normalizeColor(value: string, fallback: string): string {
  const normalized = value.trim().toUpperCase();
  return /^#[0-9A-F]{6}$/.test(normalized) ? normalized : fallback;
}

function parseNumber(value: string, fallback: number): number {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function parseSignedNumber(value: string, fallback: number): number {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function parseFloatNumber(value: string, fallback: number): number {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function formatSeconds(value: number) {
  const total = Math.max(value, 0);
  const minutes = Math.floor(total / 60);
  const seconds = Math.floor(total % 60);
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

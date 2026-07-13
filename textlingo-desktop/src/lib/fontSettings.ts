import type { AppConfig } from "./tauri";

export const FONT_PRESETS = [
  {
    value: "",
    labelKey: "settings.fonts.themeDefault",
    fallbackLabel: "Theme default",
  },
  {
    value: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif",
    labelKey: "settings.fonts.systemSans",
    fallbackLabel: "System Sans",
  },
  {
    value: "Inter, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif",
    labelKey: "settings.fonts.inter",
    fallbackLabel: "Inter",
  },
  {
    value: "\"Helvetica Neue\", Arial, sans-serif",
    labelKey: "settings.fonts.helvetica",
    fallbackLabel: "Helvetica / Arial",
  },
  {
    value: "\"PingFang SC\", \"Microsoft YaHei\", \"Noto Sans CJK SC\", sans-serif",
    labelKey: "settings.fonts.chineseSans",
    fallbackLabel: "Chinese Sans",
  },
  {
    value: "Georgia, \"Times New Roman\", serif",
    labelKey: "settings.fonts.serif",
    fallbackLabel: "Serif",
  },
  {
    value: "\"LXGW WenKai\", \"Kaiti SC\", \"STKaiti\", serif",
    labelKey: "settings.fonts.chineseSerif",
    fallbackLabel: "Chinese Serif",
  },
] as const;

export function normalizeFontFamily(value: string | null | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized ? normalized : undefined;
}

export function applyFontSettings(config: Pick<AppConfig, "ui_font_family" | "reader_font_family"> | null | undefined) {
  if (typeof document === "undefined") return;

  const root = document.documentElement;
  const uiFontFamily = normalizeFontFamily(config?.ui_font_family);
  const readerFontFamily = normalizeFontFamily(config?.reader_font_family);

  if (uiFontFamily) {
    root.style.setProperty("--font-sans", uiFontFamily);
  } else {
    root.style.removeProperty("--font-sans");
  }

  if (readerFontFamily) {
    root.style.setProperty("--openkoto-reader-font-family", readerFontFamily);
  } else {
    root.style.removeProperty("--openkoto-reader-font-family");
  }
}

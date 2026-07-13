import { afterEach, describe, expect, it } from "vitest";

import { applyFontSettings, normalizeFontFamily } from "./fontSettings";

describe("fontSettings", () => {
  afterEach(() => {
    document.documentElement.removeAttribute("style");
  });

  it("normalizes blank font-family values", () => {
    expect(normalizeFontFamily(undefined)).toBeUndefined();
    expect(normalizeFontFamily(null)).toBeUndefined();
    expect(normalizeFontFamily("  ")).toBeUndefined();
    expect(normalizeFontFamily("  Inter, sans-serif  ")).toBe("Inter, sans-serif");
  });

  it("applies and clears font CSS variables", () => {
    applyFontSettings({
      ui_font_family: "Inter, sans-serif",
      reader_font_family: "Georgia, serif",
    });

    expect(document.documentElement.style.getPropertyValue("--font-sans")).toBe("Inter, sans-serif");
    expect(document.documentElement.style.getPropertyValue("--openkoto-reader-font-family")).toBe(
      "Georgia, serif",
    );

    applyFontSettings({
      ui_font_family: undefined,
      reader_font_family: undefined,
    });

    expect(document.documentElement.style.getPropertyValue("--font-sans")).toBe("");
    expect(document.documentElement.style.getPropertyValue("--openkoto-reader-font-family")).toBe("");
  });
});

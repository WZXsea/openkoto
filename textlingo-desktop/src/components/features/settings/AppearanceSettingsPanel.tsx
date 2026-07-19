import { useTranslation } from "react-i18next";
import type { AppConfig } from "../../../lib/tauri";
import { FONT_PRESETS, normalizeFontFamily } from "../../../lib/fontSettings";
import { useTheme } from "../../theme-provider";
import { Input } from "../../ui/input";
import { Select } from "../../ui/select";

export interface AppearanceSettingsPanelProps {
  config: AppConfig;
  onConfigChange: (config: AppConfig) => void;
}

function selectedFontPresetValue(value: string | undefined) {
  const normalized = normalizeFontFamily(value) ?? "";
  if (!normalized) return "";
  return FONT_PRESETS.some((preset) => preset.value === normalized) ? normalized : "__custom__";
}

export function AppearanceSettingsPanel({ config, onConfigChange }: AppearanceSettingsPanelProps) {
  const { t } = useTranslation();
  const { themeName, themeMode, setThemeName, setThemeMode } = useTheme();

  const updateFontFamily = (key: "ui_font_family" | "reader_font_family", value: string) => {
    onConfigChange({ ...config, [key]: normalizeFontFamily(value) });
  };

  return (
    <section className="space-y-4" aria-label={t("settings.nav.appearance")}>
      <div>
        <label className="block text-sm font-medium text-foreground mb-2" htmlFor="settings-theme-name">
          {t("settings.theme.themeName")}
        </label>
        <Select
          id="settings-theme-name"
          value={themeName}
          onChange={(event) => {
            const value = event.target.value;
            if (value === "seoul" || value === "tokyo" || value === "california") {
              setThemeName(value);
            }
          }}
        >
          <option value="seoul">{t("settings.theme.seoul")}</option>
          <option value="tokyo">{t("settings.theme.tokyo")}</option>
          <option value="california">{t("settings.theme.california")}</option>
        </Select>
      </div>

      <div>
        <label className="block text-sm font-medium text-foreground mb-2" htmlFor="settings-theme-mode">
          {t("settings.theme.themeMode")}
        </label>
        <Select
          id="settings-theme-mode"
          value={themeMode}
          onChange={(event) => {
            const value = event.target.value;
            if (value === "light" || value === "dark" || value === "system") {
              setThemeMode(value);
            }
          }}
        >
          <option value="light">{t("settings.theme.light")}</option>
          <option value="dark">{t("settings.theme.dark")}</option>
          <option value="system">{t("settings.theme.system")}</option>
        </Select>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <FontFamilyCard
          kind="ui"
          value={config.ui_font_family}
          onChange={(value) => updateFontFamily("ui_font_family", value)}
        />
        <FontFamilyCard
          kind="reader"
          value={config.reader_font_family}
          onChange={(value) => updateFontFamily("reader_font_family", value)}
        />
      </div>
    </section>
  );
}

interface FontFamilyCardProps {
  kind: "ui" | "reader";
  value?: string;
  onChange: (value: string) => void;
}

function FontFamilyCard({ kind, value, onChange }: FontFamilyCardProps) {
  const { t } = useTranslation();
  const isReader = kind === "reader";
  const title = isReader
    ? t("settings.fonts.readerFont", "Reader font")
    : t("settings.fonts.uiFont", "UI font");
  const inputLabel = isReader
    ? t("settings.fonts.readerFontInput", "Reader font family")
    : t("settings.fonts.uiFontInput", "UI font family");
  const help = isReader
    ? t("settings.fonts.readerHelp", "Applies to article text and plain-text reading areas.")
    : t("settings.fonts.uiHelp", "Applies to navigation, buttons, dialogs and most interface text.");

  return (
    <div className="rounded-lg border border-border bg-card/50 p-4">
      <label className="mb-2 block text-sm font-medium text-foreground" htmlFor={`settings-${kind}-font-preset`}>
        {title}
      </label>
      <Select
        id={`settings-${kind}-font-preset`}
        value={selectedFontPresetValue(value)}
        onChange={(event) => {
          if (event.target.value !== "__custom__") {
            onChange(event.target.value);
          }
        }}
      >
        {FONT_PRESETS.map((preset) => (
          <option key={preset.labelKey} value={preset.value}>
            {t(preset.labelKey, preset.fallbackLabel)}
          </option>
        ))}
        <option value="__custom__">{t("settings.fonts.custom", "Custom")}</option>
      </Select>
      <label className="mt-3 mb-2 block text-xs font-medium text-muted-foreground" htmlFor={`settings-${kind}-font-input`}>
        {t("settings.fonts.customInput", "Custom font-family")}
      </label>
      <Input
        id={`settings-${kind}-font-input`}
        aria-label={inputLabel}
        value={value ?? ""}
        onChange={(event) => onChange(event.target.value)}
        placeholder={t("settings.fonts.inputPlaceholder", 'e.g. "PingFang SC", sans-serif')}
      />
      <p className="mt-2 text-xs text-muted-foreground">{help}</p>
      <div
        className={`${isReader ? "openkoto-reader-font leading-relaxed" : ""} mt-3 rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground`}
        style={{ fontFamily: normalizeFontFamily(value) }}
      >
        {t("settings.fonts.preview", "The quick brown fox jumps over OpenKoto. 中文字体预览。")}
      </div>
    </div>
  );
}

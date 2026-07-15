import { useTranslation } from "react-i18next";
import type { AppConfig } from "../../../lib/tauri";
import { Select } from "../../ui/select";

export interface LanguageSettingsPanelProps {
  config: AppConfig;
  onConfigChange: (config: AppConfig) => void;
}

export function LanguageSettingsPanel({ config, onConfigChange }: LanguageSettingsPanelProps) {
  const { t, i18n } = useTranslation();
  const interfaceLanguages = [
    { value: "en", label: t("settings.interfaceLanguages.en") },
    { value: "zh", label: t("settings.interfaceLanguages.zh") },
    { value: "ja", label: t("settings.interfaceLanguages.ja") },
  ];
  const targetLanguages = [
    { value: "en", label: t("settings.languages.en") },
    { value: "zh-CN", label: t("settings.languages.zh-CN") },
    { value: "zh-TW", label: t("settings.languages.zh-TW") },
    { value: "ja", label: t("settings.languages.ja") },
    { value: "ko", label: t("settings.languages.ko") },
    { value: "es", label: t("settings.languages.es") },
    { value: "fr", label: t("settings.languages.fr") },
    { value: "de", label: t("settings.languages.de") },
    { value: "ru", label: t("settings.languages.ru") },
    { value: "ar", label: t("settings.languages.ar") },
  ];

  const handleInterfaceLanguageChange = async (language: string) => {
    onConfigChange({ ...config, interface_language: language });
    await i18n.changeLanguage(language);
  };

  return (
    <section className="space-y-4" aria-label={t("settings.nav.language")}>
      <div>
        <label className="block text-sm font-medium text-foreground mb-2" htmlFor="settings-interface-language">
          {t("settings.interfaceLanguage")}
        </label>
        <Select
          id="settings-interface-language"
          value={config.interface_language}
          onChange={(event) => void handleInterfaceLanguageChange(event.target.value)}
        >
          {interfaceLanguages.map((language) => (
            <option key={language.value} value={language.value}>{language.label}</option>
          ))}
        </Select>
      </div>
      <div>
        <label className="block text-sm font-medium text-foreground mb-2" htmlFor="settings-target-language">
          {t("settings.targetLanguage")}
        </label>
        <Select
          id="settings-target-language"
          value={config.target_language}
          onChange={(event) => onConfigChange({ ...config, target_language: event.target.value })}
        >
          {targetLanguages.map((language) => (
            <option key={language.value} value={language.value}>{language.label}</option>
          ))}
        </Select>
      </div>
    </section>
  );
}

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Loader2, Download } from "lucide-react";
import { Article } from "../../types";
import { MaterialImportPreviewDialogs, useMaterialImportPreview } from "../../features/materials/useMaterialImportPreview";

interface YouTubeImportFormProps {
    onSave?: (article: Article) => void;
    onCancel: () => void;
}

export function YouTubeImportForm({ onSave, onCancel }: YouTubeImportFormProps) {
    const { t } = useTranslation();
    const [url, setUrl] = useState("");
    const [error, setError] = useState<string | null>(null);
    const importPreview = useMaterialImportPreview<Article>({
        commit: (importJobId, duplicatePolicy) => invoke<Article>("import_youtube_video_cmd", {
            url: url.trim(),
            importJobId,
            duplicatePolicy,
        }),
        onSuccess: onSave,
        onError: (err) => setError(String(err)),
    });
    const isImporting = importPreview.isBusy;

    const handleImport = async () => {
        if (!url.trim()) {
            setError(t("youtubeImport.errors.urlRequired"));
            return;
        }

        if (!url.includes("youtube.com") && !url.includes("youtu.be")) {
            setError(t("youtubeImport.errors.urlInvalid"));
            return;
        }

        setError(null);
        await importPreview.startPreview({ sourceKind: "youtube", sourceUri: url.trim() });
    };

    return (
        <div className="flex flex-col h-full">
            <MaterialImportPreviewDialogs
                preview={importPreview.preview}
                duplicate={importPreview.duplicate}
                isBusy={importPreview.isBusy}
                onConfirm={() => void importPreview.confirmPreview()}
                onCancel={() => void importPreview.cancelPreview()}
                onResolve={(action) => void importPreview.resolveDuplicate(action)}
            />
            <div className="flex-1 space-y-4">
                {error && (
                    <div className="mb-4">
                        <div className="p-3 bg-red-900/30 border border-red-700 rounded-lg text-red-300 text-sm break-words">
                            {error.includes("登录") || error.includes("Sign in") || error.includes("login") || error.includes("Sign in to confirm you're not a bot") ? (
                                <div className="space-y-3">
                                    <p>{t("youtubeImport.errors.loginRequiredTip")}</p>
                                    <Button
                                        size="sm"
                                        variant="outline"
                                        className="w-full border-red-700 hover:bg-red-900/50 text-red-200"
                                        onClick={() => openUrl(url)}
                                    >
                                        {t("youtubeImport.openInBrowser")}
                                    </Button>
                                    <p className="text-[10px] opacity-70 border-t border-red-700/50 pt-2 break-all">{error}</p>
                                </div>
                            ) : (
                                <div className="break-all">{error}</div>
                            )}
                        </div>
                    </div>
                )}

                <div>
                    <label className="block text-sm font-medium text-foreground mb-2">
                        {t("youtubeImport.urlLabel")}
                    </label>
                    <div className="flex gap-2">
                        <Input
                            value={url}
                            onChange={(e) => setUrl(e.target.value)}
                            placeholder={t("youtubeImport.urlPlaceholder")}
                            disabled={isImporting}
                        />
                    </div>
                    <p className="text-xs text-muted-foreground mt-2">
                        {t("youtubeImport.processing")} (Downloads video & subtitles)
                    </p>
                </div>
            </div>

            <div className="flex justify-end gap-3 mt-6 pt-4 border-t border-border">
                <Button variant="secondary" onClick={onCancel} disabled={isImporting}>
                    {t("youtubeImport.cancel")}
                </Button>
                <Button onClick={handleImport} disabled={isImporting} className="gap-2 bg-red-600 hover:bg-red-700 text-white">
                    {isImporting ? (
                        <>
                            <Loader2 size={16} className="animate-spin" />
                            {t("youtubeImport.importing")}
                        </>
                    ) : (
                        <>
                            <Download size={16} />
                            {t("youtubeImport.import")}
                        </>
                    )}
                </Button>
            </div>
        </div>
    );
}

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dialog } from "../ui/dialog";
import { Button } from "../ui/button";
import { Plus, FileText, Youtube, FolderOpen, BookOpen, Music, Globe } from "lucide-react";
import { Article } from "../../types";
import { NewArticleForm } from "./NewArticleForm";
import { YouTubeImportForm } from "./YouTubeImportForm";
import { LocalVideoImportForm } from "./LocalVideoImportForm";
import { BookImportForm } from "./BookImportForm";
import { LocalAudioImportForm } from "./LocalAudioImportForm";
import { LocalSubtitleImportForm } from "./LocalSubtitleImportForm";
import { TextFileImportForm } from "./TextFileImportForm";
import { WebImportForm } from "./WebImportForm";
import { cn } from "../../lib/utils";
import { isPhase1CapabilityEnabled } from "../../lib/phase1Capabilities";
import { MaterialImportActivityProvider } from "../../features/materials/useMaterialImportPreview";

const AUDIO_EXTENSIONS = ['mp3', 'wav', 'm4a', 'aac', 'flac', 'ogg', 'wma'];

function isAudioFile(path: string): boolean {
    const ext = path.split('.').pop()?.toLowerCase() || '';
    return AUDIO_EXTENSIONS.includes(ext);
}

type MaterialType = "article" | "web" | "textFile" | "youtube" | "local" | "book" | "audio" | "subtitle";

interface NewMaterialDialogProps {
    isOpen: boolean;
    onClose: () => void;
    onSave?: (article: Article) => void;
    editingArticle?: Article | null;
}

export function NewMaterialDialog({ isOpen, onClose, onSave, editingArticle }: NewMaterialDialogProps) {
    const { t } = useTranslation();
    const [activeTab, setActiveTab] = useState<MaterialType>("article");
    const [isImportBusy, setIsImportBusy] = useState(false);
    const activeTabClassName = "bg-primary/10 text-primary";
    const inactiveTabClassName = "hover:bg-muted text-muted-foreground hover:text-foreground";
    const canUseWebImport = isPhase1CapabilityEnabled("webImport");
    const canUseYouTubeImport = isPhase1CapabilityEnabled("youtubeImport");

    // Initialize/reset tab when the dialog opens or the edited material changes.
    useEffect(() => {
        if (!isOpen) return;
        if (editingArticle) {
            if (editingArticle.source_type === "web" && canUseWebImport) setActiveTab("web");
            else if (editingArticle.source_type === "text_file") setActiveTab("textFile");
            else if (editingArticle.book_path) setActiveTab("book");
            else if (editingArticle.media_path?.includes("http") && canUseYouTubeImport) setActiveTab("youtube"); // Simple heuristic
            else if (editingArticle.media_path && isAudioFile(editingArticle.media_path)) setActiveTab("audio");
            else if (editingArticle.media_path) setActiveTab("local");
            else setActiveTab("article");
        } else {
            setActiveTab("article");
        }
        setIsImportBusy(false);
    }, [isOpen, editingArticle]);

    const handleClose = () => {
        onClose();
    };

    const handleSave = (article: Article) => {
        onSave?.(article);
        handleClose();
    };

    const isEditing = !!editingArticle;

    return (
        <MaterialImportActivityProvider onBusyChange={setIsImportBusy}>
        <Dialog
            isOpen={isOpen}
            onClose={handleClose}
            title={isEditing ? t("articleList.edit", "编辑素材") : t("header.newMaterial")}
            className="md:max-w-3xl !p-0 overflow-hidden flex flex-col h-[600px]"
            closeDisabled={isImportBusy}
        >
            <div className="flex flex-1 h-full overflow-hidden">
                {/* Left Sidebar - Tabs */}
                <div className="w-48 bg-muted/30 border-r border-border p-4 flex flex-col gap-2">
                    <h3 className="text-sm font-medium text-muted-foreground mb-2 px-2">
                        {isEditing ? t("articleList.edit") : t("header.newMaterial")}
                    </h3>

                    <button
                        className={cn(
                            "flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors text-left",
                            activeTab === "article"
                                ? activeTabClassName
                                : inactiveTabClassName,
                            isEditing && activeTab !== "article" && "opacity-50 cursor-not-allowed"
                        )}
                        onClick={() => !isEditing && !isImportBusy && setActiveTab("article")}
                        disabled={isEditing || isImportBusy}
                    >
                        <FileText size={18} />
                        {t("newArticle.title")}
                    </button>

                    {canUseWebImport && (
                        <button
                            className={cn(
                                "flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors text-left",
                                activeTab === "web"
                                    ? activeTabClassName
                                    : inactiveTabClassName,
                                isEditing && activeTab !== "web" && "opacity-50 cursor-not-allowed"
                            )}
                            onClick={() => !isEditing && !isImportBusy && setActiveTab("web")}
                            disabled={isEditing || isImportBusy}
                        >
                            <Globe size={18} />
                            {t("webImport.title", "网页导入")}
                        </button>
                    )}

                    <button
                        className={cn(
                            "flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors text-left",
                            activeTab === "textFile"
                                ? activeTabClassName
                                : inactiveTabClassName,
                            isEditing && activeTab !== "textFile" && "opacity-50 cursor-not-allowed"
                        )}
                        onClick={() => !isEditing && !isImportBusy && setActiveTab("textFile")}
                        disabled={isEditing || isImportBusy}
                    >
                        <FileText size={18} />
                        {t("textFileImport.title", "文本文件")}
                    </button>

                    <button
                        className={cn(
                            "flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors text-left",
                            activeTab === "book"
                                ? activeTabClassName
                                : inactiveTabClassName,
                            isEditing && activeTab !== "book" && "opacity-50 cursor-not-allowed"
                        )}
                        onClick={() => !isEditing && !isImportBusy && setActiveTab("book")}
                        disabled={isEditing || isImportBusy}
                    >
                        <BookOpen size={18} />
                        {t("bookImport.title", "导入书籍")}
                    </button>

                    {canUseYouTubeImport && (
                        <button
                            className={cn(
                                "flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors text-left",
                                activeTab === "youtube"
                                    ? activeTabClassName
                                    : inactiveTabClassName,
                                isEditing && activeTab !== "youtube" && "opacity-50 cursor-not-allowed"
                            )}
                            onClick={() => !isEditing && !isImportBusy && setActiveTab("youtube")}
                            disabled={isEditing || isImportBusy}
                        >
                            <Youtube size={18} />
                            {t("youtubeImport.title")}
                        </button>
                    )}

                    <button
                        className={cn(
                            "flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors text-left",
                            activeTab === "local"
                                ? activeTabClassName
                                : inactiveTabClassName,
                            isEditing && activeTab !== "local" && "opacity-50 cursor-not-allowed"
                        )}
                        onClick={() => !isEditing && !isImportBusy && setActiveTab("local")}
                        disabled={isEditing || isImportBusy}
                    >
                        <FolderOpen size={18} />
                        {t("localImport.title")}
                    </button>

                    <button
                        className={cn(
                            "flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors text-left",
                            activeTab === "audio"
                                ? activeTabClassName
                                : inactiveTabClassName,
                            isEditing && activeTab !== "audio" && "opacity-50 cursor-not-allowed"
                        )}
                        onClick={() => !isEditing && !isImportBusy && setActiveTab("audio")}
                        disabled={isEditing || isImportBusy}
                    >
                        <Music size={18} />
                        {t("audioImport.title", "本地音频")}
                    </button>

                    <button
                        className={cn(
                            "flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors text-left",
                            activeTab === "subtitle"
                                ? activeTabClassName
                                : inactiveTabClassName,
                            isEditing && activeTab !== "subtitle" && "opacity-50 cursor-not-allowed"
                        )}
                        onClick={() => !isEditing && !isImportBusy && setActiveTab("subtitle")}
                        disabled={isEditing || isImportBusy}
                    >
                        <FileText size={18} />
                        {t("subtitleImport.title", "字幕文件")}
                    </button>
                </div>

                {/* Right Content */}
                <div className="flex-1 p-6 overflow-hidden">
                    <div className="h-full">
                        {activeTab === "article" && (
                            <NewArticleForm onSave={handleSave} onCancel={handleClose} initialArticle={editingArticle || undefined} />
                        )}
                        {/* Other forms do not support editing yet, so they will behave as 'New' or might just ignore initialArticle since we didn't add it to them.
                            Ideally we should add initialArticle support to them too, or fallback to 'article' form for editing metadata.
                            For now, if it's not 'article', we might just show the 'article' form which allows editing Title/Content.
                            BUT, I forced the tab above.
                            Let's relax the tab force if it's not 'article' type?
                            Or, stick to 'article' form for all edits?
                            Actually, 'book', 'youtube', 'local' tabs are import forms. They don't have fields for Title/Content usually (except import params).
                            So using NewArticleForm for EDITING is probably the correct approach for all types (editing the result).
                        */}
                        {activeTab !== "article" && isEditing ? (
                            // Fallback to NewArticleForm for editing metadata of non-article types
                            <NewArticleForm onSave={handleSave} onCancel={handleClose} initialArticle={editingArticle || undefined} />
                        ) : (
                            <>
                                {activeTab === "book" && <BookImportForm onSave={handleSave} onCancel={handleClose} />}
                                {activeTab === "web" && canUseWebImport && <WebImportForm onSave={handleSave} onCancel={handleClose} />}
                                {activeTab === "textFile" && <TextFileImportForm onSave={handleSave} onCancel={handleClose} />}
                                {activeTab === "youtube" && canUseYouTubeImport && <YouTubeImportForm onSave={handleSave} onCancel={handleClose} />}
                                {activeTab === "local" && <LocalVideoImportForm onSave={handleSave} onCancel={handleClose} />}
                                {activeTab === "audio" && <LocalAudioImportForm onSave={handleSave} onCancel={handleClose} />}
                                {activeTab === "subtitle" && <LocalSubtitleImportForm onSave={handleSave} onCancel={handleClose} />}
                            </>
                        )}
                    </div>
                </div>
            </div>
        </Dialog>
        </MaterialImportActivityProvider>
    );
}

interface NewMaterialButtonProps {
    onSave?: (article: Article) => void;
}

export function NewMaterialButton({ onSave }: NewMaterialButtonProps) {
    const { t } = useTranslation();
    const [isOpen, setIsOpen] = useState(false);

    return (
        <>
            <Button onClick={() => setIsOpen(true)} className="gap-2">
                <Plus size={16} />
                {t("header.newMaterial")}
            </Button>
            <NewMaterialDialog
                isOpen={isOpen}
                onClose={() => setIsOpen(false)}
                onSave={onSave}
            />
        </>
    );
}

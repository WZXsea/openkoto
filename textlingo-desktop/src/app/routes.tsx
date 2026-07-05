import { LayoutGrid, List, RotateCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import { ArticleList } from "../components/features/ArticleList";
import { ArticleReader } from "../components/features/ArticleReader";
import { BookReader } from "../components/features/BookReader";
import { FavoritesPage } from "../components/features/FavoritesPage";
import { KtvExportPage } from "../components/features/KtvExportPage";
import { Button } from "../components/ui/button";
import type { Article } from "../lib/tauri";
import { getAppNavigationItem, type AppScreen, type MaterialViewMode } from "./navigation";

interface AppRoutesProps {
  activeScreen: AppScreen;
  articles: Article[];
  canUseKtvExport: boolean;
  isLoading: boolean;
  selectedArticle: Article | null;
  selectedIndex: number;
  viewMode: MaterialViewMode;
  onArticleUpdate: () => Promise<void>;
  onBackFromFavorites: () => void;
  onBackToList: () => void;
  onBackToReader: () => void;
  onDeleteArticle: (id: string) => Promise<void>;
  onEditArticle: (article: Article) => void;
  onNewMaterial: () => void;
  onNextArticle: () => void;
  onOpenKtvExport: () => void;
  onPreviousArticle: () => void;
  onRefresh: () => Promise<Article[]>;
  onSelectArticle: (article: Article) => void;
  onViewModeChange: (mode: MaterialViewMode) => void;
}

export function AppRoutes({
  activeScreen,
  articles,
  canUseKtvExport,
  isLoading,
  selectedArticle,
  selectedIndex,
  viewMode,
  onArticleUpdate,
  onBackFromFavorites,
  onBackToList,
  onBackToReader,
  onDeleteArticle,
  onEditArticle,
  onNewMaterial,
  onNextArticle,
  onOpenKtvExport,
  onPreviousArticle,
  onRefresh,
  onSelectArticle,
  onViewModeChange,
}: AppRoutesProps) {
  const { t } = useTranslation();
  const homeNavItem = getAppNavigationItem("home");

  if (selectedArticle) {
    if (activeScreen === "ktv-export" && canUseKtvExport) {
      return <KtvExportPage article={selectedArticle} onBack={onBackToReader} />;
    }

    if (selectedArticle.book_path) {
      return (
        <BookReader
          article={selectedArticle}
          onBack={onBackToList}
          onUpdate={onArticleUpdate}
        />
      );
    }

    return (
      <ArticleReader
        article={selectedArticle}
        onBack={onBackToList}
        onNext={onNextArticle}
        onPrev={onPreviousArticle}
        hasNext={selectedIndex < articles.length - 1}
        hasPrev={selectedIndex > 0}
        onUpdate={onArticleUpdate}
        onOpenKtvExport={canUseKtvExport ? onOpenKtvExport : undefined}
      />
    );
  }

  if (activeScreen === "favorites") {
    return (
      <FavoritesPage
        onBack={onBackFromFavorites}
        onSelectArticle={onSelectArticle}
      />
    );
  }

  return (
    <div className="h-full max-w-4xl mx-auto p-6 overflow-y-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-xl font-semibold">
          {t(homeNavItem.labelKey, homeNavItem.fallbackLabel).replace("我的文章", "我的素材")}
        </h2>
        <div className="flex items-center gap-2 bg-muted/50 p-1 rounded-lg border border-border">
          <Button
            variant={viewMode === "list" ? "secondary" : "ghost"}
            size="sm"
            onClick={() => onViewModeChange("list")}
            className="h-7 px-2"
            title={t("articleList.listView")}
          >
            <List size={14} />
          </Button>
          <Button
            variant={viewMode === "card" ? "secondary" : "ghost"}
            size="sm"
            onClick={() => onViewModeChange("card")}
            className="h-7 px-2"
            title={t("articleList.cardView")}
          >
            <LayoutGrid size={14} />
          </Button>
        </div>
        <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void onRefresh()}
            disabled={isLoading}
            title={t("common.refresh")}
          >
            <RotateCw size={16} className={isLoading ? "animate-spin" : ""} />
          </Button>
        </div>
      </div>
      <ArticleList
        articles={articles}
        isLoading={isLoading}
        onSelectArticle={onSelectArticle}
        onDelete={onDeleteArticle}
        onEdit={onEditArticle}
        onNewMaterial={onNewMaterial}
        onUpdate={onArticleUpdate}
        selectedId={undefined}
        viewMode={viewMode}
      />
    </div>
  );
}

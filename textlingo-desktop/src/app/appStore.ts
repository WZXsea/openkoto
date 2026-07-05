import { useCallback, useMemo, useRef, useState } from "react";

import type { Article, AppConfig } from "../lib/tauri";
import type { AppScreen, MaterialViewMode } from "./navigation";

type ReaderReturnScreen = "home" | "favorites";

export function useAppStore() {
  const [articles, setArticles] = useState<Article[]>([]);
  const [selectedArticle, setSelectedArticle] = useState<Article | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [editingArticle, setEditingArticle] = useState<Article | null>(null);
  const [isEditDialogOpen, setIsEditDialogOpen] = useState(false);
  const [viewMode, setViewMode] = useState<MaterialViewMode>("card");
  const [activeScreen, setActiveScreen] = useState<AppScreen>("home");
  const [readerReturnScreen, setReaderReturnScreen] = useState<ReaderReturnScreen>("home");
  const [showOnboarding, setShowOnboarding] = useState(false);
  const onboardingDismissedRef = useRef(false);

  const selectedIndex = useMemo(() => {
    if (!selectedArticle) return -1;
    return articles.findIndex((article) => article.id === selectedArticle.id);
  }, [articles, selectedArticle]);

  const showReaderArticle = useCallback((article: Article) => {
    setSelectedArticle(article);
    setActiveScreen("reader");
  }, []);

  const openArticle = useCallback((article: Article, options?: { returnScreen?: ReaderReturnScreen }) => {
    setReaderReturnScreen(options?.returnScreen ?? "home");
    showReaderArticle(article);
  }, [showReaderArticle]);

  const openArticleById = useCallback((articleId: string, options?: { returnScreen?: "home" | "favorites" }) => {
    const article = articles.find((item) => item.id === articleId);
    if (article) openArticle(article, options);
    return article ?? null;
  }, [articles, openArticle]);

  const openNextArticle = useCallback(() => {
    if (selectedIndex >= 0 && selectedIndex < articles.length - 1) {
      showReaderArticle(articles[selectedIndex + 1]);
    }
  }, [articles, selectedIndex, showReaderArticle]);

  const openPreviousArticle = useCallback(() => {
    if (selectedIndex > 0) {
      showReaderArticle(articles[selectedIndex - 1]);
    }
  }, [articles, selectedIndex, showReaderArticle]);

  const goHome = useCallback(() => {
    setSelectedArticle(null);
    setReaderReturnScreen("home");
    setActiveScreen("home");
  }, []);

  const openFavorites = useCallback(() => {
    setSelectedArticle(null);
    setReaderReturnScreen("favorites");
    setActiveScreen("favorites");
  }, []);

  const backFromFavorites = useCallback(() => {
    setReaderReturnScreen("home");
    setActiveScreen("home");
  }, []);

  const backToReaderList = useCallback(() => {
    setSelectedArticle(null);
    setActiveScreen(readerReturnScreen);
  }, [readerReturnScreen]);

  const openKtvExport = useCallback(() => {
    setActiveScreen("ktv-export");
  }, []);

  const backToReader = useCallback(() => {
    setActiveScreen("reader");
  }, []);

  const startCreateMaterial = useCallback(() => {
    setEditingArticle(null);
    setIsEditDialogOpen(true);
  }, []);

  const startEditMaterial = useCallback((article: Article) => {
    setEditingArticle(article);
    setIsEditDialogOpen(true);
  }, []);

  const closeMaterialDialog = useCallback(() => {
    setIsEditDialogOpen(false);
    setEditingArticle(null);
  }, []);

  const refreshSelectedArticle = useCallback((nextArticles: Article[]) => {
    if (!selectedArticle) return;
    const updatedArticle = nextArticles.find((article) => article.id === selectedArticle.id);
    if (updatedArticle) setSelectedArticle(updatedArticle);
  }, [selectedArticle]);

  const prependArticleIfMissing = useCallback((article: Article) => {
    setArticles((currentArticles) => (
      currentArticles.some((item) => item.id === article.id)
        ? currentArticles
        : [article, ...currentArticles]
    ));
  }, []);

  const hasDismissedOnboarding = useCallback(() => onboardingDismissedRef.current, []);

  const dismissOnboarding = useCallback(() => {
    onboardingDismissedRef.current = true;
    setShowOnboarding(false);
  }, []);

  return {
    activeScreen,
    articles,
    backFromFavorites,
    backToReader,
    backToReaderList,
    closeMaterialDialog,
    config,
    dismissOnboarding,
    editingArticle,
    goHome,
    hasDismissedOnboarding,
    isEditDialogOpen,
    isLoading,
    openArticle,
    openArticleById,
    openFavorites,
    openKtvExport,
    openNextArticle,
    openPreviousArticle,
    prependArticleIfMissing,
    refreshSelectedArticle,
    selectedArticle,
    selectedIndex,
    setActiveScreen,
    setArticles,
    setConfig,
    setIsLoading,
    setShowOnboarding,
    setViewMode,
    showOnboarding,
    startCreateMaterial,
    startEditMaterial,
    viewMode,
  };
}

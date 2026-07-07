import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { BookOpen, Plus } from "lucide-react";
import { useTranslation } from "react-i18next";

import { ApiQuickSwitcher } from "../components/features/ApiQuickSwitcher";
import { DropImportOverlay, type DropImportStatus } from "../components/features/DropImportOverlay";
import { NewMaterialDialog } from "../components/features/NewMaterialDialog";
import { OnboardingDialog } from "../components/features/OnboardingDialog";
import { SettingsButton } from "../components/features/SettingsDialog";
import { UpdateChecker } from "../components/features/UpdateChecker";
import { Button } from "../components/ui/button";
import { getApiClient } from "../lib/api";
import { importDroppedPath, isSupportedDropPath, getFileName } from "../lib/dropImport";
import { useAgentOpenMaterialListener } from "../lib/hooks/useAgentOpenMaterialListener";
import { isPhase1CapabilityEnabled } from "../lib/phase1Capabilities";
import type { Article, AppConfig } from "../lib/tauri";
import { useAppStore } from "./appStore";
import { getAppNavigationItem } from "./navigation";
import { AppRoutes } from "./routes";

export function AppShell() {
  const { t } = useTranslation();
  const favoritesNavItem = getAppNavigationItem("favorites");
  const FavoritesIcon = favoritesNavItem.icon;
  const store = useAppStore();
  const {
    dismissOnboarding,
    hasDismissedOnboarding,
    openArticle,
    openArticleById,
    prependArticleIfMissing,
    refreshSelectedArticle,
    setArticles,
    setConfig,
    setIsLoading,
    setShowOnboarding,
  } = store;

  const [isDragging, setIsDragging] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [importingCount, setImportingCount] = useState(0);
  const [dropStatus, setDropStatus] = useState<DropImportStatus | null>(null);
  const dropStatusTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isImportingRef = useRef(false);
  const isMountedRef = useRef(true);

  const hasConfig = Boolean(store.config?.model_configs?.length && store.config.active_model_id);
  const canUseKtvExport = isPhase1CapabilityEnabled("ktvExport");
  const canCheckForUpdates = isPhase1CapabilityEnabled("updateCheck");
  const isFavoritesActive = store.activeScreen === "favorites";

  const clearDropStatusTimer = useCallback(() => {
    if (dropStatusTimer.current) {
      clearTimeout(dropStatusTimer.current);
      dropStatusTimer.current = null;
    }
  }, []);

  const scheduleStatusClear = useCallback(() => {
    clearDropStatusTimer();
    dropStatusTimer.current = setTimeout(() => {
      if (isMountedRef.current) setDropStatus(null);
      dropStatusTimer.current = null;
    }, 3500);
  }, [clearDropStatusTimer]);

  const loadData = useCallback(async (): Promise<Article[]> => {
    if (isMountedRef.current) setIsLoading(true);
    try {
      const [configResult, articlesResult] = await Promise.all([
        invoke<AppConfig | null>("get_config"),
        invoke<Article[]>("list_articles_cmd"),
      ]);
      if (isMountedRef.current) {
        setConfig(configResult);
        const hasSavedModelConfigs = Boolean(configResult?.model_configs?.length);
        const shouldShowOnboarding =
          !hasDismissedOnboarding() &&
          (!configResult || (!configResult.onboarding_completed && !hasSavedModelConfigs));

        if (configResult) {
          getApiClient(configResult);
        }
        setShowOnboarding(shouldShowOnboarding);
        setArticles(articlesResult);
      }
      return articlesResult;
    } catch (err) {
      console.error("Failed to load data:", err);
      return [];
    } finally {
      if (isMountedRef.current) setIsLoading(false);
    }
  }, [hasDismissedOnboarding, setArticles, setConfig, setIsLoading, setShowOnboarding]);

  const dropActionsRef = useRef({
    loadData,
    openArticle,
    scheduleStatusClear,
    t,
  });

  useEffect(() => {
    dropActionsRef.current = {
      loadData,
      openArticle,
      scheduleStatusClear,
      t,
    };
  }, [loadData, openArticle, scheduleStatusClear, t]);

  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
      isImportingRef.current = false;
      clearDropStatusTimer();
    };
  }, [clearDropStatusTimer]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  useEffect(() => {
    let isCancelled = false;
    let unlisten: (() => void) | undefined;

    try {
      void getCurrentWebview()
        .onDragDropEvent(async (event) => {
          const isDropActive = () => !isCancelled && isMountedRef.current;
          if (!isDropActive()) return;

          const dropActions = dropActionsRef.current;
          const payload = event.payload;
          if (payload.type === "enter" || payload.type === "over") {
            if (!isImportingRef.current) setIsDragging(true);
            return;
          }

          if (payload.type === "leave") {
            setIsDragging(false);
            return;
          }

          if (payload.type !== "drop") return;

          setIsDragging(false);
          if (isImportingRef.current) return;

          const paths = (payload.paths || []).filter(isSupportedDropPath);
          const unsupported = (payload.paths || []).filter((p) => !isSupportedDropPath(p));
          if (paths.length === 0) {
            setDropStatus({
              ok: 0,
              errors: [dropActions.t("dropImport.unsupported", "不支持的文件类型: {{name}}", {
                name: unsupported.map(getFileName).join(", ") || "?",
              })],
            });
            dropActions.scheduleStatusClear();
            return;
          }

          isImportingRef.current = true;
          setIsImporting(true);
          setImportingCount(paths.length);
          const imported: Article[] = [];
          const errors: string[] = [];
          for (const path of paths) {
            try {
              const article = await importDroppedPath(path);
              if (!isDropActive()) {
                isImportingRef.current = false;
                return;
              }
              imported.push(article);
            } catch (err) {
              if (!isDropActive()) {
                isImportingRef.current = false;
                return;
              }
              const msg = err instanceof Error ? err.message : String(err);
              errors.push(
                msg.startsWith("unsupported:")
                  ? dropActions.t("dropImport.unsupported", "不支持的文件类型: {{name}}", { name: msg.slice("unsupported:".length) })
                  : dropActions.t("dropImport.failed", "导入失败: {{error}}", { error: msg }),
              );
            }
          }

          if (!isDropActive()) {
            isImportingRef.current = false;
            return;
          }
          setIsImporting(false);
          isImportingRef.current = false;

          const freshArticles = await dropActions.loadData();
          if (!isDropActive()) return;
          if (imported.length === 1 && errors.length === 0) {
            const article = freshArticles.find((item) => item.id === imported[0].id) ?? imported[0];
            dropActions.openArticle(article, { returnScreen: "home" });
          }
          setDropStatus({ ok: imported.length, errors });
          dropActions.scheduleStatusClear();
        })
        .then((fn) => {
          if (isCancelled) {
            fn();
            return;
          }
          unlisten = fn;
        })
        .catch(() => {});
    } catch {
      // Ignore drag/drop wiring outside the Tauri runtime, such as component tests.
    }

    return () => {
      isCancelled = true;
      unlisten?.();
    };
  }, []);

  const handleAgentOpenMaterial = useCallback((materialId: string) => {
    const existingArticle = openArticleById(materialId, { returnScreen: "home" });
    if (existingArticle) return;

    void invoke<Article>("get_article", { id: materialId })
      .then((article) => {
        prependArticleIfMissing(article);
        openArticle(article, { returnScreen: "home" });
      })
      .catch((error) => {
        console.error("Failed to open material from agent event:", error);
      });
  }, [openArticle, openArticleById, prependArticleIfMissing]);

  useAgentOpenMaterialListener(handleAgentOpenMaterial);

  const handleSelectArticle = useCallback((article: Article) => {
    openArticle(article, {
      returnScreen: store.activeScreen === "favorites" ? "favorites" : "home",
    });
  }, [openArticle, store.activeScreen]);

  const handleArticleUpdate = useCallback(async () => {
    const refreshedArticles = await loadData();
    refreshSelectedArticle(refreshedArticles);
  }, [loadData, refreshSelectedArticle]);

  const handleDeleteArticle = useCallback(async (id: string) => {
    try {
      await invoke("delete_article_cmd", { id });
      await loadData();
    } catch (err) {
      console.error("App: Failed to delete article", err);
    }
  }, [loadData]);

  const handleOnboardingFinish = useCallback(() => {
    dismissOnboarding();
    void loadData();
  }, [dismissOnboarding, loadData]);

  if (store.isLoading) {
    return (
      <div className="h-screen flex items-center justify-center bg-background">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary mx-auto mb-4" />
          <p className="text-muted-foreground">{t("app.loading")}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col bg-background text-foreground">
      <DropImportOverlay
        isDragging={isDragging}
        isImporting={isImporting}
        importingCount={importingCount}
        status={dropStatus}
      />

      {!store.selectedArticle && (
        <header className="flex items-center justify-between px-6 py-4 border-b border-border bg-card/50 backdrop-blur-sm supports-[backdrop-filter]:bg-card/50">
          <div className="flex items-center gap-3 cursor-pointer hover:opacity-80 transition-opacity" onClick={store.goHome}>
            <div className="flex items-center justify-center w-10 h-10 rounded-lg bg-primary text-primary-foreground">
              <BookOpen size={20} />
            </div>
            <div>
              <h1 className="text-lg font-semibold">{t("app.title")}</h1>
              <p className="text-xs text-muted-foreground">{t("app.subtitle")}</p>
            </div>
          </div>

          <div className="flex items-center gap-3">
            {!hasConfig && (
              <div className="px-3 py-1.5 bg-yellow-500/10 border border-yellow-500/50 rounded-lg text-yellow-600 dark:text-yellow-400 text-sm">
                {t("header.localReadingReady")}
              </div>
            )}

            <Button
              variant={isFavoritesActive ? "default" : "secondary"}
              onClick={store.openFavorites}
              className="gap-2"
            >
              <FavoritesIcon size={16} className={isFavoritesActive ? "fill-current" : ""} />
              {t(favoritesNavItem.labelKey, favoritesNavItem.fallbackLabel)}
            </Button>

            <Button onClick={store.startCreateMaterial} className="gap-2">
              <Plus size={16} />
              {t("header.newMaterial")}
            </Button>
            <SettingsButton onSave={handleArticleUpdate} />
          </div>
        </header>
      )}

      <main className="flex-1 overflow-hidden">
        <NewMaterialDialog
          isOpen={store.isEditDialogOpen}
          onClose={store.closeMaterialDialog}
          onSave={handleArticleUpdate}
          editingArticle={store.editingArticle}
        />
        <OnboardingDialog
          isOpen={store.showOnboarding}
          onFinish={handleOnboardingFinish}
        />
        {canCheckForUpdates && <UpdateChecker />}
        <AppRoutes
          activeScreen={store.activeScreen}
          articles={store.articles}
          canUseKtvExport={canUseKtvExport}
          isLoading={store.isLoading}
          selectedArticle={store.selectedArticle}
          selectedIndex={store.selectedIndex}
          viewMode={store.viewMode}
          onArticleUpdate={handleArticleUpdate}
          onBackFromFavorites={store.backFromFavorites}
          onBackToList={store.backToReaderList}
          onBackToReader={store.backToReader}
          onDeleteArticle={handleDeleteArticle}
          onEditArticle={store.startEditMaterial}
          onNewMaterial={store.startCreateMaterial}
          onNextArticle={store.openNextArticle}
          onOpenKtvExport={store.openKtvExport}
          onPreviousArticle={store.openPreviousArticle}
          onRefresh={loadData}
          onSelectArticle={handleSelectArticle}
          onViewModeChange={store.setViewMode}
        />
      </main>

      <footer className="px-6 py-3 border-t border-border bg-card/50 text-xs text-muted-foreground">
        <div className="flex items-center justify-between">
          <p>OpenKoto v{__APP_VERSION__}</p>
          <ApiQuickSwitcher config={store.config} onConfigChange={() => { void loadData(); }} />
        </div>
      </footer>
    </div>
  );
}

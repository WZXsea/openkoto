import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { BookOpen, Plus } from "lucide-react";
import { useTranslation } from "react-i18next";

import { AccountMenu } from "../components/features/AccountMenu";
import { BackendConnectionGate } from "../components/features/BackendConnectionGate";
import { DropImportOverlay, type DropImportStatus } from "../components/features/DropImportOverlay";
import { NewMaterialDialog } from "../components/features/NewMaterialDialog";
import { OnboardingDialog } from "../components/features/OnboardingDialog";
import { SettingsButton } from "../components/features/SettingsDialog";
import { UpdateChecker } from "../components/features/UpdateChecker";
import { Button } from "../components/ui/button";
import { getApiClient } from "../lib/api";
import { importDroppedPath, isSupportedDropPath, getFileName } from "../lib/dropImport";
import { applyFontSettings } from "../lib/fontSettings";
import { useAgentOpenMaterialListener } from "../lib/hooks/useAgentOpenMaterialListener";
import { isPhase1CapabilityEnabled } from "../lib/phase1Capabilities";
import type { Article, AppConfig, BackendSessionCheck } from "../lib/tauri";
import type { Annotation } from "../types";
import type { AssistantSourceReference } from "../features/assistant";
import { useAppStore } from "./appStore";
import { APP_NAVIGATION_ITEMS } from "./navigation";
import { AppRoutes } from "./routes";

const STARTUP_INVOKE_TIMEOUT_MS = 8_000;
const PACKAGED_BACKEND_STARTUP_TIMEOUT_MS = 30_000;

interface PackagedBackendStartupStatus {
  enabled: boolean;
  running: boolean;
  message: string | null;
}

function invokeWithTimeout<T>(command: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      reject(new Error(`${command} timed out after ${STARTUP_INVOKE_TIMEOUT_MS}ms`));
    }, STARTUP_INVOKE_TIMEOUT_MS);

    void invoke<T>(command).then(
      (value) => {
        window.clearTimeout(timeout);
        resolve(value);
      },
      (error) => {
        window.clearTimeout(timeout);
        reject(error);
      },
    );
  });
}

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

async function waitForPackagedBackend(): Promise<PackagedBackendStartupStatus | null> {
  const deadline = Date.now() + PACKAGED_BACKEND_STARTUP_TIMEOUT_MS;

  while (Date.now() < deadline) {
    let status: PackagedBackendStartupStatus;
    try {
      status = await invokeWithTimeout<PackagedBackendStartupStatus>("packaged_backend_status_cmd");
    } catch {
      // Core/dev builds and component tests may not provide the packaged runtime.
      return null;
    }

    if (!status || typeof status.enabled !== "boolean" || typeof status.running !== "boolean") {
      return null;
    }
    if (!status.enabled || status.running || status.message) return status;
    await sleep(200);
  }

  return {
    enabled: true,
    running: false,
    message: "本地 Backend 启动超时，请重试或检查应用日志。",
  };
}

export function AppShell() {
  const { t } = useTranslation();
  const store = useAppStore();
  const {
    dismissOnboarding,
    goHome,
    hasDismissedOnboarding,
    openArticle,
    openArticleById,
    openAnnotationSource,
    openLearningItem,
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
  const [backendStatus, setBackendStatus] = useState<BackendSessionCheck | null>(null);
  const [isCheckingBackend, setIsCheckingBackend] = useState(true);
  const [isLoggingOut, setIsLoggingOut] = useState(false);
  const dropStatusTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isImportingRef = useRef(false);
  const isMountedRef = useRef(true);
  const backendAuthenticatedRef = useRef(false);

  const hasConfig = Boolean(store.config?.model_configs?.length && store.config.active_model_id);
  const isBackendAuthenticated = Boolean(backendStatus?.authenticated);
  const canUseKtvExport = isPhase1CapabilityEnabled("ktvExport");
  const canCheckForUpdates = isPhase1CapabilityEnabled("updateCheck");
  const sidebarItems = APP_NAVIGATION_ITEMS.filter((item) => item.id !== "favorites");

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
      const packagedBackendStatus = await waitForPackagedBackend();
      const [configOutcome, sessionOutcome] = await Promise.allSettled([
        invokeWithTimeout<AppConfig | null>("get_config"),
        invokeWithTimeout<BackendSessionCheck>("backend_check_session_cmd"),
      ]);
      const configResult = configOutcome.status === "fulfilled" ? configOutcome.value : null;

      if (configOutcome.status === "rejected") {
        console.error("Failed to load config:", configOutcome.reason);
      }

      if (sessionOutcome.status === "rejected") {
        const message = sessionOutcome.reason instanceof Error
          ? sessionOutcome.reason.message
          : String(sessionOutcome.reason);
        if (isMountedRef.current) {
          applyFontSettings(configResult);
          setConfig(configResult);
          setArticles([]);
          setBackendStatus({
            configured: Boolean(configResult?.backend_url),
            connected: false,
            authenticated: false,
            backend_url: configResult?.backend_url ?? "http://127.0.0.1:19421",
            user: null,
            error: `Backend 启动检查失败：${message}`,
          });
        }
        return [];
      }

      const sessionResult = sessionOutcome.value;
      const packagedBackendError = packagedBackendStatus?.enabled && !packagedBackendStatus.running
        ? packagedBackendStatus.message
        : null;
      const effectiveSessionResult = !sessionResult.configured && packagedBackendError
        ? {
            ...sessionResult,
            configured: true,
            backend_url: configResult?.backend_url ?? "http://127.0.0.1:19421",
            error: packagedBackendError,
          }
        : sessionResult;
      if (isMountedRef.current) {
        applyFontSettings(configResult);
        setConfig(configResult);
        setBackendStatus(effectiveSessionResult);
        const hasSavedModelConfigs = Boolean(configResult?.model_configs?.length);
        const shouldShowOnboarding =
          effectiveSessionResult.authenticated &&
          !hasDismissedOnboarding() &&
          (!configResult || (!configResult.onboarding_completed && !hasSavedModelConfigs));

        if (configResult) {
          getApiClient(configResult);
        }
        setShowOnboarding(shouldShowOnboarding);
      }

      if (!effectiveSessionResult.authenticated) {
        if (isMountedRef.current) setArticles([]);
        return [];
      }

      const articlesResult = await invokeWithTimeout<Article[]>("list_articles_cmd");
      if (isMountedRef.current) {
        setArticles(articlesResult);
      }
      return articlesResult;
    } catch (err) {
      console.error("Failed to load data:", err);
      return [];
    } finally {
      if (isMountedRef.current) {
        setIsLoading(false);
        setIsCheckingBackend(false);
      }
    }
  }, [hasDismissedOnboarding, setArticles, setConfig, setIsLoading, setShowOnboarding]);

  const handleBackendAuthenticated = useCallback(async (config: AppConfig) => {
    if (isMountedRef.current) {
      applyFontSettings(config);
      setConfig(config);
      getApiClient(config);
      setIsCheckingBackend(true);
    }
    await loadData();
  }, [loadData, setConfig]);

  const handleBackendLogout = useCallback(async () => {
    if (isLoggingOut) return;
    setIsLoggingOut(true);
    try {
      const configResult = await invoke<AppConfig>("backend_logout_cmd");
      if (!isMountedRef.current) return;

      applyFontSettings(configResult);
      setConfig(configResult);
      setArticles([]);
      goHome();
      setShowOnboarding(false);
      setBackendStatus({
        configured: Boolean(configResult.backend_url || backendStatus?.backend_url),
        connected: Boolean(backendStatus?.connected),
        authenticated: false,
        backend_url: configResult.backend_url || backendStatus?.backend_url || null,
        user: null,
        error: null,
      });
      setIsCheckingBackend(false);
    } catch (error) {
      console.error("Failed to logout backend account:", error);
      await loadData();
    } finally {
      if (isMountedRef.current) setIsLoggingOut(false);
    }
  }, [
    backendStatus?.backend_url,
    backendStatus?.connected,
    goHome,
    isLoggingOut,
    loadData,
    setArticles,
    setConfig,
    setShowOnboarding,
  ]);

  const dropActionsRef = useRef({
    activeScreen: store.activeScreen,
    loadData,
    openArticle,
    scheduleStatusClear,
    t,
  });

  useEffect(() => {
    dropActionsRef.current = {
      activeScreen: store.activeScreen,
      loadData,
      openArticle,
      scheduleStatusClear,
      t,
    };
  }, [loadData, openArticle, scheduleStatusClear, store.activeScreen, t]);

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
    backendAuthenticatedRef.current = isBackendAuthenticated;
  }, [isBackendAuthenticated]);

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
          if (!backendAuthenticatedRef.current) {
            if (payload.type === "drop" || payload.type === "leave") {
              setIsDragging(false);
            }
            return;
          }

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
              const result = await importDroppedPath(path);
              if (!isDropActive()) {
                isImportingRef.current = false;
                return;
              }
              if (result.kind === "conflict") {
                errors.push(dropActions.t("dropImport.duplicate", "发现重复素材，导入已暂停。请在导入任务中选择取消、打开已有、替换或保留副本。"));
                continue;
              }
              imported.push(result.article);
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
            dropActions.openArticle(article, { returnScreen: dropActions.activeScreen === "materials" ? "materials" : "home" });
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
    if (!isBackendAuthenticated) return;

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
  }, [isBackendAuthenticated, openArticle, openArticleById, prependArticleIfMissing]);

  useAgentOpenMaterialListener(handleAgentOpenMaterial);

  const handleSelectArticle = useCallback((article: Article) => {
    openArticle(article, {
      returnScreen: store.activeScreen === "materials" || store.activeScreen === "assistant" || store.activeScreen === "favorites" || store.activeScreen === "learning" || store.activeScreen === "annotations"
        ? store.activeScreen
        : "home",
    });
  }, [openArticle, store.activeScreen]);

  const handleNavigateAnnotationSource = useCallback((annotation: Annotation) => {
    const existing = store.articles.find((article) => article.id === annotation.material_id);
    if (existing) {
      openAnnotationSource(existing, annotation);
      return;
    }
    void invoke<Article>("get_article", { id: annotation.material_id })
      .then((article) => {
        prependArticleIfMissing(article);
        openAnnotationSource(article, annotation);
      })
      .catch((error) => {
        console.error("Failed to open annotation source:", error);
      });
  }, [openAnnotationSource, prependArticleIfMissing, store.articles]);

  const handleNavigateAssistantSource = useCallback((reference: AssistantSourceReference) => {
    if (reference.target === "learning_item") {
      openLearningItem(reference.learningItemId);
      return;
    }
    const openSource = (article: Article) => {
      prependArticleIfMissing(article);
      if (!reference.locator) {
        openArticle(article, { returnScreen: "assistant" });
        return;
      }
      const now = new Date().toISOString();
      const annotation: Annotation = {
        id: `assistant-source:${reference.articleId}:task`,
        material_id: reference.articleId,
        kind: "excerpt",
        locator: reference.locator,
        source_text: reference.locator.quote?.exact ?? "",
        tags: [],
        learning_item_id: null,
        created_at: now,
        updated_at: now,
      };
      openAnnotationSource(article, annotation, { returnScreen: "assistant" });
    };

    const existing = store.articles.find((article) => article.id === reference.articleId);
    if (existing) {
      openSource(existing);
      return;
    }
    void invoke<Article>("get_article", { id: reference.articleId })
      .then(openSource)
      .catch((error) => {
        console.error("Failed to open Assistant source:", error);
      });
  }, [openAnnotationSource, openArticle, openLearningItem, prependArticleIfMissing, store.articles]);

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

  if (!backendStatus?.authenticated) {
    return (
      <BackendConnectionGate
        config={store.config}
        status={backendStatus}
        isChecking={isCheckingBackend}
        onRetry={loadData}
        onAuthenticated={handleBackendAuthenticated}
      />
    );
  }

  const navigateTo = (screen: "home" | "materials" | "assistant" | "learning" | "annotations") => {
    if (screen === "home") store.goHome();
    else if (screen === "materials") store.openMaterials();
    else if (screen === "assistant") store.openAssistant();
    else if (screen === "learning") store.openLearning();
    else store.openAnnotations();
  };

  return (
    <div className="h-screen flex bg-background text-foreground">
      <DropImportOverlay
        isDragging={isDragging}
        isImporting={isImporting}
        importingCount={importingCount}
        status={dropStatus}
      />

      {!store.selectedArticle && (
        <aside className="flex w-[72px] shrink-0 flex-col border-r border-sidebar-border bg-sidebar/75 px-2 py-4 text-sidebar-foreground backdrop-blur-sm md:w-56 md:px-3" aria-label="主导航">
          <button type="button" onClick={store.goHome} className="mb-7 flex items-center gap-3 rounded-xl px-2 py-1.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring md:px-3">
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-primary text-primary-foreground"><BookOpen size={18} /></span>
            <span className="hidden min-w-0 md:block"><span className="block truncate text-sm font-semibold">{t("app.title")}</span><span className="block truncate text-[11px] text-muted-foreground">专注阅读与学习</span></span>
          </button>

          <nav className="space-y-1">
            {sidebarItems.map((item) => {
              const Icon = item.icon;
              const active = item.id === "learning"
                ? store.activeScreen === "learning" || store.activeScreen === "favorites"
                : store.activeScreen === item.id;
              const label = t(item.labelKey, item.fallbackLabel);
              return (
                <button
                  key={item.id}
                  type="button"
                  aria-current={active ? "page" : undefined}
                  aria-label={label}
                  title={label}
                  onClick={() => navigateTo(item.id as "home" | "materials" | "assistant" | "learning" | "annotations")}
                  className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring ${active ? "bg-sidebar-accent text-sidebar-accent-foreground" : "text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground"}`}
                >
                  <Icon size={18} className="shrink-0" /><span className="hidden md:inline">{label}</span>
                </button>
              );
            })}
          </nav>

          <Button onClick={store.startCreateMaterial} className="mt-5 gap-2 md:justify-start" size="sm" aria-label={t("header.newMaterial", "导入素材")} title={t("header.newMaterial", "导入素材")}><Plus size={16} /><span className="hidden md:inline">{t("header.newMaterial", "导入素材")}</span></Button>

          <div className="mt-auto space-y-1 overflow-hidden border-t border-sidebar-border pt-3">
            {!hasConfig && <p className="hidden px-3 pb-2 text-[11px] leading-4 text-muted-foreground md:block">{t("header.localReadingReady")}</p>}
            <SettingsButton onSave={handleArticleUpdate} compact />
            <AccountMenu user={backendStatus.user} backendUrl={backendStatus.backend_url} isLoggingOut={isLoggingOut} onLogout={handleBackendLogout} onSwitchAccount={handleBackendLogout} compact />
            <p className="hidden px-3 pt-2 text-[10px] text-muted-foreground md:block">OpenKoto v{__APP_VERSION__}</p>
          </div>
        </aside>
      )}

      <main className="min-w-0 flex-1 overflow-hidden">
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
          activeAnnotation={store.activeAnnotation}
          articles={store.articles}
          canUseKtvExport={canUseKtvExport}
          isLoading={store.isLoading}
          focusedLearningItemId={store.focusedLearningItemId}
          selectedArticle={store.selectedArticle}
          selectedIndex={store.selectedIndex}
          viewMode={store.viewMode}
          materialFilters={store.materialFilters}
          materialsScrollTop={store.materialsScrollTop}
          onArticleUpdate={handleArticleUpdate}
          onBackFromFavorites={store.backFromFavorites}
          onBackToList={store.backToReaderList}
          onBackToReader={store.backToReader}
          onDeleteArticle={handleDeleteArticle}
          onEditArticle={store.startEditMaterial}
          onNewMaterial={store.startCreateMaterial}
          onOpenFavorites={store.openFavorites}
          onOpenLearning={store.openLearning}
          onOpenMaterials={store.openMaterials}
          onNextArticle={store.openNextArticle}
          onNavigateAnnotationSource={handleNavigateAnnotationSource}
          onNavigateAssistantSource={handleNavigateAssistantSource}
          onOpenKtvExport={store.openKtvExport}
          onPreviousArticle={store.openPreviousArticle}
          onRefresh={loadData}
          onSelectArticle={handleSelectArticle}
          onMaterialFiltersChange={store.setMaterialFilters}
          onMaterialsScrollTopChange={store.setMaterialsScrollTop}
          onViewModeChange={store.setViewMode}
        />
      </main>
    </div>
  );
}

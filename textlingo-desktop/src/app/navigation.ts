import type { LucideIcon } from "lucide-react";
import { BookCheck, BookOpen, Highlighter, Star } from "lucide-react";

export type AppScreen = "home" | "favorites" | "annotations" | "learning" | "reader" | "ktv-export";
export type MaterialViewMode = "list" | "card";

export interface AppNavigationItem {
  id: Exclude<AppScreen, "reader" | "ktv-export">;
  labelKey: string;
  fallbackLabel: string;
  icon: LucideIcon;
}

export const APP_NAVIGATION_ITEMS: AppNavigationItem[] = [
  {
    id: "home",
    labelKey: "articleList.title",
    fallbackLabel: "我的素材",
    icon: BookOpen,
  },
  {
    id: "favorites",
    labelKey: "header.favorites",
    fallbackLabel: "收藏夹",
    icon: Star,
  },
  {
    id: "annotations",
    labelKey: "header.annotations",
    fallbackLabel: "批注",
    icon: Highlighter,
  },
  {
    id: "learning",
    labelKey: "header.learningWorkbench",
    fallbackLabel: "学习工作台",
    icon: BookCheck,
  },
];

export function getAppNavigationItem(id: AppNavigationItem["id"]) {
  return APP_NAVIGATION_ITEMS.find((item) => item.id === id) ?? APP_NAVIGATION_ITEMS[0];
}

export function isLibraryScreen(screen: AppScreen) {
  return screen === "home" || screen === "favorites" || screen === "annotations" || screen === "learning";
}

export function isReaderScreen(screen: AppScreen) {
  return screen === "reader" || screen === "ktv-export";
}

import type { LucideIcon } from "lucide-react";
import { BookOpen, Star } from "lucide-react";

export type AppScreen = "home" | "favorites" | "reader" | "ktv-export";
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
];

export function getAppNavigationItem(id: AppNavigationItem["id"]) {
  return APP_NAVIGATION_ITEMS.find((item) => item.id === id) ?? APP_NAVIGATION_ITEMS[0];
}

export function isLibraryScreen(screen: AppScreen) {
  return screen === "home" || screen === "favorites";
}

export function isReaderScreen(screen: AppScreen) {
  return screen === "reader" || screen === "ktv-export";
}

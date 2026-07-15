import type { LucideIcon } from "lucide-react";
import { BookCheck, BookOpen, Bot, Highlighter, House, Library } from "lucide-react";

export type AppScreen = "home" | "materials" | "assistant" | "favorites" | "annotations" | "learning" | "reader" | "ktv-export";
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
    labelKey: "navigation.home",
    fallbackLabel: "首页",
    icon: House,
  },
  {
    id: "materials",
    labelKey: "navigation.materials",
    fallbackLabel: "素材库",
    icon: Library,
  },
  {
    id: "assistant",
    labelKey: "navigation.assistant",
    fallbackLabel: "Assistant",
    icon: Bot,
  },
  {
    id: "learning",
    labelKey: "header.learningWorkbench",
    fallbackLabel: "学习",
    icon: BookCheck,
  },
  {
    id: "annotations",
    labelKey: "header.annotations",
    fallbackLabel: "笔记",
    icon: Highlighter,
  },
  {
    id: "favorites",
    labelKey: "header.favorites",
    fallbackLabel: "词包与已收录",
    icon: BookOpen,
  },
];

export function getAppNavigationItem(id: AppNavigationItem["id"]) {
  return APP_NAVIGATION_ITEMS.find((item) => item.id === id) ?? APP_NAVIGATION_ITEMS[0];
}

export function isLibraryScreen(screen: AppScreen) {
  return screen === "home" || screen === "materials" || screen === "assistant" || screen === "favorites" || screen === "annotations" || screen === "learning";
}

export function isReaderScreen(screen: AppScreen) {
  return screen === "reader" || screen === "ktv-export";
}

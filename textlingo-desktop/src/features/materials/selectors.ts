import type { Article } from "../../types";
import {
  type MaterialTag,
  type MaterialArticle,
  type MaterialFilters,
  type MaterialType,
  type ReadingStatus,
} from "./types";

const AUDIO_EXTENSIONS = new Set(["mp3", "wav", "m4a", "aac", "flac", "ogg", "wma"]);

export function asMaterialArticle(article: Article): MaterialArticle {
  return article as MaterialArticle;
}

export function getMaterialType(article: MaterialArticle): MaterialType {
  if (article.source_type === "web") return "web";
  if (article.source_type === "youtube" || article.source_type === "local_video") return "video";
  if (article.source_type === "audio") return "audio";
  if (article.source_type === "book" || article.book_path) return "book";
  if (article.source_type === "text_file") return "text";
  if (article.media_path) {
    const extension = article.media_path.split(".").pop()?.toLowerCase();
    return extension && AUDIO_EXTENSIONS.has(extension) ? "audio" : "video";
  }
  return "article";
}

function isReadingProgress(value: MaterialArticle["reading_progress"]): value is NonNullable<Exclude<MaterialArticle["reading_progress"], number>> {
  return typeof value === "object" && value !== null;
}

export function getMaterialTagLabels(article: MaterialArticle): string[] {
  return (article.tags || []).flatMap((tag) => {
    if (typeof tag === "string") return tag.trim() ? [tag] : [];
    const objectTag = tag as MaterialTag;
    const label = objectTag.name || objectTag.label || objectTag.value || objectTag.id;
    return label?.trim() ? [label] : [];
  });
}

function getProgressRatio(article: MaterialArticle): number | null {
  const nested = isReadingProgress(article.reading_progress) ? article.reading_progress : undefined;
  const value = nested?.progress_ratio ?? article.progress_ratio ?? article.reading_progress;
  if (typeof value !== "number" || !Number.isFinite(value)) return null;
  // Existing local drafts used 0..100. Backend values use progress_ratio 0..1.
  return Math.max(0, Math.min(1, value > 1 ? value / 100 : value));
}

export function getReadingStatus(article: MaterialArticle): ReadingStatus {
  const nested = isReadingProgress(article.reading_progress) ? article.reading_progress : undefined;
  if (article.archived_at || nested?.status === "archived" || article.reading_status === "archived" || article.status === "archived") return "archived";
  if (nested?.status === "reading") return "in_progress";
  if (nested?.status) return nested.status;
  if (article.reading_status) return article.reading_status;
  if (article.status) return article.status;
  if (nested?.completed_at || article.completed_at || getProgressRatio(article) === 1) return "completed";
  if ((getProgressRatio(article) ?? 0) > 0) return "in_progress";
  return "unread";
}

export function getMaterialProgress(article: MaterialArticle): number | null {
  const ratio = getProgressRatio(article);
  return ratio === null ? null : Math.round(ratio * 100);
}

export function getMaterialOpenedAt(article: MaterialArticle): string {
  const nested = isReadingProgress(article.reading_progress) ? article.reading_progress : undefined;
  return nested?.last_opened_at || article.last_opened_at || article.created_at;
}

function timestamp(value: string | undefined): number {
  const parsed = value ? Date.parse(value) : Number.NaN;
  return Number.isNaN(parsed) ? 0 : parsed;
}

export function getMaterialTags(articles: Article[]): string[] {
  return [...new Set(articles.flatMap((article) => getMaterialTagLabels(asMaterialArticle(article))))]
    .sort((left, right) => left.localeCompare(right));
}

export function filterAndSortMaterials(articles: Article[], filters: MaterialFilters): MaterialArticle[] {
  const normalizedQuery = filters.query.trim().toLocaleLowerCase();
  const createdFrom = filters.createdFrom ? Date.parse(`${filters.createdFrom}T00:00:00`) : Number.NEGATIVE_INFINITY;
  const createdTo = filters.createdTo ? Date.parse(`${filters.createdTo}T23:59:59.999`) : Number.POSITIVE_INFINITY;

  return articles
    .map(asMaterialArticle)
    .filter((article) => {
      const searchable = [article.title, article.content, article.source_url, article.source_name, ...getMaterialTagLabels(article)]
        .filter(Boolean)
        .join(" ")
        .toLocaleLowerCase();
      return (!normalizedQuery || searchable.includes(normalizedQuery))
        && (filters.type === "all" || getMaterialType(article) === filters.type)
        && (filters.readingStatus === "all" || getReadingStatus(article) === filters.readingStatus)
        && (filters.tag === "all" || getMaterialTagLabels(article).includes(filters.tag))
        && timestamp(article.created_at) >= createdFrom
        && timestamp(article.created_at) <= createdTo;
    })
    .sort((left, right) => {
      if (filters.sort === "title") return left.title.localeCompare(right.title);
      if (filters.sort === "progress") return (getMaterialProgress(right) ?? -1) - (getMaterialProgress(left) ?? -1);
      if (filters.sort === "created") return timestamp(right.created_at) - timestamp(left.created_at);
      return timestamp(getMaterialOpenedAt(right)) - timestamp(getMaterialOpenedAt(left));
    });
}

export function getContinueReadingMaterials(articles: Article[], limit = 4): MaterialArticle[] {
  return articles
    .map(asMaterialArticle)
    .filter((article) => getReadingStatus(article) === "in_progress")
    .sort((left, right) => timestamp(getMaterialOpenedAt(right)) - timestamp(getMaterialOpenedAt(left)))
    .slice(0, limit);
}

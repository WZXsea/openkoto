import { invoke } from "@tauri-apps/api/core";
import type { Article } from "./tauri";
import { previewMaterialImport, type PreviewMaterialImportRequest } from "../features/materials/api";

// 各类型可拖入导入的扩展名（与现有导入表单保持一致）
export const BOOK_EXTENSIONS = ["pdf", "epub"];
export const TEXT_EXTENSIONS = ["md", "markdown", "txt", "docx"];
export const VIDEO_EXTENSIONS = ["mp4", "mkv", "webm", "mov", "avi"];
export const AUDIO_EXTENSIONS = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "wma"];
export const SUBTITLE_EXTENSIONS = ["srt"];

const ALL_SUPPORTED = new Set([
  ...BOOK_EXTENSIONS,
  ...TEXT_EXTENSIONS,
  ...VIDEO_EXTENSIONS,
  ...AUDIO_EXTENSIONS,
  ...SUBTITLE_EXTENSIONS,
]);

export type DroppedMaterialImportResult =
  | { kind: "imported"; article: Article }
  | { kind: "conflict"; preview: Awaited<ReturnType<typeof previewMaterialImport>> };

export function getExtension(path: string): string {
  const name = path.split(/[/\\]/).pop() || path;
  const idx = name.lastIndexOf(".");
  return idx >= 0 ? name.slice(idx + 1).toLowerCase() : "";
}

export function getFileName(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

export function isSupportedDropPath(path: string): boolean {
  return ALL_SUPPORTED.has(getExtension(path));
}

async function importDroppedMaterial(
  command: string,
  sourceKind: PreviewMaterialImportRequest["sourceKind"],
  path: string,
  args: Record<string, unknown>,
): Promise<DroppedMaterialImportResult> {
  const preview = await previewMaterialImport({
    sourceKind,
    sourceUri: `file://${path}`,
    filePath: path,
  });
  if (preview.duplicates.length > 0) {
    return { kind: "conflict", preview };
  }
  const article = await invoke<Article>(command, {
    ...args,
    importJobId: preview.jobId,
    duplicatePolicy: "keep_copy",
  });
  return { kind: "imported", article };
}

/**
 * 按扩展名把拖入的文件路由到对应的导入命令，复用现有后端命令。
 * 返回创建的素材或明确的重复冲突。不支持的扩展名会抛错。
 */
export async function importDroppedPath(path: string): Promise<DroppedMaterialImportResult> {
  const ext = getExtension(path);

  if (BOOK_EXTENSIONS.includes(ext)) {
    return importDroppedMaterial("import_book_cmd", "book", path, { filePath: path, title: null });
  }
  if (TEXT_EXTENSIONS.includes(ext)) {
    return importDroppedMaterial("import_text_file_cmd", "text_file", path, { filePath: path, title: null });
  }
  if (VIDEO_EXTENSIONS.includes(ext) || AUDIO_EXTENSIONS.includes(ext)) {
    const sourceKind = AUDIO_EXTENSIONS.includes(ext) ? "audio" : "video";
    return importDroppedMaterial("import_local_video_cmd", sourceKind, path, { filePath: path });
  }
  if (SUBTITLE_EXTENSIONS.includes(ext)) {
    return importDroppedMaterial("import_srt_file_cmd", "subtitle", path, { filePath: path, title: null });
  }

  throw new Error(`unsupported:${getFileName(path)}`);
}

import { invoke } from "@tauri-apps/api/core";

const PLAYBACK_POSITION_KEY_PREFIX = "textlingo_video_position_";

export interface ResourceServerInfo {
  base_url: string;
  token: string;
}

let resourceServerInfoPromise: Promise<ResourceServerInfo> | null = null;

export function getResourceServerInfo(): Promise<ResourceServerInfo> {
  if (!resourceServerInfoPromise) {
    resourceServerInfoPromise = Promise.resolve(invoke<ResourceServerInfo>("get_resource_server_info_cmd"))
      .then((info) => {
        const baseUrl = info.base_url?.replace(/\/$/, "");
        if (!baseUrl || !info.token) {
          throw new Error("Invalid resource server info");
        }
        return {
          base_url: baseUrl,
          token: info.token,
        };
      })
      .catch((error) => {
        resourceServerInfoPromise = null;
        throw error;
      });
  }

  return resourceServerInfoPromise;
}

export function clearResourceServerInfoCacheForTests() {
  resourceServerInfoPromise = null;
}

export async function buildMediaResourceUrl(
  mediaPath: string | undefined,
  resourceType: "video" | "book" = "video",
): Promise<string> {
  if (!mediaPath) {
    return "";
  }

  if (mediaPath.startsWith("http://") || mediaPath.startsWith("https://")) {
    return mediaPath;
  }

  const filename = mediaPath.split(/[/\\]/).pop();
  if (!filename) {
    return mediaPath;
  }

  const info = await getResourceServerInfo();
  return `${info.base_url}/resource/${encodeURIComponent(info.token)}/${resourceType}/${encodeURIComponent(filename)}`;
}

export function buildPlaybackPositionKey(articleId: string | undefined, mediaUrl: string): string {
  return `${PLAYBACK_POSITION_KEY_PREFIX}${articleId || mediaUrl}`;
}

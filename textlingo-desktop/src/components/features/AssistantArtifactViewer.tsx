import { AlertTriangle, FileJson2, FileText, GitBranch, ScrollText } from "lucide-react";

import type { AssistantArtifact } from "../../features/assistant";
import { MarkdownContent } from "../ui/MarkdownContent";

interface AssistantArtifactViewerProps {
  artifact: AssistantArtifact;
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function prettyJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2) ?? String(value);
  } catch {
    return String(value);
  }
}

function markdownContent(value: unknown): string | null {
  if (typeof value === "string") return value;
  const content = asRecord(value);
  for (const key of ["markdown", "answer", "report", "text", "content", "reply"]) {
    if (typeof content[key] === "string") return content[key] as string;
  }
  return null;
}

interface MindMapNodeLike {
  title: string;
  summary?: string;
  children: MindMapNodeLike[];
}

function mindMapRoot(value: unknown): MindMapNodeLike | null {
  const content = asRecord(value);
  const map = asRecord(content.map ?? content);
  const rawRoot = asRecord(map.root);
  if (!Object.keys(rawRoot).length) return null;
  const normalize = (node: Record<string, unknown>): MindMapNodeLike => ({
    title: typeof node.title === "string" ? node.title : "未命名节点",
    summary: typeof node.summary === "string" ? node.summary : undefined,
    children: Array.isArray(node.children) ? node.children.map((child) => normalize(asRecord(child))) : [],
  });
  return normalize(rawRoot);
}

function MindMapTree({ node, depth = 0 }: { node: MindMapNodeLike; depth?: number }) {
  return (
    <li className={depth ? "ml-4 border-l border-border pl-3" : ""}>
      <div className="py-1.5">
        <p className="text-sm font-medium">{node.title}</p>
        {node.summary && <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{node.summary}</p>}
      </div>
      {node.children.length > 0 && <ul>{node.children.map((child, index) => <MindMapTree key={`${child.title}-${index}`} node={child} depth={depth + 1} />)}</ul>}
    </li>
  );
}

export function AssistantArtifactViewer({ artifact }: AssistantArtifactViewerProps) {
  const type = artifact.artifact_type;
  const isMissing = artifact.missing || artifact.file_available === false;
  const fileName = typeof artifact.metadata.file_name === "string"
    ? artifact.metadata.file_name
    : typeof artifact.metadata.path === "string" ? artifact.metadata.path : artifact.id;
  const mindMap = type === "mind_map" ? mindMapRoot(artifact.content) : null;
  const markdown = ["article_answer", "structured_report"].includes(type) ? markdownContent(artifact.content) : null;

  return (
    <article className="overflow-hidden rounded-2xl border border-border bg-card" data-testid={`assistant-artifact-${artifact.id}`}>
      <header className="flex flex-wrap items-center justify-between gap-2 border-b border-border bg-muted/25 px-4 py-3">
        <div className="flex min-w-0 items-center gap-2">
          {type === "mind_map" ? <GitBranch size={16} className="text-primary" /> : type === "article_answer" || type === "structured_report" ? <ScrollText size={16} className="text-primary" /> : type === "file" ? <FileText size={16} className="text-primary" /> : <FileJson2 size={16} className="text-primary" />}
          <div className="min-w-0"><h4 className="truncate text-sm font-medium">{type}</h4><p className="truncate text-[11px] text-muted-foreground">{artifact.id}</p></div>
        </div>
        <span className="rounded-full border border-border px-2 py-0.5 text-[11px] text-muted-foreground">版本 {artifact.version}</span>
      </header>

      <div className="p-4">
        {isMissing ? (
          <div className="flex gap-3 rounded-xl border border-amber-500/25 bg-amber-500/10 p-3 text-sm text-amber-700 dark:text-amber-300" role="alert">
            <AlertTriangle size={17} className="mt-0.5 shrink-0" /><div><p className="font-medium">产物文件不可用</p><p className="mt-1 text-xs opacity-85">{fileName} 可能已移动或删除，任务记录仍保留。</p></div>
          </div>
        ) : type === "mind_map" ? (
          mindMap ? <ul className="space-y-1"><MindMapTree node={mindMap} /></ul> : <pre className="max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-xl bg-muted/45 p-3 text-xs">{prettyJson(artifact.content)}</pre>
        ) : markdown ? (
          <div className="prose-sm max-w-none"><MarkdownContent content={markdown} /></div>
        ) : type === "file" ? (
          <div><p className="text-sm font-medium">{fileName}</p>{artifact.content == null ? <p className="mt-2 text-xs text-muted-foreground">该产物只保留文件元数据。</p> : <pre className="mt-3 max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-xl bg-muted/45 p-3 text-xs">{typeof artifact.content === "string" ? artifact.content : prettyJson(artifact.content)}</pre>}</div>
        ) : (
          <pre className="max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-xl bg-muted/45 p-3 text-xs">{prettyJson(artifact.content)}</pre>
        )}
      </div>
    </article>
  );
}

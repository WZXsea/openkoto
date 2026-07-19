import { ArrowRight, BookOpen, CalendarDays, FilePlus2, Library, Sparkles } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { homeActivityApi, type HomeActivityApi, type LearningActivityHeatmap } from "../../features/home";
import {
  asMaterialArticle,
  getContinueReadingMaterials,
  getMaterialOpenedAt,
  getMaterialProgress,
  getMaterialType,
  getReadingStatus,
} from "../../features/materials/selectors";
import type { MaterialArticle } from "../../features/materials/types";
import type { Article } from "../../types";
import { Button } from "../ui/button";

interface HomePageProps {
  articles: Article[];
  activityApi?: HomeActivityApi;
  onSelectArticle: (article: Article) => void;
  onNewMaterial: () => void;
  onOpenMaterials: () => void;
  onOpenLearning: () => void;
}

const TYPE_LABELS: Record<string, string> = {
  article: "文章",
  web: "网页",
  text: "文本",
  book: "书籍",
  video: "视频",
  audio: "音频",
};

function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function activityQuery() {
  const end = new Date();
  const start = new Date(end);
  start.setDate(start.getDate() - 83);
  return {
    start_date: localDateKey(start),
    end_date: localDateKey(end),
    timezone_offset_minutes: -end.getTimezoneOffset(),
  };
}

function progressLabel(article: MaterialArticle): string {
  const progress = getMaterialProgress(article);
  if (progress === null) return "尚未开始";
  return `${progress}%`;
}

function ActivityHeatmap({ heatmap }: { heatmap: LearningActivityHeatmap }) {
  const levelClass = (score: number) => {
    if (score === 0) return "bg-muted";
    if (score === 1) return "bg-primary/20";
    if (score <= 3) return "bg-primary/40";
    if (score <= 6) return "bg-primary/65";
    return "bg-primary";
  };

  return (
    <div>
      <div className="grid grid-flow-col grid-rows-7 gap-1" role="list" aria-label="近12周学习活跃度">
        {heatmap.days.map((day) => {
          const label = `${day.date}：阅读 ${day.read_materials} 份素材，学习整理 ${day.learning_actions} 次`;
          return (
            <span
              key={day.date}
              role="listitem"
              aria-label={label}
              title={label}
              className={`block aspect-square min-h-2 min-w-2 rounded-[3px] transition-transform hover:scale-125 ${levelClass(day.activity_score)}`}
            />
          );
        })}
      </div>
      <div className="mt-3 flex items-center justify-end gap-1 text-[11px] text-muted-foreground">
        <span>少</span>
        {[0, 1, 2, 4, 7].map((score) => <span key={score} className={`h-2.5 w-2.5 rounded-[3px] ${levelClass(score)}`} />)}
        <span>多</span>
      </div>
    </div>
  );
}

function MaterialSummaryCard({ article, onOpen }: { article: MaterialArticle; onOpen: () => void }) {
  const progress = getMaterialProgress(article);
  return (
    <button
      type="button"
      onClick={onOpen}
      className="group flex min-w-0 flex-col rounded-2xl border border-border/70 bg-card/70 p-4 text-left transition-colors hover:border-primary/35 hover:bg-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span className="mb-5 flex items-center justify-between gap-3 text-xs text-muted-foreground">
        <span>{TYPE_LABELS[getMaterialType(article)] || "素材"}</span>
        <ArrowRight size={15} className="transition-transform group-hover:translate-x-0.5" />
      </span>
      <span className="line-clamp-2 min-h-10 text-sm font-medium leading-5">{article.title || "未命名素材"}</span>
      <span className="mt-4 flex items-center gap-2 text-xs text-muted-foreground">
        <span className="h-1 flex-1 overflow-hidden rounded-full bg-muted">
          <span className="block h-full rounded-full bg-primary" style={{ width: `${progress ?? 0}%` }} />
        </span>
        {progressLabel(article)}
      </span>
    </button>
  );
}

export function HomePage({
  articles,
  activityApi = homeActivityApi,
  onSelectArticle,
  onNewMaterial,
  onOpenMaterials,
  onOpenLearning,
}: HomePageProps) {
  const [heatmap, setHeatmap] = useState<LearningActivityHeatmap | null>(null);
  const [activityError, setActivityError] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setActivityError(false);
    void activityApi.getActivityHeatmap(activityQuery())
      .then((result) => { if (!cancelled) setHeatmap(result); })
      .catch(() => { if (!cancelled) setActivityError(true); });
    return () => { cancelled = true; };
  }, [activityApi]);

  const activeMaterials = useMemo(() => articles
    .map(asMaterialArticle)
    .filter((article) => getReadingStatus(article) !== "archived")
    .sort((left, right) => Date.parse(getMaterialOpenedAt(right)) - Date.parse(getMaterialOpenedAt(left))), [articles]);
  const primary = useMemo(() => getContinueReadingMaterials(articles, 1)[0] ?? activeMaterials[0] ?? null, [activeMaterials, articles]);
  const recent = useMemo(() => activeMaterials.filter((article) => article.id !== primary?.id).slice(0, 4), [activeMaterials, primary?.id]);
  const today = heatmap?.days.find((day) => day.date === localDateKey(new Date())) ?? null;

  return (
    <div className="h-full overflow-y-auto bg-gradient-to-br from-background via-background to-muted/30">
      <div className="mx-auto grid min-h-full max-w-[1500px] gap-8 px-5 py-8 lg:grid-cols-[minmax(0,1fr)_300px] lg:px-10 lg:py-10 xl:gap-12">
        <section className="min-w-0">
          <header className="mb-8">
            <p className="mb-2 text-sm text-muted-foreground">开始今天的阅读</p>
            <h1 className="text-3xl font-semibold tracking-tight">你好，今天想读什么？</h1>
          </header>

          {primary ? (
            <section className="relative overflow-hidden rounded-3xl border border-primary/15 bg-card p-6 shadow-sm sm:p-8" aria-labelledby="home-primary-title">
              <div className="absolute -right-14 -top-20 h-48 w-48 rounded-full bg-primary/10 blur-3xl" />
              <div className="relative">
                <div className="mb-8 flex items-center gap-2 text-sm text-primary">
                  <BookOpen size={17} />
                  {getReadingStatus(primary) === "in_progress" ? "继续阅读" : "最近素材"}
                </div>
                <h2 id="home-primary-title" className="max-w-3xl break-words text-2xl font-semibold leading-snug sm:text-3xl">{primary.title || "未命名素材"}</h2>
                <p className="mt-3 break-all text-sm text-muted-foreground">
                  {TYPE_LABELS[getMaterialType(primary)] || "素材"}
                  {primary.source_name || primary.source_url ? ` · ${primary.source_name || primary.source_url}` : ""}
                </p>
                <div className="mt-8 flex flex-wrap items-center gap-4">
                  <Button onClick={() => onSelectArticle(primary)} className="gap-2 rounded-full px-5">
                    <BookOpen size={16} />{getReadingStatus(primary) === "in_progress" ? "继续阅读" : "开始阅读"}
                  </Button>
                  <span className="flex min-w-48 items-center gap-3 text-sm text-muted-foreground">
                    <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                      <span className="block h-full rounded-full bg-primary" style={{ width: `${getMaterialProgress(primary) ?? 0}%` }} />
                    </span>
                    {progressLabel(primary)}
                  </span>
                </div>
              </div>
            </section>
          ) : (
            <section className="rounded-3xl border border-dashed border-border bg-card/55 px-6 py-14 text-center" aria-labelledby="empty-library-title">
              <FilePlus2 className="mx-auto mb-4 text-primary" size={30} />
              <h2 id="empty-library-title" className="text-xl font-semibold">建立你的阅读素材库</h2>
              <p className="mx-auto mt-2 max-w-md text-sm leading-6 text-muted-foreground">导入文章、网页、书籍或本地文件，开始阅读并保留学习语境。</p>
              <Button onClick={onNewMaterial} className="mt-6 gap-2 rounded-full px-5"><FilePlus2 size={16} />导入第一份素材</Button>
            </section>
          )}

          <section className="mt-10" aria-labelledby="recent-materials-title">
            <div className="mb-4 flex items-center justify-between gap-4">
              <div>
                <h2 id="recent-materials-title" className="text-lg font-semibold">最近素材</h2>
                <p className="mt-1 text-sm text-muted-foreground">从上次停留的地方继续</p>
              </div>
              <Button variant="ghost" size="sm" onClick={onOpenMaterials} className="gap-1.5">查看素材库<ArrowRight size={15} /></Button>
            </div>
            {recent.length > 0 ? (
              <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                {recent.map((article) => <MaterialSummaryCard key={article.id} article={article} onOpen={() => onSelectArticle(article)} />)}
              </div>
            ) : (
              <div className="flex items-center justify-between gap-4 rounded-2xl border border-border/70 bg-card/50 px-5 py-4 text-sm text-muted-foreground">
                <span>{primary ? "还没有更多素材" : "导入后，最近阅读的素材会出现在这里"}</span>
                <Button variant="outline" size="sm" onClick={onNewMaterial} className="shrink-0 gap-1.5"><FilePlus2 size={15} />导入素材</Button>
              </div>
            )}
          </section>
        </section>

        <aside className="space-y-5 lg:pt-[5.6rem]" aria-label="学习活动">
          <section className="rounded-2xl border border-border/70 bg-card/70 p-5">
            <div className="mb-5 flex items-center justify-between gap-3">
              <div>
                <h2 className="font-semibold">学习活跃度</h2>
                <p className="mt-1 text-xs text-muted-foreground">近12周</p>
              </div>
              <CalendarDays size={18} className="text-muted-foreground" />
            </div>
            {heatmap ? <ActivityHeatmap heatmap={heatmap} /> : activityError ? (
              <p className="rounded-xl bg-muted/60 px-3 py-4 text-sm text-muted-foreground" role="status">活动摘要暂时无法读取，不影响阅读。</p>
            ) : (
              <div className="grid grid-flow-col grid-rows-7 gap-1" aria-label="正在加载学习活跃度">
                {Array.from({ length: 84 }, (_, index) => <span key={index} className="aspect-square min-h-2 min-w-2 animate-pulse rounded-[3px] bg-muted" />)}
              </div>
            )}
          </section>

          <section className="rounded-2xl border border-border/70 bg-card/70 p-5">
            <div className="mb-4 flex items-center gap-2"><Sparkles size={17} className="text-primary" /><h2 className="font-semibold">今日摘要</h2></div>
            <dl className="grid grid-cols-2 gap-3">
              <div className="rounded-xl bg-muted/55 p-3"><dt className="text-xs text-muted-foreground">阅读素材</dt><dd className="mt-1 text-2xl font-semibold tabular-nums">{today?.read_materials ?? 0}</dd></div>
              <div className="rounded-xl bg-muted/55 p-3"><dt className="text-xs text-muted-foreground">学习整理</dt><dd className="mt-1 text-2xl font-semibold tabular-nums">{today?.learning_actions ?? 0}</dd></div>
            </dl>
            <Button variant="outline" className="mt-4 w-full gap-2" onClick={onOpenLearning}>进入学习工作台<ArrowRight size={15} /></Button>
          </section>

          <button type="button" onClick={onOpenMaterials} className="flex w-full items-center gap-3 rounded-2xl border border-border/70 bg-card/45 p-4 text-left text-sm transition-colors hover:bg-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
            <Library size={18} className="text-primary" /><span className="flex-1"><span className="block font-medium text-foreground">素材库</span><span className="mt-0.5 block text-xs text-muted-foreground">搜索、筛选与管理全部素材</span></span><ArrowRight size={15} />
          </button>
        </aside>
      </div>
    </div>
  );
}

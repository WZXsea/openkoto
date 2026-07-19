# PR-9 批注与来源定位架构
## 目标
PR-9 在 Backend-first 架构中加入可持久化批注，并让 Article、TXT、PDF、EPUB、Media 共用版本化来源定位契约。批注保存、编辑、检索、删除和候选转换由 PostgreSQL Backend 负责；Desktop 只保留会话和页面状态。
## 组件关系
```text
Reader selection
  -> ReaderAnnotationDraft
  -> AppRoutes
  -> Tauri annotation commands
  -> Axum /annotations API
  -> PostgreSQL annotations

AnnotationWorkbench
  -> filter/edit/delete/convert
  -> source navigation request
  -> AppStore activeAnnotation
  -> reader resolver
  -> exact | degraded | unresolved
```
## 数据边界
| 层 | 责任 | 不承担 |
|---|---|---|
| Reader adapters | 生成 locator、解析回跳、显示定位状态 | 数据持久化、用户隔离 |
| AppRoutes/AppStore | 创建请求、页面跳转、当前目标批注 | 业务约束、幂等裁决 |
| Tauri bridge | 认证会话下转发 Backend API | 本地 JSON 双写 |
| Backend | CRUD、筛选、校验、幂等、事务转换 | 阅读器 DOM 定位 |
| PostgreSQL | 用户隔离关系、索引、删除策略 | UI 临时状态 |
## SourceLocator v1
共享字段包括 `version`、`reader_kind`、`material_revision`、`content_sha256` 和 `quote`。定位锚点按阅读器分为：
| Reader | 主要锚点 | 降级路径 |
|---|---|---|
| Article/TXT | segment + text range + quote | quote -> 邻近段落 -> 段落位置 |
| PDF | page + text range + quote | quote -> page |
| EPUB | CFI + quote | CFI -> quote 扩展点 |
| Media | time range + subtitle segment | subtitle -> 时间点 |
`material_revision` 或 `content_sha256` 不一致时不得静默报告精确命中。解析器必须返回 `exact`、`degraded` 或 `unresolved`，由 Reader 向用户显示。
## 幂等与事务
1. 创建使用用户范围内唯一的 `client_request_id`，相同请求返回原对象，不同负载复用同一键返回冲突。
2. 批注转学习候选在单个 PostgreSQL 事务内创建 `learning_item` 并回写 `annotation.learning_item_id`。
3. 重复转换返回已关联候选，不产生静默重复。
4. 材料删除级联删除批注；段落删除仅清空 `segment_id`，保留 locator 与原文快照用于降级恢复。
## UI 行为
1. 顶部“批注”入口打开工作台。
2. 工作台支持素材、类型、标签、关键词和时间范围筛选。
3. 阅读器选择文本后创建 `highlight`，保存原文、quote、locator、revision/hash 和默认颜色。
4. 普通打开素材时加载最近批注；从工作台回跳时优先加载指定批注。
5. 批注可编辑颜色、标签和笔记，可删除、转候选并回到来源。
## 验证边界
Backend 集成测试覆盖 migration 重放、CRUD、过滤、多用户隔离、并发幂等、事务转换和删除策略。前端测试覆盖 locator 解析、五类 Reader 适配、工作台操作和应用路由；Playwright 覆盖工作台编辑、转换与来源回跳。

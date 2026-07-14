# PR-10 学习对象收敛与本地复盘

## 目标
PR-10 将 `learning_items` 收敛为学习内容、审核状态、来源和质量标记的唯一规范对象，并在不连接 Anki、FSRS 或其他外部软件的前提下，提供候选审核、批量整理、兼容迁移、活动记录和本地复盘。

## 领域所有权
| 对象 | 职责 | 兼容边界 |
|---|---|---|
| `learning_items` | 学习内容、状态、原文证据、质量标记的唯一事实源 | 所有新候选和接受流程均写入该表 |
| `word_pack_learning_items` | 词包到 accepted learning item 的规范关联 | 旧 `favorite_vocabulary_packs` 在兼容期保留 |
| `favorite_vocabularies`、`favorite_grammars` | 旧界面、导入导出和 API 的兼容投影 | 不再作为新学习对象的唯一来源 |
| `learning_activity_events` | 阅读、创建、接受、整理和本地预习的不可变历史 | 回顾统计从事件生成，不从当前状态反推 |
| `annotations.learning_item_id` | 批注到规范学习项的来源关联 | 保留 PR-9 的幂等转换关系 |

外部同步状态不写入本地学习状态。第一阶段不实现 due、interval、retention 或其他 FSRS 调度字段。

## 状态机
| 当前状态 | 合法目标或操作 |
|---|---|
| `candidate` | 接受、拒绝、归档、编辑、合并 |
| `accepted` | 幂等接受、归档、整理词包 |
| `rejected` | 恢复为 candidate、归档 |
| `archived` | 按 `status_before_archive` 恢复；缺失时恢复为 candidate |
| merged loser | 保持 archived，并以 `merged_into_id` 指向保留项 |

通用 PATCH 和旧批量状态接口不得旁路进入 accepted。接受始终通过原子接受事务完成：锁定学习项、校验状态和类型、写入兼容 favorite、建立词包关联、更新状态并记录事件。任一步失败时全部回滚。

accepted 与 archived-accepted 的内容编辑会在同一事务同步兼容 favorite；已接纳项、merged loser 和存在 merged loser 的目标项禁止硬删除。删除旧 favorite 时，规范学习项原子转为 archived 并保留来源和词包证据。merged loser 不允许通过通用恢复接口解除合并。

## 数据迁移
Migration `20260714001100_learning_domain_activity.sql` 执行以下结构变更：

1. 增加 `quality_flags`、`status_before_archive`、`merged_into_id` 和来源快照字段。
2. 将素材和段落外键删除策略改为 `SET NULL`，删除来源素材后仍保留学习项和原句快照。
3. 新增 `word_pack_learning_items` 规范关联。
4. 新增 `learning_activity_events`、幂等键、payload hash 和查询索引。

旧收藏和词包数据通过显式兼容迁移接口处理。`dry_run=true` 只返回计划、已迁移数量和冲突，不写数据；commit 只补充缺失的规范对象和关联，不删除旧数据，重复执行保持幂等。

## Backend API
| 方法与路径 | 作用 |
|---|---|
| `GET /learning-items` | 保持数组响应；新增状态、类型、素材、来源类型、来源文本、标签和质量标记筛选 |
| `POST /learning-items/bulk-organize` | 逐项事务执行接受、拒绝、归档、恢复、标签、质量标记和合并；返回逐项成功或错误 |
| `POST /learning-items/compatibility-migration` | 旧收藏和词包的 dry-run 或幂等迁移 |
| `POST /learning-items/{id}/local-preview` | 记录本地预习事件，不修改记忆调度字段 |
| `GET /learning-activity-events` | 按事件、学习项、素材和日期分页查询 |
| `GET /learning-review/daily` | 按调用端时区汇总当日全量事件 |
| `GET /materials/{id}/learning-review` | 汇总指定素材的全量事件 |

现有 `/learning-items`、`/learning-items/from-selection`、`/learning-items/bulk-status`、`/learning-items/{id}/accept`、favorite、word-pack 和 annotation API 保持兼容。

## 活动事件
事件类型包括 `read`、`create`、`accept`、`reject`、`archive`、`restore`、`organize`、`local_preview`、`merge` 和 `migrate`。

1. 领域状态变化与事件在同一数据库事务提交。
2. 相同 idempotency key 与相同 payload 重放返回既有事件。
3. 相同 key 与不同 payload 返回冲突。
4. 阅读进度按日期、阅读器、状态和进度桶去重，避免滚动保存产生事件爆炸。
5. 摘要使用 SQL 对完整时间范围聚合；事件明细单独限制最多 500 条。
6. 当日边界由 `timezone_offset_minutes` 转换为 UTC 范围，默认使用北京时间偏移。

## Desktop 与前端
Tauri 保留原命令名并新增以下桥接命令：

- `bulk_organize_learning_items_cmd`
- `migrate_legacy_learning_items_cmd`
- `list_learning_activity_events_cmd`
- `get_daily_learning_review_cmd`
- `get_material_learning_review_cmd`
- `record_local_preview_cmd`

主导航新增“学习工作台”。工作台支持：

1. 按状态、类型、来源素材、来源类型、标签和质量标记筛选。
2. 编辑学习内容、语境含义、定义、标签和“待人工核验”。
3. 批量接受、拒绝、归档、恢复、设置标签和质量标记；失败项保留选中以便重试。
4. 展示原句、上下文和来源，并返回原素材。
5. 展示今日或所选素材的活动汇总，记录 accepted item 的本地预习。
6. 检查并执行旧收藏/词包兼容迁移。

## 兼容与恢复边界
1. favorite 和旧词包表在第一阶段继续保留，避免旧 UI、导入导出和历史数据失效。
   linked favorite 的列表只暴露 canonical 状态为 accepted 的记录，避免 archived item 进入旧复习队列。
2. 删除素材只解除来源外键；学习项、原句快照、接受投影和词包关联继续保留。
3. 合并保留 loser 学习项和 `merged_into_id`，不得删除批注或来源证据。
4. 旧本地预习和 SRS 兼容命令继续存在，但 PR-10 主流程只使用本地活动事件，不模拟 FSRS。
5. 发行版仍是未做 Developer ID 签名和公证的个人预发布构建。

## 验证入口
统一验证脚本为 `bash script/verify_pr10_learning_domain.sh`。验证范围包括 migration 静态护栏、Backend 编译与测试、Desktop Rust 编译与测试、前端 typecheck/Vitest/生产构建、Playwright 候选流程、版本一致性和 PR-9 回归。

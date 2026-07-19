## PR-14 结构化正文编辑与版本管理

## 目标
PR-14 将正文编辑从整篇文本覆盖改为受约束块编辑和 Backend 事务提交。正文结构、句级学习单元、批注定位、学习项来源和派生翻译必须在一次编辑后保持可解释的一致状态。

目标预发布版本为 `0.12.0`。PR-13 的 `0.11.0` 保留为工程候选基线，M2 外部集成在 PR-14 验收和正式发行门禁完成前不进入。

`0.12.1` 作为 PR-14.1 交互修复版，仅收敛结构化块内部拖动与全局文件导入拖放的事件边界，不改变文档数据模型、版本契约或第一阶段边界。

## 数据边界
1. `material_blocks` 是编辑结构，`material_segments` 是句级翻译与学习结构；两者通过稳定 UUID 和块内顺序关联。
2. `materials.content` 是事务生成的兼容投影，不能与 blocks、segments 分别写入。
3. 正文修改递增 `current_revision`；标题、标签、翻译、读音、精讲和产物关联不递增正文版本。
4. 翻译、读音和精讲保存各自的来源哈希；文本变化后保留旧值并标记 stale，不自动调用模型。
5. 删除采用软删除并记录 segment lineage，批注和学习项继续保留原来源快照。
6. PDF、EPUB 原件不可变；可编辑稿是独立材料，通过 `material_relations` 关联。

## 接口
| 接口 | 责任 |
|---|---|
| `GET /materials/{id}/document` | 当前 revision、hash、blocks、segments 和派生状态 |
| `POST /materials/{id}/document/preview` | 无写入计算结构、派生、批注和学习项影响 |
| `PUT /materials/{id}/document` | 使用 `base_revision` 和 `client_request_id` 事务提交 |
| `GET/PUT/DELETE /materials/{id}/document/draft` | 恢复草稿 |
| `GET /materials/{id}/revisions` | 版本摘要 |
| `GET /materials/{id}/revisions/{revision}` | 版本快照和变更摘要 |
| `POST /materials/{id}/revisions/{revision}/restore` | 以旧快照创建新 revision |
| `PATCH /materials/{id}/segments/derived` | 原地更新翻译、读音和精讲 |
| `POST /materials/{id}/editable-derivative` | 创建或返回原件的关联编辑稿 |

相同 `base_revision` 的并发提交只允许一个成功；冲突返回 `409 material_revision_conflict`。相同幂等键携带不同载荷返回 `409 document_edit_idempotency_conflict`。

## 编辑交互
1. Reader 显式进入编辑态，自动收起 Assistant，退出后恢复原状态。
2. 首版支持段落、H2/H3、一级有序/无序列表、引用、分隔线，以及粗体、斜体、行内代码和链接。
3. `Enter` 拆分、块首 `Backspace` 合并、`Shift+Enter` 软换行、`Alt+↑/↓` 移动；空块行首 `/` 和 `Cmd/Ctrl+K` 提供命令入口。
4. 编辑过程只写恢复草稿；显式保存先预览影响，再创建正式 revision。
5. 返回、切换材料和关闭窗口统一经过脏状态保护；取消必须恢复进入编辑时快照。
6. 保存失败保留草稿；409 冲突禁止静默覆盖，并提供重新载入当前版本。

## 兼容与迁移
1. migration 根据 `is_new_paragraph` 将现有 segments 组合为 blocks，保留原 segment UUID。
2. content 与 segments 不一致时记录 `legacy_content_mismatch`；只有规范化文本一致的句子继承派生结果。
3. 旧在线整篇 PATCH 仅保留媒体字幕维护；文本导入替换已转为 document preview/commit，翻译、精讲、读音和产物关联使用稀疏专用接口。
4. 媒体字幕结构仍保留受限兼容入口，不允许文本材料调用全量 segment 替换。

## 验证门槛
1. 空库、升级库和重复 migration 通过。
2. 未修改句子的 UUID、翻译、精讲、批注和学习项来源保持；修改和删除路径返回明确状态。
3. preview、commit、draft、revision、restore、derived 和 derivative 接口覆盖用户隔离、幂等和冲突测试。
4. 前端覆盖块操作、IME、撤销重做、自动草稿、取消、影响确认、版本恢复和窄窗交互。
5. Backend、Desktop Rust、Frontend、Playwright、历史回归、packaged smoke 和 0.11.0→0.12.0 实机升级全部通过。

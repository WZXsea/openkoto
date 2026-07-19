## PR-15 Pi Agent Runtime 施工计划

### 状态

| 项目 | 内容 |
|---|---|
| 阶段 | PR-15 |
| 分支 | `wzx/pr15-pi-agent-runtime` |
| 基线 | `wzx/assistant-runtime-hotfix` / OpenKoto Desktop 0.12.7 |
| 官方依赖 | `earendil-works/pi` |
| 目标内核 | `@earendil-works/pi-agent-core@0.80.10` |
| Provider 层 | `@earendil-works/pi-ai@0.80.10` |
| GitHub PR | [`WZXsea/openkoto#1`](https://github.com/WZXsea/openkoto/pull/1) |
| 执行状态 | 已提交，待合并 |

详细边界见 [`docs/architecture/pr15-pi-agent-runtime.md`](../architecture/pr15-pi-agent-runtime.md)。

### 任务 1：锁定依赖和运行环境

涉及文件：

- `textlingo-desktop/agent-worker/package.json`
- `textlingo-desktop/agent-worker/package-lock.json`
- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `.github/workflows/release-dev.yml`

施工步骤：

1. 删除 `@opencode-ai/sdk`。
2. 精确安装 `@earendil-works/pi-agent-core@0.80.10`。
3. 精确安装 `@earendil-works/pi-ai@0.80.10`。
4. 声明 Worker Node engine 为 `>=22.19.0`。
5. 将 CI、开发发布和正式发布的 Node 版本统一到 `22.19.0`。
6. 检查 lockfile 中旧 SDK、生命周期脚本和版本漂移。

验收命令：

```bash
npm --prefix textlingo-desktop/agent-worker ci
npm --prefix textlingo-desktop/agent-worker run typecheck
```

### 任务 2：实现 Pi Runtime Adapter

新增文件：

- `textlingo-desktop/agent-worker/src/piRuntime.ts`
- `textlingo-desktop/agent-worker/src/piRuntime.test.ts`

施工步骤：

1. 建立请求级 `Models + Model + Agent`。
2. 映射 OpenAI-compatible、Google 和 Anthropic。
3. 自定义 OpenAI-compatible 模型使用动态 Model，允许任意模型 ID。
4. 显式使用 `models.streamSimple.bind(models)`。
5. 通过 `getApiKey` 传递请求级密钥。
6. 固定 `tools: []` 和 `thinkingLevel: "off"`。
7. 订阅 Pi 生命周期事件并向任务编排器提供事件回调。
8. 提取最终 Assistant 文本。
9. 检查 `stopReason`、空消息和空文本。
10. 实现 API key/Bearer token 脱敏。
11. 接入外部 AbortSignal、`agent.abort()` 和 `waitForIdle()`。
12. 实现 300 秒默认超时及环境变量配置。

单元测试：

- Provider 构造与 base URL 归一化。
- `tools: []`。
- 请求级 API key。
- 流式事件。
- error、aborted、length。
- 用户取消。
- 超时。
- 并发任务凭据隔离。
- 空消息和空文本。

### 任务 3：迁移思维导图任务

涉及文件：

- `textlingo-desktop/agent-worker/src/mindMapTask.ts`
- `textlingo-desktop/agent-worker/src/mindMapTask.test.ts`

施工步骤：

1. 删除 OpenCode server、端口分配、session 和 SDK 类型。
2. 删除手写 `/chat/completions` fetch。
3. 将 Prompt Runner 类型切换为 `PiAgentPromptRequest`。
4. 保留临时任务工作区、Prompt、Schema 和结果归一化。
5. 将首次 Pi 文本事件映射为 streaming 进度。
6. 保存 Artifact 前再次检查取消信号。
7. 验证取消后不写入部分 Artifact。

### 任务 4：迁移 Assistant 任务

涉及文件：

- `textlingo-desktop/agent-worker/src/assistantTask.ts`
- `textlingo-desktop/agent-worker/src/assistantTask.test.ts`

施工步骤：

1. 接入同一 Pi Runtime Adapter。
2. 保留 `{reply, action}` JSON 契约。
3. 保留现有三个动作类型和未知动作降级。
4. 保留 Tauri allowlist 和审计执行链。
5. 将首次文本事件映射为 streaming 进度。
6. 不向 Pi 注册应用工具。

### 任务 5：切换 Worker Host 和 Tauri 监管

涉及文件：

- `textlingo-desktop/agent-worker/src/index.ts`
- `textlingo-desktop/agent-worker/src/index.test.ts`
- `textlingo-desktop/src-tauri/src/agent_worker.rs`
- `textlingo-desktop/src-tauri/tests/agent_worker_test.rs`

施工步骤：

1. Worker 主入口注入 `runPiAgentPrompt`。
2. ready 事件固定为 `runtime: "pi-agent-core"`。
3. 保留每任务独立 AbortController 和重复 task ID 拒绝。
4. Tauri 必需输出清单增加 `piRuntime.js`。
5. Tauri 收到非 Pi runtime 的 ready 事件时拒绝进入 healthy。
6. 保持 JSONL 请求、结果、错误和 heartbeat 协议不变。

### 任务 6：更新构建与发布门禁

涉及文件：

- `script/verify_pr6_packaged_local_stack.sh`
- `script/verify_pr7_material_workbench.sh`
- `script/verify_pr12_assistant_observability.sh`
- Release 工作流中的安装包检查步骤

门禁要求：

1. Pi 两个依赖存在且版本精确。
2. `@opencode-ai/sdk`、`createOpencode`、`runOpenCodePrompt` 不得残留。
3. Worker 生产代码不得保留手写 Provider fetch。
4. `piRuntime.js` 必须进入 dist 和安装包。
5. Bundled Node 版本满足 `>=22.19.0`。
6. Bundled Node 能导入两个 Pi 包。
7. Worker ready runtime 为 `pi-agent-core`。
8. 取消测试产生 `task_cancelled`。
9. 生产 Worker 不包含 mock 后门。

### 任务 7：验证矩阵

| 层级 | 验证 |
|---|---|
| Worker | typecheck、45 项以上单元测试、build |
| Provider | Faux Provider 确定性测试、本地 mock HTTP、当前 OpenAI-compatible 配置 |
| 生命周期 | 正常完成、发送前取消、流式取消、超时、Provider 错误、截断 |
| 任务 | mind_map applicable/partial/not_applicable；Assistant reply/action |
| Rust | Worker event、runtime 标识、bundle stale、checkpoint、cancel |
| 静态门禁 | PR-6、PR-7、PR-12 |
| 发布 | Node 版本、Pi 模块解析、dist 输出、签名步骤 |
| 回归 | 前端 typecheck/test、Backend test、Tauri test |

### 任务 8：提交和 Pull Request

1. 确认 `git diff` 仅包含 PR-15 范围文件。
2. 排除用户工作区中的无关改动。
3. 提交信息使用 `feat(agent): migrate worker runtime to Pi`。
4. 推送 `wzx/pr15-pi-agent-runtime`。
5. 在 `WZXsea/openkoto` 创建 Pull Request。
6. PR base 使用 `wzx/assistant-runtime-hotfix`。
7. PR 描述包含范围、风险、验证结果、包体变化和回退方法。

### 完成定义

- [x] 架构计划与施工计划已写入。
- [x] Pi Runtime 生产路径完成。
- [x] 旧 OpenCode/手写 Provider 生产路径删除。
- [x] 取消、超时、错误和流式进度通过测试。
- [x] Worker、Rust、静态门禁和发布工作流通过。
- [x] 无真实文章或 API key 出现在测试和日志。
- [x] 无关工作区改动未进入提交。
- [x] 分支已推送并创建 PR。

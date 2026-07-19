## PR-15 Pi Agent Runtime

### 阶段目标

PR-15 将 OpenKoto Assistant Worker 的模型执行内核迁移到 Pi 官方 SDK，统一 Provider、流式事件、取消和错误终态。官方来源固定为：

- 仓库：[`earendil-works/pi`](https://github.com/earendil-works/pi)
- Agent 内核：`@earendil-works/pi-agent-core@0.80.10`
- 模型适配层：`@earendil-works/pi-ai@0.80.10`
- 运行环境：Node.js `>=22.19.0`

本阶段继续保留 OpenKoto 已有的任务事实源和安全边界：

1. Tauri 负责 Worker 进程监管、取消、检查点和事件转发。
2. Backend 负责任务、时间线和 Artifact 持久化。
3. Worker 继续使用 `agent.run`、`agent.cancel` 和既有 JSONL 事件协议。
4. 思维导图继续使用现有 JSON Schema、Zod 校验和 `mind_map` Artifact。
5. Assistant 继续返回 `{reply, action}`，由 Tauri 注册表校验和审计动作。

### 现状与迁移动因

0.12.7 的生产路径已经绕开 OpenCode CLI，直接调用 OpenAI-compatible `/chat/completions`。该修复解决了安装包缺少 `opencode` 可执行文件导致的 `spawn opencode ENOENT`，仍存在以下结构性问题：

| 问题 | 当前影响 | PR-15 处理 |
|---|---|---|
| Worker 保留 `@opencode-ai/sdk` 和 OpenCode server 代码 | 依赖和运行语义混杂 | 完整删除旧 SDK 与启动逻辑 |
| 手写 Provider 请求只覆盖 OpenAI-compatible | Google、Anthropic 无法使用统一运行路径 | 由 Pi Provider 层接管 |
| 固定 120 秒超时 | 长文章容易被统一截断 | 改为 300 秒默认值并支持运行时配置 |
| 请求结束与任务成功语义耦合 | Pi/Provider 错误可能以终态消息返回 | 显式检查 `stopReason` |
| 缺少统一流式事件 | 进度只能按固定阶段推进 | 将首次文本事件映射为 streaming 阶段 |
| SDK 与外部 CLI 边界不清 | 打包后容易出现系统命令依赖 | SDK 同进程嵌入，不启动外部 Pi/OpenCode CLI |

### 目标架构

```text
Frontend
  └─ Tauri Agent Worker Supervisor
      ├─ task/checkpoint/retry/cancel
      └─ JSONL agent.run / agent.cancel
          └─ Node Agent Worker
              ├─ mind_map.generate
              ├─ assistant.agent_turn
              └─ Pi Runtime Adapter
                  ├─ @earendil-works/pi-agent-core
                  ├─ @earendil-works/pi-ai
                  ├─ request-scoped provider/model/API key
                  ├─ tools: []
                  └─ stopReason / timeout / abort mapping
```

Pi Agent 仅在单个任务生命周期内存在。每个任务创建独立的 `Models`、`Model` 和 `Agent`，任务结束后释放内存状态。Pi 会话不写入磁盘，Backend 仍是任务与产物的唯一持久化事实源。

### Provider 映射

| OpenKoto Provider | Pi API | 配置要求 |
|---|---|---|
| `openai_compatible` | `openai-completions` | 保留任意 `baseUrl`、模型 ID 和可选 API key |
| `native_google` | `google-generative-ai` | 请求级传入 Google API key |
| `native_anthropic` | `anthropic-messages` | 请求级传入 Anthropic API key |
| `unsupported` | 不分发 | 延续 `provider_unsupported` 终态 |

自定义 OpenAI-compatible 模型允许不在 Pi 静态目录中。适配器按请求动态构造 Model，避免模型目录更新成为任务阻断条件。

### 生命周期与错误语义

#### 正常任务

1. Worker 发出 `task.started`。
2. 任务编排器发出 planning、starting_agent、analyzing/thinking。
3. Pi `message_update` 首次出现文本时发出 streaming。
4. 完整响应通过现有 JSON 解析和 Schema 校验。
5. 取消状态再次检查后才允许写入 Artifact。
6. Worker 发出唯一 `task.result`。

#### 取消

1. Tauri 发送 `agent.cancel`。
2. Worker 中对应 `AbortController` 中止。
3. Pi Adapter 调用 `agent.abort()`，并等待 `agent.waitForIdle()`。
4. 任务映射为 `task.error`，错误码为 `task_cancelled`。
5. 已收到的部分文本不得保存为 Artifact。

#### 超时和 Provider 错误

- 默认绝对超时为 300 秒。
- 可通过 `TEXTLINGO_PI_AGENT_TIMEOUT_MS` 配置正整数毫秒值。
- 超时调用 `agent.abort()`，最终任务失败。
- `stopReason=error` 必须读取并脱敏 `errorMessage`。
- `stopReason=aborted` 在用户取消时映射为取消；Provider 自发中止映射为 Provider 错误。
- `stopReason=length`、`toolUse` 等非完整终态不得作为成功结果。
- API key、Bearer token、完整 Prompt 和文章正文不得进入日志。

### 权限与安全边界

Pi 官方说明其本身不提供文件、进程或网络沙箱。PR-15 采用以下约束：

1. 仅安装 `pi-agent-core` 和 `pi-ai`，不安装 `pi-coding-agent`。
2. Agent 固定使用 `tools: []`。
3. 不加载 Shell、文件读写、扩展、技能、MCP 或 Pi SessionManager。
4. Assistant 动作继续由 OpenKoto Tauri allowlist 执行。
5. API key 通过请求级回调传入，不从 Pi 全局凭据目录读取。
6. 每个任务使用独立 Agent，禁止不同任务共享消息、模型和凭据。
7. 生产 Worker 不提供 mock 环境变量或测试后门。

### 打包和供应链

Pi `0.80.10` 要求 Node.js `>=22.19.0`。PR-15 同步升级 CI、开发发布和正式发布工作流，并增加以下门禁：

1. Worker 依赖精确锁定到 `0.80.10`，不使用范围版本。
2. `package-lock.json` 不得包含 `@opencode-ai/sdk`。
3. 安装包必须包含 `dist/piRuntime.js`。
4. Bundled Node 必须能够导入两个 Pi 包。
5. Worker 启动事件必须声明 `runtime: "pi-agent-core"`。
6. Tauri 监管层拒绝旧 runtime 标识。
7. Release 继续保留 npm lockfile、第三方许可证和签名检查。

当前构建仍复制 Worker 的生产 `node_modules`。Pi Provider SDK 会增加安装包体积；本阶段以可靠迁移为优先，记录 DMG 和 `.app` 体积变化。后续仅在体积数据超过发布预算时新增 Worker bundling/tree-shaking 阶段。

### 文件范围

| 范围 | 主要文件 |
|---|---|
| Worker 内核 | `agent-worker/src/piRuntime.ts`、`mindMapTask.ts`、`assistantTask.ts`、`index.ts` |
| Worker 测试 | `piRuntime.test.ts`、任务测试、Worker host 测试 |
| 依赖 | `agent-worker/package.json`、`package-lock.json` |
| Tauri 监管 | `src-tauri/src/agent_worker.rs`、对应 Rust 测试 |
| 构建门禁 | `verify_pr6_*`、`verify_pr7_*`、`verify_pr12_*` |
| CI/Release | `ci.yml`、`release.yml`、`release-dev.yml` |

第一阶段不修改 Backend 数据结构、数据库迁移、前端任务页面和 Artifact Schema。

### 回退策略

1. PR-15 不写入 Pi 专属持久化状态，旧版本仍能读取现有任务和 Artifact。
2. 回退通过安装上一稳定版本或将分支回退到 0.12.7 完成。
3. 不保留自动 direct-provider fallback，避免超时、网络异常或部分响应后重复请求和重复计费。
4. Provider 失败保留明确终态和脱敏错误，交由用户主动重试。
5. 未完成任务在应用重启后继续使用现有 checkpoint 恢复规则。

### 验收标准

- [ ] 两个 Pi 包精确锁定到 `0.80.10`，旧 OpenCode SDK 完全移除。
- [ ] Worker 使用 `runtime: "pi-agent-core"` 启动，无系统 Pi/OpenCode CLI 依赖。
- [ ] OpenAI-compatible、Google、Anthropic 均能构造隔离的 Pi Runtime。
- [ ] Pi Agent 固定 `tools: []`，请求级 API key 不跨任务泄漏。
- [ ] `stopReason=error`、`aborted`、`length`、空响应均不能产生 Artifact。
- [ ] 用户取消得到唯一 `task_cancelled` 终态。
- [ ] 超时可配置，默认值为 300 秒。
- [ ] 思维导图和 Assistant 保持现有结果 Schema。
- [ ] Worker typecheck、单元测试和 build 通过。
- [ ] Rust Worker 测试、PR-6、PR-7、PR-12 门禁通过。
- [ ] CI 和 Release 使用 Node.js `22.19.0` 或更高版本。
- [ ] 打包后的 Bundled Node 能导入 Pi，干净环境启动 Worker 并收到 ready/heartbeat。

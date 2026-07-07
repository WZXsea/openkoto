# PR-4.1 验收说明

## 目标

PR-4.1 确认 OpenKoto Backend 骨架可以独立启动、连接 PostgreSQL、执行 SQLx migration，并暴露稳定的 `GET /health`。

## 已实现范围

| 类别 | 验收项 |
|---|---|
| 开发环境 | `docker-compose.dev.yml` 启动 PostgreSQL 16 |
| 配置 | `.env.example` 覆盖 `DATABASE_URL`、`OPENKOTO_BACKEND_BIND`、`OPENKOTO_JWT_SECRET`、`OPENKOTO_FILE_STORAGE_DIR` |
| 后端启动 | `cargo run --manifest-path openkoto-backend/Cargo.toml` 启动 Axum 服务 |
| 数据库 | 启动时建立 SQLx PostgreSQL 连接池并执行 migrations |
| API | `GET /health` 返回 2xx JSON |
| 错误格式 | 后端错误统一返回 `{ "error": { "code", "message" } }` |
| 测试 | `cargo check`、`cargo test` 通过 |

## 推荐验收命令

从仓库根目录执行：

```bash
bash script/verify_pr4_1_backend_env.sh --start-db
```

或手动分步执行：

```bash
docker compose -f docker-compose.dev.yml up -d openkoto-postgres
cargo check --manifest-path openkoto-backend/Cargo.toml
cargo test --manifest-path openkoto-backend/Cargo.toml
cargo run --manifest-path openkoto-backend/Cargo.toml
```

另开终端检查健康状态：

```bash
curl -fsS http://127.0.0.1:4000/health
```

## Migration 验收

当前 migration 创建 `backend_metadata` 表并写入 PR-4.1 schema 标记。SQLx 自身维护 `_sqlx_migrations` 表，重复运行 migration 不应破坏 schema。

PR-4.2 开始再加入 `users`、`sessions` 等正式业务表。

## Desktop 边界

PR-4.1 不改 Desktop 读写路径，不改现有 Tauri command，不切换前端数据源。Desktop 仍按 PR-3 的本地模式工作。

## 外部依赖边界

PR-4.1 新增的唯一外部运行依赖是 PostgreSQL。不得在此阶段引入：

1. Anki / AnkiConnect。
2. Zotero。
3. MinerU。
4. LanguageTool。
5. MCP server 或 MCP client。
6. 浏览器插件桥接。

# PR-4.1 验收说明

## 目标

PR-4.1 的验收目标是确认 OpenKoto Backend 骨架可以独立启动、连接 PostgreSQL、执行 SQLx migration，并暴露稳定的 `GET /health`。

本文件用于指导主线实现者验收后端骨架。本 worker 只新增文档、dev compose 和检查脚本。

## 必须满足

| 类别 | 验收项 |
|---|---|
| 开发环境 | `docker-compose.dev.yml` 可以启动 PostgreSQL 16 |
| 配置 | `.env.example` 覆盖 `DATABASE_URL`、`OPENKOTO_BACKEND_BIND`、`OPENKOTO_JWT_SECRET`、`OPENKOTO_FILE_STORAGE_DIR` |
| 后端启动 | `cargo run --manifest-path openkoto-backend/Cargo.toml` 可以启动服务 |
| 数据库 | 启动时建立连接池并执行 SQLx migrations |
| API | `GET /health` 返回 2xx 和 JSON |
| 错误格式 | 后端错误统一返回 `{ "error": { "code", "message", "request_id" } }` |
| 测试 | `cargo check`、`cargo test`、migration repeatability test 通过 |

## 推荐验收命令

从仓库根目录执行：

```bash
bash script/verify_pr4_1_backend_env.sh --start-db
```

加载环境变量：

```bash
cp openkoto-backend/.env.example openkoto-backend/.env
set -a
. openkoto-backend/.env
set +a
```

后端 skeleton 完成后执行：

```bash
cargo check --manifest-path openkoto-backend/Cargo.toml
cargo test --manifest-path openkoto-backend/Cargo.toml
cargo run --manifest-path openkoto-backend/Cargo.toml
```

另开终端检查健康状态：

```bash
curl -fsS http://127.0.0.1:4000/health
```

预期响应示例：

```json
{
  "status": "ok",
  "service": "openkoto-backend",
  "database": "ok"
}
```

## Migration 验收建议

SQLx migration 需要至少覆盖以下行为：

1. 空数据库启动时自动创建基础表。
2. 重复运行 migration 不报错。
3. 数据库连接失败时 `/health` 不返回误导性的全绿状态。
4. migration 失败时服务启动失败，并输出明确日志。

PR-4.1 的第一版 migration 可以只建立后续阶段需要的最小基础：

```sql
CREATE TABLE IF NOT EXISTS schema_migrations_probe (
  id BIGSERIAL PRIMARY KEY,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

后续 PR-4.2 开始再加入 `users`、`sessions` 等正式业务表。

## Desktop 边界

PR-4.1 不改 Desktop 读写路径，不改现有 Tauri command，不切换前端数据源。Desktop 仍按 PR-3 的本地模式工作。

可接受的 Desktop 相关动作只有：

1. 文档中说明未来连接方式。
2. 后续 PR-4.4 再引入 Backend client。

## 外部依赖边界

PR-4.1 新增的唯一外部运行依赖是 PostgreSQL。不得在此阶段引入：

1. Anki / AnkiConnect。
2. Zotero。
3. MinerU。
4. LanguageTool。
5. MCP server 或 MCP client。
6. 浏览器插件桥接。

## 完成标准

PR-4.1 完成后，主线应能够给出以下证据：

1. `docker compose -f docker-compose.dev.yml up -d openkoto-postgres` 成功。
2. `cargo check --manifest-path openkoto-backend/Cargo.toml` 成功。
3. `cargo test --manifest-path openkoto-backend/Cargo.toml` 成功。
4. `curl -fsS http://127.0.0.1:4000/health` 返回健康 JSON。
5. 重复启动服务不会重复破坏数据库 schema。

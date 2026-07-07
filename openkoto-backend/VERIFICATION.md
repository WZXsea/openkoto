# PR-4.2 Backend 用户账户和认证验收说明

## 目标

PR-4.2 确认 OpenKoto Backend 可以在 PR-4.1 骨架基础上创建 PostgreSQL 用户账户和登录会话，并通过 Bearer JWT 暴露最小认证闭环：

1. 用户注册：`POST /auth/register`。
2. 用户登录：`POST /auth/login`。
3. 当前用户查询：`GET /auth/me`。

## 已实现范围

| 类别 | 验收项 |
|---|---|
| 开发环境 | `docker-compose.dev.yml` 启动 PostgreSQL 16 |
| 配置 | `.env.example` 覆盖 `DATABASE_URL`、`OPENKOTO_BACKEND_BIND`、`OPENKOTO_JWT_SECRET`、`OPENKOTO_FILE_STORAGE_DIR` |
| 可选测试配置 | `OPENKOTO_TEST_DATABASE_URL` 控制 PostgreSQL 集成测试；未设置时 DB 集成测试跳过 |
| 后端启动 | `cargo run --manifest-path openkoto-backend/Cargo.toml` 启动 Axum 服务 |
| 数据库 | 启动时建立 SQLx PostgreSQL 连接池并执行 migrations |
| 账户表 | migration 创建 `users` 表，保存用户标识、邮箱、可选显示名、Argon2id password hash 和时间戳 |
| 会话表 | migration 创建 `sessions` 表，记录登录会话、`token_hash`、过期时间和撤销状态，并关联 `users` |
| 密码 | 注册和登录使用 Argon2id 校验密码，不保存明文密码 |
| 认证 | 登录和注册返回 Bearer JWT；`GET /auth/me` 从 `Authorization` header 读取 token |
| API | `GET /health` 返回 2xx JSON |
| API | `POST /auth/register`、`POST /auth/login`、`GET /auth/me` 返回稳定 JSON |
| 错误格式 | 后端错误统一返回 `{ "error": { "code", "message" } }` |
| 测试 | `cargo check`、`cargo test` 通过；无本地 PostgreSQL 时普通测试不失败 |

## 推荐验收命令

从仓库根目录执行：

```bash
bash script/verify_pr4_2_backend_auth.sh --start-db
```

或手动分步执行静态检查和普通测试：

```bash
docker compose -f docker-compose.dev.yml up -d openkoto-postgres
cargo check --manifest-path openkoto-backend/Cargo.toml
cargo test --manifest-path openkoto-backend/Cargo.toml
cargo run --manifest-path openkoto-backend/Cargo.toml
```

普通 `cargo test` 不要求本地 PostgreSQL。未设置 `OPENKOTO_TEST_DATABASE_URL` 时，数据库集成测试应被跳过。

需要显式运行 PostgreSQL 集成测试时：

```bash
OPENKOTO_TEST_DATABASE_URL=postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev \
  cargo test --manifest-path openkoto-backend/Cargo.toml
```

若本机 `5433` 已被其他容器或本地 PostgreSQL 占用，可改用其他宿主机端口：

```bash
OPENKOTO_POSTGRES_PORT=55433 bash script/verify_pr4_2_backend_auth.sh --start-db
```

另开终端检查健康状态和认证接口：

```bash
curl -fsS http://127.0.0.1:4000/health
```

## 认证接口验收

### 注册

```bash
curl -i -X POST http://127.0.0.1:4000/auth/register \
  -H 'Content-Type: application/json' \
  -d '{
    "email": "reader@example.com",
    "password": "correct-horse-battery-staple",
    "display_name": "Reader"
  }'
```

期望结果：

1. 首次请求返回 2xx JSON。
2. 响应包含 `user.email`。
3. 响应包含可用于 Bearer 认证的 `token`。
4. 重复注册同一邮箱返回 4xx，并保持统一错误格式。

### 登录

```bash
curl -i -X POST http://127.0.0.1:4000/auth/login \
  -H 'Content-Type: application/json' \
  -d '{
    "email": "reader@example.com",
    "password": "correct-horse-battery-staple"
  }'
```

期望结果：

1. 正确密码返回 2xx JSON，包含 `user` 和 `token`。
2. 错误密码返回 401。
3. 错误密码响应不得暴露用户是否存在以外的敏感信息。

### 当前用户

```bash
TOKEN="$(curl -fsS -X POST http://127.0.0.1:4000/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"reader@example.com","password":"correct-horse-battery-staple"}' \
  | jq -r '.token')"

curl -i http://127.0.0.1:4000/auth/me \
  -H "Authorization: Bearer ${TOKEN}"
```

期望结果：

1. 有效 token 返回 2xx JSON，包含当前 `user`。
2. 缺少 `Authorization` header 返回 401。
3. 无效 Bearer token 返回 401。

## 错误响应验收

统一错误响应外形：

```json
{
  "error": {
    "code": "invalid_credentials",
    "message": "invalid email or password"
  }
}
```

认证相关错误至少覆盖：

| 场景 | HTTP Status | 示例 code |
|---|---:|---|
| 请求字段格式无效 | 400 | `invalid_email` / `weak_password` |
| 重复注册邮箱 | 409 | `email_already_registered` |
| 登录凭证错误 | 401 | `invalid_credentials` |
| 缺少或无效 Bearer token | 401 | `missing_bearer_token` / `invalid_token` |

## Migration 验收

PR-4.1 migration 创建 `backend_metadata` 表并写入 schema 标记。PR-4.2 migration 增加认证业务表：

1. `users` 保存用户主记录、唯一 normalized email、可选显示名、Argon2id password hash 和时间戳。
2. `sessions` 保存登录会话、`token_hash`、过期时间、撤销状态，外键关联 `users`。
3. SQLx 自身维护 `_sqlx_migrations` 表。
4. 重复启动后端或重复运行 migration 不应破坏 schema。

## Desktop 边界

PR-4.2 不改 Desktop 读写路径，不改现有 Tauri command，不切换前端数据源。Desktop 仍按 PR-3 的本地模式工作。

## 外部依赖边界

PR-4.2 新增的唯一外部运行依赖仍是 PostgreSQL。不得在此阶段引入：

1. Anki / AnkiConnect。
2. Zotero。
3. MinerU。
4. LanguageTool。
5. MCP server 或 MCP client。
6. 浏览器插件桥接。

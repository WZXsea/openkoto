# OpenKoto Backend

`openkoto-backend` 是 OpenKoto Desktop + Backend + PostgreSQL 架构中的 Axum 后端服务。当前后端覆盖认证、素材、文件、学习状态、旧 JSON 导入和 PR-5 本地学习条目候选箱。

## 范围

当前后端提供：

- Rust Axum HTTP service。
- SQLx PostgreSQL connection pool。
- 启动时执行 SQLx migrations。
- PostgreSQL `users` / `sessions` 表。
- Argon2id 密码哈希，不保存明文密码。
- Bearer JWT 认证，签名密钥来自 `OPENKOTO_JWT_SECRET`。
- `GET /health`。
- `POST /auth/register`。
- `POST /auth/login`。
- `GET /auth/me`。
- PostgreSQL `materials` / `material_segments` / `files` 表。
- `GET /materials`、`POST /materials`、`GET /materials/{id}`、`PATCH /materials/{id}`、`DELETE /materials/{id}`。
- `POST /files`、`GET /files/{id}`。
- PR-5 本地学习条目候选箱：`GET /learning-items`、`POST /learning-items`、`POST /learning-items/from-selection`、`GET /learning-items/{id}`、`PATCH /learning-items/{id}`、`DELETE /learning-items/{id}`、`POST /learning-items/bulk-status`。
- 统一错误响应格式。
- `OPENKOTO_TEST_DATABASE_URL` 控制的可选数据库集成测试。

Desktop 现在以 Backend-first 方式访问核心素材和学习状态；旧本地 JSON 只作为 legacy import 来源保留。

## 本地开发

从仓库根目录启动 PostgreSQL：

```bash
docker compose -f docker-compose.dev.yml up -d openkoto-postgres
```

准备本地配置并启动后端：

```bash
cp openkoto-backend/.env.example openkoto-backend/.env
cargo run --manifest-path openkoto-backend/Cargo.toml
```

后端优先加载当前目录下的 `.env`，随后回退到 `openkoto-backend/.env`。

## 配置

| 变量 | 默认值 | 用途 |
|---|---|---|
| `DATABASE_URL` | `postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev` | 运行时 PostgreSQL 连接串。后端启动和 migration 需要可连接数据库。 |
| `OPENKOTO_TEST_DATABASE_URL` | unset | 可选测试数据库连接串。未设置时，`cargo test` 跳过 PostgreSQL 集成测试，只运行不依赖数据库的测试。 |
| `OPENKOTO_BACKEND_BIND` | `127.0.0.1:4000` | Axum HTTP 监听地址。 |
| `OPENKOTO_JWT_SECRET` | `openkoto-dev-insecure-change-me` | JWT 签名密钥。认证接口要求该值不是默认值，且长度至少 32 bytes。 |
| `OPENKOTO_FILE_STORAGE_DIR` | `.data/files` | 后续文件存储目录。 |
| `OPENKOTO_POSTGRES_PORT` | `5433` | `docker-compose.dev.yml` 暴露 PostgreSQL 的宿主机端口；仅用于 dev compose 和验证脚本。 |

## 素材接口

所有素材接口都需要 `Authorization: Bearer <jwt>`。

```bash
curl -i -X POST http://127.0.0.1:4000/materials \
  -H "Authorization: Bearer ${TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{
    "title": "Academic Reading",
    "content": "Dr. Smith reviewed it. It worked.",
    "source_type": "article"
  }'
```

响应保持现有 Desktop `Article` 兼容形状，`segments` 始终返回数组。

## 文件接口

文件上传使用 `multipart/form-data`，文件下载使用同一个 Bearer token 保护。

```bash
curl -i -X POST http://127.0.0.1:4000/files \
  -H "Authorization: Bearer ${TOKEN}" \
  -F 'metadata={"purpose":"source"}' \
  -F 'file=@paper.pdf'
```

上传响应包含 `download_url`，例如 `/files/{id}`。后端文件库默认位于 `OPENKOTO_FILE_STORAGE_DIR`，数据库只保存受控 storage path，不使用用户提交的文件名作为磁盘路径。

## 学习条目接口

所有学习条目接口都需要 `Authorization: Bearer <jwt>`。学习条目用于保存本地 candidate inbox，不连接 Anki/FSRS；复习调度仍由后续 Anki 集成负责。

### 从划词创建候选

```bash
curl -i -X POST http://127.0.0.1:4000/learning-items/from-selection \
  -H "Authorization: Bearer ${TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{
    "material_id": "00000000-0000-0000-0000-000000000000",
    "segment_id": "00000000-0000-0000-0000-000000000001",
    "selected_text": "mitigate",
    "source_sentence": "Macrophages can mitigate inflammatory damage.",
    "tags": ["immunology", "academic"]
  }'
```

后端会校验 `material_id` / `segment_id` 属于当前用户。相同用户、来源、条目类型、文本和 source sentence 的重复提交会返回既有条目，不新增重复候选。

### 直接创建或更新

```bash
curl -i -X POST http://127.0.0.1:4000/learning-items \
  -H "Authorization: Bearer ${TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{
    "item_type": "word",
    "text": "attenuate",
    "source_sentence": "The drug attenuated the inflammatory response.",
    "meaning_in_context": "to reduce the strength of a response",
    "definition_en": "to make something weaker",
    "definition_zh": "减弱；缓和",
    "collocations": [{"text": "attenuate the response"}],
    "examples": [{"text": "The intervention attenuated the signal."}],
    "tags": ["academic"],
    "status": "candidate",
    "priority": 10,
    "difficulty": 3,
    "review_state": {}
  }'
```

`item_type` 当前允许 `word`、`phrase`、`sentence`、`grammar`。`status` 当前允许 `candidate`、`accepted`、`rejected`、`archived`。`collocations`、`examples`、`tags` 和 `review_state` 存入 PostgreSQL JSONB。

### 查询和状态流转

```bash
curl -i 'http://127.0.0.1:4000/learning-items?status=candidate&limit=100' \
  -H "Authorization: Bearer ${TOKEN}"

curl -i -X PATCH http://127.0.0.1:4000/learning-items/${ITEM_ID} \
  -H "Authorization: Bearer ${TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{"status":"accepted","review_state":{"local":"new"}}'

curl -i -X POST http://127.0.0.1:4000/learning-items/bulk-status \
  -H "Authorization: Bearer ${TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{"ids":["00000000-0000-0000-0000-000000000000"],"status":"archived"}'
```

列表接口支持 `status`、`item_type`、`material_id`、`limit`、`offset` 查询参数。所有读取、更新、删除都按 `user_id` 隔离，其他用户访问返回 `404` 或空列表。

## 健康检查

```bash
curl -fsS http://127.0.0.1:4000/health
```

响应外形：

```json
{
  "status": "ok",
  "service": "openkoto-backend",
  "version": "0.1.0",
  "database": {
    "connected": true,
    "latency_ms": 1
  },
  "file_storage": {
    "ready": true
  },
  "auth": {
    "configured": true
  },
  "started_at": "2026-07-07T00:00:00Z"
}
```

## 认证接口

所有认证接口使用 JSON request/response。登录后客户端通过 `Authorization: Bearer <jwt>` 访问需要认证的接口。

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

成功响应示例：

```json
{
  "user": {
    "id": "018f4e7a-7b49-7b52-9f6f-682dbdbe9f42",
    "email": "reader@example.com",
    "display_name": "Reader",
    "created_at": "2026-07-07T12:00:00Z",
    "updated_at": "2026-07-07T12:00:00Z"
  },
  "token": "<jwt>",
  "token_type": "bearer",
  "expires_at": "2026-08-06T12:00:00Z"
}
```

### 登录

```bash
curl -i -X POST http://127.0.0.1:4000/auth/login \
  -H 'Content-Type: application/json' \
  -d '{
    "email": "reader@example.com",
    "password": "correct-horse-battery-staple"
  }'
```

成功响应与注册接口一致，返回当前用户和新的 Bearer JWT。

### 当前用户

```bash
TOKEN="$(curl -fsS -X POST http://127.0.0.1:4000/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"reader@example.com","password":"correct-horse-battery-staple"}' \
  | jq -r '.token')"

curl -i http://127.0.0.1:4000/auth/me \
  -H "Authorization: Bearer ${TOKEN}"
```

成功响应示例：

```json
{
  "user": {
    "id": "018f4e7a-7b49-7b52-9f6f-682dbdbe9f42",
    "email": "reader@example.com",
    "display_name": "Reader",
    "created_at": "2026-07-07T12:00:00Z",
    "updated_at": "2026-07-07T12:00:00Z"
  }
}
```

## 错误响应

后端错误统一返回以下外形：

```json
{
  "error": {
    "code": "invalid_credentials",
    "message": "invalid email or password"
  }
}
```

常见认证错误包括：

| HTTP Status | 示例 code | 含义 |
|---|---|---|
| `400` | `invalid_email` / `weak_password` | 请求字段格式无效。 |
| `401` | `invalid_credentials` | 登录邮箱或密码错误。 |
| `401` | `missing_bearer_token` / `invalid_token` | 缺少 Bearer token、token 无效或 token 已失效。 |
| `409` | `email_already_registered` | 注册邮箱已存在。 |

## 验证

```bash
bash script/verify_pr4_3_materials_db.sh
cargo check --manifest-path openkoto-backend/Cargo.toml
cargo test --manifest-path openkoto-backend/Cargo.toml
```

普通 `cargo test` 在未设置 `OPENKOTO_TEST_DATABASE_URL` 时会跳过 PostgreSQL 集成测试。需要运行 DB 集成测试时，先启动 PostgreSQL，再显式传入测试数据库连接串：

```bash
docker compose -f docker-compose.dev.yml up -d openkoto-postgres
OPENKOTO_TEST_DATABASE_URL=postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev \
  cargo test --manifest-path openkoto-backend/Cargo.toml
```

当本机 `5433` 已被其他项目占用时，可以指定开发数据库端口：

```bash
OPENKOTO_POSTGRES_PORT=55433 bash script/verify_pr4_3_materials_db.sh --start-db
```

需要 verification script 代为启动开发数据库时使用 `--start-db`：

```bash
bash script/verify_pr4_3_materials_db.sh --start-db
```

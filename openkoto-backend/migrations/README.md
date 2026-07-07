# Migrations

本目录存放 `openkoto-backend` 的 SQLx PostgreSQL migrations。

命名约定：

```text
YYYYMMDDHHMMSS_short_description.sql
```

## 当前 schema 阶段

| 阶段 | 用途 |
|---|---|
| PR-4.1 | Backend bootstrap migration。创建 `backend_metadata`，SQLx 维护 `_sqlx_migrations`。 |
| PR-4.2 | Authentication migration。增加 `users` 和 `sessions`，用于本地账户注册、登录和 Bearer JWT session tracking。 |
| PR-4.3 | Materials/files migration。增加 `materials`、`material_segments` 和 `files`，用于核心素材、段落和文件库。 |

## PR-4.2 认证表

`users` 保存持久账户记录：

- 稳定 user id。
- 唯一 email。
- 可选 display name。
- Argon2id password hash。
- 创建和更新时间戳。

`sessions` 保存登录会话和 JWT metadata：

- 稳定 session id。
- 指向 `users` 的 `user_id` foreign key。
- 不可逆的 `token_hash`。
- session 创建和过期时间戳。
- `revoked_at` 和 `last_used_at` metadata。

Migrations 不得保存明文密码或明文 Bearer token。密码只以 Argon2id hash 表示；token 持久化只应保存 metadata 或不可逆 token identifier。

## PR-4.3 素材和文件表

`materials` 保存 Article-like 素材记录，包含标题、正文、来源类型、来源 URL、媒体/书籍路径、翻译状态和 mind map artifact id。

`material_segments` 保存素材段落，数据库列名使用 `segment_order`，API 序列化为现有前端兼容的 `order` 字段。

`files` 保存上传文件 metadata、sha256 和受控 storage path。用户提交的文件名只作为 `original_name` 展示，不作为磁盘路径。

## 测试数据库

`OPENKOTO_TEST_DATABASE_URL` 只用于可选 PostgreSQL integration tests。未设置该变量时，`cargo test --manifest-path openkoto-backend/Cargo.toml` 应跳过 DB integration tests，使本地无 PostgreSQL 时仍可运行普通测试。

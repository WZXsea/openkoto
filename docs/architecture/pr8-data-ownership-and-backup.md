## PR-8 数据所有权与备份说明
### 适用范围
本文定义 OpenKoto Desktop、打包本地 Backend、PostgreSQL 和历史本地 JSON 共存时的数据事实源、备份边界及恢复顺序。路径中的 `${app_data_dir}` 表示 Tauri `app_data_dir()` 返回的应用数据目录；开发环境和打包环境的实际绝对路径可以不同。

PR-8 不把数据库、文件目录、配置文件或日志合并成一个模糊的“应用目录”概念。恢复和删除必须按下表的所有权执行。

### 事实源与责任边界
| 数据对象 | 当前路径或位置 | 事实源 | 责任边界 |
| --- | --- | --- | --- |
| PostgreSQL data | 开发环境为 Docker volume `openkoto_postgres_data`；打包环境为 `${app_data_dir}/backend/postgres-data`；外部部署由 `DATABASE_URL` 指向外部实例 | PostgreSQL 数据库本身 | Backend 负责 schema、migration 和接口；Desktop 不直接修改数据库文件 |
| backend/files | 开发默认 `.data/files`；打包环境 `${app_data_dir}/backend/files` | 文件字节由该目录保存，`files` 表保存元数据、相对 `storage_path` 和 SHA-256 | 必须与 PostgreSQL 的 `files` 表成对备份和恢复；文件名不是事实源，数据库记录的受控相对路径才是映射依据 |
| jwt_secret | 打包环境 `${app_data_dir}/backend/jwt_secret`；开发或外部 Backend 使用 `OPENKOTO_JWT_SECRET` | 当前运行 Backend 使用的 secret 来源 | 认证签名密钥，不进入 Git、普通备份包或日志；丢失会使既有 JWT 无法验证 |
| App config | `${app_data_dir}/config.json` | Desktop 本地配置文件 | 保存 UI 偏好、Backend URL、登录 token、模型/ASR provider 配置；Backend 业务数据不以此文件为事实源 |
| legacy JSON `agent_tasks` / `artifacts` | `${app_data_dir}/agent_tasks/*.json`、`${app_data_dir}/artifacts/articles/` | 在迁移完成前是旧本地数据的原始来源；迁移成功后对应 Backend 的 `agent_tasks` / `artifacts` 表 | 仅作为 legacy import 输入保留，不应继续作为新的 Backend-first 写入路径；迁移完成且核对通过前禁止删除 |
| 日志 | `${app_data_dir}/logs/openkoto.log` | 诊断记录，不是业务事实源 | 由 Desktop `LogStore` 追加写入；可按事件窗口保留，不能用日志恢复业务数据 |
| books | `${app_data_dir}/books` | Desktop 本地书籍二进制文件；Backend `materials.book_path` 保存关联路径，导入成功时通常也会上传到 Backend `files` | 数据库记录不能重建原始书籍文件；删除前必须确认 Backend 文件副本和材料引用状态 |
| videos | `${app_data_dir}/videos` | Desktop 本地视频及字幕缓存；Backend `materials.media_path` 保存关联路径，导入成功时通常也会上传到 Backend `files` | 视频、字幕和材料元数据需要一起考虑；删除缓存前必须确认是否仍承担离线播放或重新导入职责 |

#### PostgreSQL data
PostgreSQL 是 Backend-first 材料、素材分段、文件元数据、用户、会话、学习状态、学习条目、导入批次以及 Backend 侧 agent task/artifact 记录的事实源。迁移文件位于 `openkoto-backend/migrations/`，属于版本控制中的 schema 定义，不替代数据库备份。

开发环境的 `docker-compose.dev.yml` 使用名为 `openkoto_postgres_data` 的 volume。打包运行时由 `packaged_backend.rs` 管理 `${app_data_dir}/backend/postgres-data`，并在应用关闭时由 Desktop 管理进程生命周期。使用外部 `DATABASE_URL` 时，数据库由外部 PostgreSQL 运维方负责，应用目录中的 `postgres-data` 不代表外部数据库。

#### backend/files
`files` 表和 `backend/files` 目录是一个逻辑对象的两部分：表记录用户、原始文件名、大小、内容类型、相对存储路径、元数据和 SHA-256；目录保存实际字节。只备份其中一部分会产生“数据库有记录但文件不存在”或“磁盘有孤儿文件”的不一致。

### 备份范围
#### 必须备份
1. PostgreSQL：使用与部署形态匹配的逻辑备份或物理备份。逻辑备份应覆盖 schema、用户/会话、材料、文件元数据、学习状态、学习条目、legacy import 批次及 Backend agent task/artifact 数据。
2. `backend/files`：保留完整相对目录结构和文件字节，并在备份清单中记录路径、大小和 SHA-256。
3. `${app_data_dir}/books` 和 `${app_data_dir}/videos`：保留仍需离线访问、重新解析或重新导入的书籍、视频、字幕文件。
4. `${app_data_dir}/config.json`：仅在加密备份中保留；它包含 `auth_token` 和模型配置中的 API key，不能当作普通非敏感配置归档。
5. legacy JSON：在每个 legacy import batch 被核对并确认可恢复前，保留 `agent_tasks`、`artifacts/articles` 以及同一批次所依赖的其他旧 JSON。

#### 条件备份
1. `${app_data_dir}/logs/openkoto.log` 只为故障诊断或审计事件保留，建议按事件窗口导出并设置短期保留期。
2. `${app_data_dir}/backend/backend.sha256`、PID 文件和 socket 目录属于运行时状态，不是恢复所需业务数据，不应作为数据库备份的替代物。
3. PostgreSQL 的 Docker volume 或打包 `postgres-data` 可以做停机物理快照，但跨版本恢复仍必须遵循 PostgreSQL 官方兼容性要求，并在副本上先验证。

#### PR-8 升级备份实现边界
Desktop 的 `data_backup` 模块会在打包 Backend 二进制发生变化且已有 PostgreSQL 数据目录时生成 `backend/backups/pre-upgrade-*`，保存 PostgreSQL 快照、可选的 `config.json`、可选的 `jwt_secret` 和 `backend/files` 的大小/SHA-256 清单，并执行 dry restore 检查。该升级备份目前只记录 `backend/files` 清单，不复制文件字节，也不包含 books、videos、legacy JSON 或日志；这些对象仍必须由外部备份任务按本页范围保存。升级备份属于回滚保护，不替代完整的数据库、文件和 secret 备份。

### 恢复顺序
1. 停止 Desktop、打包 Backend、PostgreSQL 写入进程，保留故障现场和当前目录清单；恢复前不得让旧进程继续写入。
2. 恢复 PostgreSQL data。开发 Docker 环境恢复 `openkoto_postgres_data` 或导入 `pg_dump`；打包环境恢复 `${app_data_dir}/backend/postgres-data`。启动数据库后确认连接正常，并让应用执行或验证对应 migrations。
3. 恢复 `backend/files`，保持数据库 `files.storage_path` 所需的相对路径和文件权限。逐项用文件大小与 SHA-256 对照 `files` 表，先处理缺失或不匹配项。
4. 恢复 `jwt_secret`：打包环境将密钥文件写回 `${app_data_dir}/backend/jwt_secret`，外部或开发 Backend 通过受控环境变量注入。恢复正确 secret 后再启动 Backend；不得用新随机值静默替换，否则旧 JWT 会全部失效。
5. 恢复 `config.json` 和 `books`、`videos`。先确认 `backend_url` 指向已恢复 Backend，再恢复本地媒体文件；`auth_token` 或模型 API key 需要按 secret 策略重新验证，必要时重新登录或轮换。
6. 恢复尚未完成迁移的 legacy JSON，并确认 `agent_tasks`、`artifacts` 与材料文件的关联路径仍存在。已完成迁移的记录以 Backend 表为准，legacy 文件仅作为回滚证据保留。
7. 启动 Backend 并检查 health、认证、材料数量、文件数量和代表性文件下载；随后启动 Desktop，验证书籍/视频读取、历史任务和 artifact 读取。
8. 最后按需恢复日志归档或导入故障窗口日志。日志恢复不应阻塞业务数据恢复，也不应覆盖新会话产生的日志。

### 删除策略
#### 业务数据
1. PostgreSQL 业务数据优先通过应用 API 执行删除或归档，遵循 `archived_at` 等现有软删除字段和表的外键约束。不得直接删除 PostgreSQL 数据目录中的单个关系文件。
2. Backend 文件只有在确认 `files` 表记录已按业务规则删除、没有材料或导入任务引用、且保留期已到后才能清理。当前文件接口没有提供通用的孤儿文件清理证明流程，人工清理前必须先生成引用检查和 SHA-256 清单。
3. 书籍和视频的本地副本只有在确认材料元数据、Backend 文件副本、字幕/解析结果和离线需求都不再依赖它们后才能删除。删除本地缓存不会自动删除 PostgreSQL 材料记录。

#### Legacy 与日志
1. legacy `agent_tasks` / `artifacts` 只能在 import batch 状态完成、失败项已处理、目标表和文件已抽样核对、并且已生成独立备份后删除。删除后不能把 legacy JSON 当作新的事实源恢复。
2. 日志按诊断和隐私要求设置保留期；可通过应用的日志清理命令截断当前日志。日志可能包含 URL、错误上下文或用户输入，外发前必须脱敏。
3. 应用卸载、目录迁移或“清空数据”操作必须显式列出上述目录；不能因为删除 `config.json` 就认为 PostgreSQL、Backend 文件或媒体数据也已删除。

### Secret 处理
1. `jwt_secret`、`config.json` 中的 `auth_token`、模型/ASR API key、数据库连接串中的密码均按 secret 管理。禁止提交 Git、写入验证输出、放入普通 zip、截图或日志。
2. secret 备份使用加密的 secret manager 或加密存储，密钥与备份分离；恢复时设置最小文件权限，并验证进程用户可读、其他用户不可读。
3. 只要怀疑 secret 泄露，应先轮换对应 provider/API key 和 JWT secret，再清理旧 token 或要求重新登录。JWT secret 轮换会使旧 token 失效，数据库内容本身不因轮换而改变。
4. 验证脚本只检查配置键、路径和代码契约，不读取真实 secret 值；测试应使用临时数据库和临时 secret。文档和脚本不得使用生产连接串或真实 token 作为示例。

### 证据来源与维护
本文当前事实依据为 `openkoto-backend/src/config.rs`、`src/database.rs`、`src/files.rs`，`openkoto-backend/migrations/`，以及 `textlingo-desktop/src-tauri/src/data_backup.rs`、`source_locator.rs`、`packaged_backend.rs`、`storage.rs`、`logging.rs`、`video_server.rs` 和 `commands.rs`。当 PR-8 后续 worker 新增数据完整性模块或 migration 时，应同步更新本页的事实源、备份清单、恢复顺序和删除策略，并调整 `script/verify_pr8_data_integrity.sh` 的实现锚点。

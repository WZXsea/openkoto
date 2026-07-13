# OpenKoto 0.7.0 macOS 安装与数据说明
## 适用版本
本页适用于 WZX PR-7 的 `OpenKoto Desktop_0.7.0_aarch64.dmg`，仅支持 Apple Silicon Mac。Intel 用户应等待对应架构的发行资产。
## 安装
1. 从 [WZX PR-7 GitHub Release](https://github.com/WZXsea/openkoto/releases/tag/wzx-v0.7.0-pr7-material-workbench) 下载 `.dmg`。
2. 打开 `.dmg`，将 `OpenKoto Desktop.app` 拖入 `Applications`。
3. 从“应用程序”打开 `OpenKoto Desktop`。
4. 应用会自动启动内置 PostgreSQL 和 Backend，用户无需安装 Docker、PostgreSQL、Node.js，也无需配置端口。
## 未签名预发布包
本地构建未使用 Apple Developer 证书签名或公证。macOS 可能阻止首次启动。仅在确认下载来源和 SHA-256 后，在“系统设置 -> 隐私与安全性”中允许打开；不要对来源不明的应用绕过 Gatekeeper。
## 完整性校验
本次 Apple Silicon DMG 的 SHA-256：
```text
08ae356ad16e7cf8e912d17ac89eeaeb7868f18a9dc79b70460f554158120188
```
终端校验命令：
```bash
shasum -a 256 "OpenKoto Desktop_0.7.0_aarch64.dmg"
```
## 运行方式
- 用户可见 Backend 固定为 `http://127.0.0.1:19421`。
- PostgreSQL 使用应用内部动态端口，用户无需管理。
- 首次启动会初始化数据库；后续启动复用原数据。
- Backend 或数据库尚未就绪时，应用显示启动门禁，不会退回旧 JSON 在线存储。
## 数据位置
应用数据位于：
```text
~/Library/Application Support/com.openkoto.desktop/
```
其中包括 PostgreSQL 数据、Backend 文件库、运行配置和进程状态。升级或替换 `/Applications/OpenKoto Desktop.app` 不会删除该目录。
## 升级与卸载
1. 升级前退出 OpenKoto。
2. 用新版本替换 `/Applications/OpenKoto Desktop.app`。
3. 首次启动新版本时 Backend 自动执行数据库 migration。
4. 仅删除应用不会删除学习数据。
5. 确认不再需要数据后，才手工删除 `~/Library/Application Support/com.openkoto.desktop/`。
## 0.7.0 实机验收
该构建已完成以下安装版流程：自动启动本地栈、恢复登录会话、创建并预览文章、唯一结果检索、写入 33% 阅读进度，以及停止 Desktop、Backend、PostgreSQL 后重新启动并恢复素材和进度。

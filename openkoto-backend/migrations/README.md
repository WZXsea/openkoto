# Migrations

该目录用于存放 PR-4.1 之后的 SQLx migration 文件。

命名建议：

```text
0001_init.sql
0002_auth.sql
0003_materials.sql
```

PR-4.1 的后端 skeleton 应在启动时执行 migration，并在测试中覆盖重复执行。

# 场景

[English](../SCENARIOS.md) | [中文文档索引](README.md)

每个场景都有稳定 ID、说明、Quick/Deep Profile 归属、平台能力规则和五种状态之一。

## Quick

- `AZ-S001 EMPTY_HOME`：把适用的主目录、Profile、配置和数据变量重定向到空目录。它不会列出或复制真实主目录。v0.1.0 不跟踪具体缺失文件。
- `AZ-S002 EMPTY_CACHE`：重定向 `XDG_CACHE_HOME`、`npm_config_cache`、`PIP_CACHE_DIR`、`UV_CACHE_DIR` 和 `GRADLE_USER_HOME`。它不会删除真实缓存，也不会猜测 Maven 缓存参数。
- `AZ-S003 CLEAN_ENV`：保留平台必需变量和 `[environment].preserve`。稳定失败、完整恢复和完成的 `ddmin` 可以产生 `PROVEN` 变量名集合。
- `AZ-S005 SPACE_WORKDIR`：复制到 `AssumeZero Test Workspace/project copy`。
- `AZ-S006 UNICODE_WORKDIR`：复制到 `项目-测试-Δ`；如果无法创建或使用该路径，则返回 `SKIPPED_UNSUPPORTED`。
- `AZ-S007 DEEP_WORKDIR`：使用可配置、安全有界的目标长度，不会故意超过已记录的操作系统限制。
- `AZ-S008 REDIRECTED_TEMP`：把 `TMP`、`TEMP` 和 `TMPDIR` 重定向到场景专属目录。

## Deep

- `AZ-S004 MINIMAL_PATH`：保留已解析的顶层程序目录、系统必需路径和显式保留条目。稳定恢复支持有序 `PATH` 条目 `ddmin`。
- `AZ-S009 TIMEZONE_UTC`：在支持的类 Unix 平台上设置进程级 `TZ=UTC`。这是 best effort，不是全系统时区变更。
- `AZ-S010 LOCALE_C`：只有在能可靠发现 C/POSIX Locale 时才设置 `LC_ALL=C` 和 `LANG=C`；不支持的平台会跳过。

## 状态

- `PASS`：Oracle 仍接受运行。
- `FAIL`：改变后的条件重复导致拒绝。
- `SKIPPED_UNSUPPORTED`：平台无法可靠启用该条件。
- `INCONCLUSIVE`：不稳定性或预算限制阻止了归因。
- `INFRASTRUCTURE_ERROR`：准备、复制或执行基础设施失败。

v0.1.0 未启用两两场景执行；详见[已知限制](LIMITATIONS.md)。

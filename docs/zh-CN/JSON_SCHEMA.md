# JSON 报告 Schema

[English](../JSON_SCHEMA.md) | [中文文档索引](README.md)

每次运行都会写入 `.assumezero/runs/<run-id>/report.json`，并符合 [report-v1.schema.json](../../schemas/report-v1.schema.json)。

CI 会使用 JSON Schema 2020-12 校验经过路径规范化的[演示报告](../demo/report-v1.example.json)。同一门禁会编译两个 Schema，并在对 TOML 进行无损 JSON 转换后，使用配置 Schema 校验 [examples/assumezero.toml](../../examples/assumezero.toml)。

稳定的顶层字段为：

```text
schema_version
tool_version
run_id
started_at
finished_at
platform
repository_fingerprint
configuration
command
baseline
baseline_status
scenarios
findings
budget
redaction_summary
workspace_integrity
```

Schema v1 中，`started_at` 和 `finished_at` 是 Unix Epoch 秒数字符串，持续时间是整数毫秒。

环境变量值不会出现在 Schema 的任何字段中。`restored_names` 只包含变量名或脱敏、规范化后的 `PATH` 条目。捕获的输出是有界、脱敏的摘要，并带有 `output_truncated` 标志。

使用方必须：

- 拒绝不支持的 `schema_version`；
- 把未知枚举值视为兼容性信号；
- 区分场景状态与发现的证据等级；
- 不把 `SUSPECTED` 当作已证明；
- 不假设 1-minimal 集合是全局最小集合。

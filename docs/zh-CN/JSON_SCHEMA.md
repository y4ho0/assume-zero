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

`run_id` 是规范的 26 字符 ULID，同时也是可移植的单一路径组件。仓库和工作区指纹是不透明、与版本相关的完整性证据，只用于同一次运行的前后比较；不要跨 AssumeZero 版本比较。`git_status_before` 和 `git_status_after` 可能包含旧版 v1 报告的 porcelain 文本，或加固构建生成的 `sha256:` 摘要；新报告只持久化摘要，避免仓库路径名称重新进入证据。

执行 `explain` 或重新生成报告之前，AssumeZero 会再次校验已保存的 v1 结构。不受支持的版本、无效必填值以及所有固定结构对象中的未知字段（包括嵌套的运行、场景和发现结构）都会被拒绝。由于已保存报告不受信任且可能包含秘密，错误不会回显解析器详情或违规值。

环境变量值不会出现在 Schema 的任何字段中。已识别或已配置的敏感 CLI 选项值会在构造 `command` 字段之前被替换。`restored_names` 只包含变量名或脱敏、规范化后的 `PATH` 条目。捕获输出及适用的 Oracle 详情会在构造 Report 前完成有界处理和脱敏；捕获输出带有 `output_truncated` 标志。

使用方必须：

- 拒绝不支持的 `schema_version`；
- 把未知枚举值视为兼容性信号；
- 区分场景状态与发现的证据等级；
- 不把 `SUSPECTED` 当作已证明；
- 不假设 1-minimal 集合是全局最小集合。

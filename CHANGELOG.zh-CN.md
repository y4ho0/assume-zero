# 变更日志

[English](CHANGELOG.md)

所有重要变更都会记录在这里。格式遵循 Keep a Changelog，版本遵循语义化版本。

## [Unreleased]

### 新增

- 完整的简体中文文档、双向导航，以及自动化覆盖范围/链接检查。

## [0.1.0] - 2026-07-27

### 新增

- 严格、带版本的 TOML 配置和机器可读 Schema。
- 稳定基线门禁及可选的严格输出比较。
- 七个 Quick 场景和三个 Deep 场景，并支持按平台能力明确跳过。
- 通过经过验证的 1-minimal `ddmin` 恢复环境变量和有序 `PATH` 条目。
- 带大小和符号链接保护的 working-tree 与 git-clean 工作区副本。
- 直接进程执行、超时、中断、有界输出和确定性 Oracle。
- 内存中的秘密/路径脱敏，以及 JSON、Markdown、终端和 JUnit 报告。
- `init`、`doctor`、`check`、`list-scenarios`、`explain` 和 `report` 命令。
- 单元、集成、端到端、完整性、隐私、超时和夹具测试。

[Unreleased]: https://github.com/y4ho0/assume-zero/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/y4ho0/assume-zero/releases/tag/v0.1.0

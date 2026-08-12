# 变更日志

[English](CHANGELOG.md)

所有重要变更都会记录在这里。格式遵循 Keep a Changelog，版本遵循语义化版本。

## [Unreleased]

### 新增

- 完整的简体中文文档、双向导航，以及自动化覆盖范围/链接检查。

### 安全

- 拒绝经中间符号链接、不安全工作区名称、目标祖先或文件 Oracle 符号链接逃逸的工作区路径。
- 拒绝报告运行 ID 穿越及报告根目录/输出文件符号链接逃逸，在发布前暂存报告文件，把所有生成产物限制为 64 MiB，并在读取 JSON 报告时应用相同上限。
- 在修改目标前预检工作区字节和条目预算，在 Unix 上保真处理 Git 路径字节，并以流式方式计算源码指纹。
- 在源码指纹中对平台原始路径字节进行分帧，并以 SHA-256 摘要持久化 Git 状态；完整性哈希与版本相关，不是跨版本标识符。
- 在显示和持久化证据中一致脱敏任意非空长度的敏感环境值、`HOME`/`USERPROFILE` 路径，以及已识别或已配置的 CLI 选项值，包括 `-pVALUE` 这类已配置短选项紧贴形式。
- 配置 v1 要求 `report.redact_home = true`，防止意外关闭文档承诺的主目录路径隐私边界。
- 渲染前重新校验已保存 report-v1 的结构，在不回显解析器细节的情况下拒绝未知字段，并在重新生成时对旧报告元数据再次应用由命令值驱动的脱敏。
- 当只存在于 CLI 的值无法从证据中安全脱敏时，拒绝单字符串不透明 Shell 脚本，并在重新生成已保存的 Shell-wrapper 报告时 fail-closed。
- 使用有界多模式字面量脱敏，并在终端、Markdown 和有效 XML 证据中把控制字符显示为可见形式。
- 将 `deep_path_length` 限制为 240 字节，使生成的工作区名称保持为单一可移植组件；超过该限制的既有配置现在会校验失败。

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

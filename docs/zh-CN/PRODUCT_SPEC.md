# 产品规格

[English](../PRODUCT_SPEC.md) | [中文文档索引](README.md)

## 产品

AssumeZero 测试项目对运行机器做了哪些假设。v0.1.0 接受会在配置超时内退出的有限命令：测试、构建、类型检查、Lint、打包、代码生成、迁移检查和一次性脚本。

证据工作流为：

```text
稳定基线
→ 受控环境变化
→ 重复确认
→ 反事实恢复
→ 支持的 1-minimization
→ 脱敏证据报告
```

工具不声称能发现所有隐藏依赖，也不声称能找出唯一根因。

## 命令

```text
assumezero init [--force] [--path <path>]
assumezero doctor [-- <command>]
assumezero check [--profile quick|deep] [--dry-run] [--strict-output] [--shell] -- <command>
assumezero list-scenarios
assumezero explain <run-id>
assumezero report <run-id> --format markdown|json|junit
```

全局选项为 `--verbose`、`--quiet`、`--no-color`、`--json` 和 `--config <path>`。

## 基线

默认运行两次，每次使用一个新的项目副本，并保持命令、源状态、配置和最初继承的环境一致。开始归因之前，每次运行都必须满足 Oracle。接受/拒绝混合的结果会产生 `BASELINE_UNSTABLE`；持续拒绝会产生 `BASELINE_FAILED`。`--strict-output` 还要求脱敏后的 stdout/stderr 摘要和退出码完全相同。

准备命令会在每个副本内单独执行。准备命令失败属于基础设施失败，不是场景证据。

## 确定性 Oracle

v0.1.0 的 Oracle 支持：

- 接受的退出码；
- stdout 必须包含的子串；
- stderr 不得包含的子串；
- stdout 必须匹配的正则表达式，并在执行前校验；
- 相对于工作区副本的必需文件；
- 相对于工作区副本的禁止文件。

绝对路径和包含父目录穿越的 Oracle 文件路径会被拒绝。每次运行都会记录真实退出码、超时/中断状态、截断状态和每个 Oracle 检查。

## 工作区

`working-tree` 会复制当前工作树，但排除配置指定的路径。默认排除 `.git` 和 `.assumezero`。普通文件按字节复制，不会通过可写链接共享。每次基线、场景和恢复运行都使用新副本。

`git-clean` 使用 Git 的已跟踪文件列表，再加上 `workspace.include_untracked` 中显式指定的相对路径。默认不复制 `.git`、被忽略的依赖或构建输出，通常需要配置准备命令。

默认最大复制大小为 2 GiB。除非用户显式接受风险，否则拒绝外部符号链接。

## 最小化

当 `CLEAN_ENV` 重复失败且完整恢复重复成功时，候选项是原始环境中非必需的变量名。`ddmin` 搜索恢复集合。变量值留在内存中，输出在持久化前脱敏，报告只记录名称。

当 `MINIMAL_PATH` 重复失败且完整恢复重复成功时，候选项是原始 `PATH` 中经过规范化、去重且保持顺序的非必需条目。顶层命令会在改变 `PATH` 之前解析。主目录路径显示为 `<HOME>`，系统路径可以显示为 `<SYSTEM>`。

预算允许时，结果会被显式验证为 1-minimal。预算耗尽时，当前最优集合返回为 `SUSPECTED`，而不是 `PROVEN`。

## 报告和退出码

JSON 报告 Schema v1 始终持久化。Markdown 和 JUnit XML 是可选配置格式。JUnit 会把场景映射为测试用例。

| 代码 | 含义 |
|---:|---|
| 0 | 已执行场景中没有确认/证明的发现 |
| 1 | 至少有一个确认/证明的发现 |
| 2 | 基线失败或不稳定 |
| 3 | 配置、参数或命令错误 |
| 4 | AssumeZero 内部错误 |
| 5 | 用户中断 |

单个不支持的场景不会导致检查失败。

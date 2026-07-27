# 已验证夹具演示

[English](../../demo/README.md) | [中文文档索引](../README.md)

以下摘录由 Rust 辅助夹具于 2026-07-27 在 macOS/aarch64 上生成。路径已经规范化，夹具秘密值已省略。每个被测命令都在一次性副本中运行。

## 隐藏环境变量

```text
Baseline:
  STABLE — 2/2 accepted

AZ-F001
  Scenario: AZ-S003
  Evidence: PROVEN

  Restoring only ASSUMEZERO_DEMO_TOKEN made the command pass repeatedly.
  Values stayed in memory and were not persisted.
  The set is 1-minimal, not guaranteed globally minimum.

Source workspace unchanged: yes
Exit code: 1
```

证据链：

```text
基线接受 2/2
→ CLEAN_ENV 拒绝 2/2
→ 完整环境恢复后重复接受
→ ddmin
→ 仅恢复 ASSUMEZERO_DEMO_TOKEN 后重复接受
→ 显式 1-minimal 验证
```

运行结束后，在 `.assumezero` 中搜索了明确无效的夹具值，结果不存在。

## `PATH` 中的隐藏子工具

```text
Baseline:
  STABLE — 2/2 accepted

AZ-F001
  Scenario: AZ-S004
  Evidence: PROVEN

  Restoring only <DEMO_BIN> made the command pass repeatedly.
  The ordered entry set is 1-minimal, not guaranteed globally minimum.

Source workspace unchanged: yes
Exit code: 1
```

顶层夹具可执行文件在改变 `PATH` 前已经解析。它调用 `az-demo-child`，而该子工具只存在于 `<DEMO_BIN>`。

## 空主目录

```text
Baseline:
  STABLE — 2/2 accepted

AZ-F001
  Scenario: AZ-S001
  Evidence: CONFIRMED

  The scenario failure repeated, but v0.1.0 does not trace a more
  specific underlying file or condition for this scenario.

Source workspace unchanged: yes
Exit code: 1
```

基线使用一个包含夹具配置的临时伪主目录。`EMPTY_HOME` 把适用变量重定向到新建空目录。过程中没有列出、读取或复制真实主目录内容。

## 工作区路径包含空格

```text
Baseline:
  STABLE — 2/2 accepted

AZ-F001
  Scenario: AZ-S005
  Evidence: CONFIRMED

  The command has a repeatable dependency exposed by SPACE_WORKDIR.

Source workspace unchanged: yes
Exit code: 1
```

夹具有意拒绝包含空格的当前目录；该失败在独立副本中重复出现。

## 不稳定基线

```text
Baseline:
  BASELINE_UNSTABLE — 1/2 accepted

Scenarios:
  0 executed

Source workspace unchanged: yes
Exit code: 2
```

确定性不稳定夹具通过源项目之外的状态交替返回退出码 `0` 和 `1`。AssumeZero 在环境归因前停止。

## 机器可读证据

[evidence-summary.json](../../demo/evidence-summary.json) 是精简证据索引；[report-v1.example.json](../../demo/report-v1.example.json) 是结构完整、路径已规范化的环境变量演示报告。CI 使用 JSON Schema 2020-12 对后者进行校验。

# AssumeZero

[English](README.md) | 简体中文

[![CI](https://github.com/y4ho0/assume-zero/actions/workflows/ci.yml/badge.svg)](https://github.com/y4ho0/assume-zero/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/y4ho0/assume-zero)](https://github.com/y4ho0/assume-zero/releases/tag/v0.1.0)

> 测试你的项目对运行机器做了哪些假设。

AssumeZero 把开发环境视为有限的测试输入。它会在新的项目副本中运行一个通常成功的命令，逐个改变支持的环境条件，重复确认失败；在支持的场景中，还会恢复并找出环境变量名或 `PATH` 条目的 1-minimal（单项不可再删）集合。

```console
$ assumezero check -- cargo test
Final command: cargo test
Configuration source: built-in defaults

AssumeZero completed.

Baseline:
  STABLE — 2/2 accepted

Scenarios:
  7 passed
  0 failed
  0 skipped
  0 inconclusive/infrastructure

Secret values persisted: no environment values are report fields
Source workspace unchanged: yes
```

## 它解决什么问题

测试、构建、Lint、打包或代码生成命令可能只在某台机器上成功，因为仓库没有声明某些状态：用户级配置、已有缓存、环境变量、全局安装的子工具、特定路径形式或临时目录。AssumeZero 针对这些条件收集受控的反事实证据。

它不是 `.env` 校验器、依赖版本检查器、开发容器管理器、静态分析器、秘密扫描器、系统调用模糊测试器或可复现构建产物比较器。

## 安装

AssumeZero v0.1.0 需要 Rust 1.80 或更新版本。推荐按正式标签安装：

```bash
cargo install \
  --git https://github.com/y4ho0/assume-zero \
  --tag v0.1.0 \
  --locked
```

也可以使用完全不可变的提交引用：

```bash
cargo install \
  --git https://github.com/y4ho0/assume-zero \
  --rev 1c06b02a42f0b494e2f31bd24af44d14731c712b \
  --locked
```

[v0.1.0 Release](https://github.com/y4ho0/assume-zero/releases/tag/v0.1.0) 提供 Windows x86_64、Linux x86_64、macOS x86_64 和 macOS arm64 的预编译压缩包。请使用随附的 `SHA256SUMS` 校验下载文件。

v0.1.0 没有发布到 crates.io。

## 30 秒上手

运行一个会自行结束的有限命令：

```bash
cd your-project
assumezero doctor -- npm test
assumezero check -- npm test
```

其他示例：

```bash
assumezero check -- pytest
assumezero check -- mvn test
assumezero check -- cargo test
assumezero check --profile deep -- cargo test
```

AssumeZero 总会打印最终选中的命令。`--` 后的参数优先于配置文件中的 `[run].command`。

## 场景

| ID | 名称 | Quick | 改变的条件 |
|---|---|:---:|---|
| AZ-S001 | `EMPTY_HOME` | ✓ | 把适用的主目录、配置和数据变量重定向到空目录 |
| AZ-S002 | `EMPTY_CACHE` | ✓ | 把支持的缓存变量重定向到空目录 |
| AZ-S003 | `CLEAN_ENV` | ✓ | 只保留平台必需变量和配置的允许列表 |
| AZ-S004 | `MINIMAL_PATH` | deep | 保留顶层命令目录、系统必需目录和显式保留条目 |
| AZ-S005 | `SPACE_WORKDIR` | ✓ | 使用包含多个空格的副本路径 |
| AZ-S006 | `UNICODE_WORKDIR` | ✓ | 使用包含 Unicode 的副本路径 |
| AZ-S007 | `DEEP_WORKDIR` | ✓ | 使用安全、有界的深层路径 |
| AZ-S008 | `REDIRECTED_TEMP` | ✓ | 重定向 `TMP`、`TEMP` 和 `TMPDIR` |
| AZ-S009 | `TIMEZONE_UTC` | deep | 在支持的平台上设置进程级 `TZ=UTC`；best effort |
| AZ-S010 | `LOCALE_C` | deep | 在 C/POSIX Locale 可用时设置 `LANG=C` 和 `LC_ALL=C` |

状态包括 `PASS`、`FAIL`、`SKIPPED_UNSUPPORTED`、`INCONCLUSIVE` 和 `INFRASTRUCTURE_ERROR`。平台能力不支持而跳过的场景不会让整个检查失败。

## 证据等级

- `PROVEN`：基线稳定、改变条件后的失败重复出现、恢复后重复成功、1-minimization 完成，并且没有基础设施错误。
- `CONFIRMED`：基线稳定且改变条件后的失败重复出现，但尚未确定更具体的可恢复变量或路径。
- `SUSPECTED`：证据有用，但不完整或受到预算限制。
- `INCONCLUSIVE`：无法进行可靠归因。
- `SKIPPED`：平台不支持或被显式排除。

“1-minimal”表示从报告集合中删除任意一个项目都会破坏已观察到的恢复；它不代表数学上的唯一解释或全局最小解释。

## 配置

在不覆盖已有文件的情况下创建 `assumezero.toml`：

```bash
assumezero init
```

只有在确实要替换时才使用 `assumezero init --force`。精简示例：

```toml
version = 1

[run]
command = ["cargo", "test"]
prepare = []
timeout_seconds = 300
baseline_runs = 2
confirm_failures = 2

[workspace]
mode = "working-tree"
max_size_mib = 2048
exclude = [".git", ".assumezero"]
include_untracked = []

[oracle]
kind = "exit-code"
accepted_exit_codes = [0]

[scenarios]
profile = "quick"

[budget]
max_total_runs = 40
max_total_seconds = 1800

[report]
formats = ["terminal", "json", "markdown"]
```

未知字段会导致校验失败。输出文本、正则表达式、必需文件和禁止文件等 Oracle 条件见[产品规格](docs/zh-CN/PRODUCT_SPEC.md)。机器可读配置格式见 [config-v1.schema.json](schemas/config-v1.schema.json)；字段名保持英文，作为稳定接口的一部分。

## 报告

每次检查都会保存脱敏 JSON 证据：

```text
.assumezero/runs/<run-id>/report.json
```

配置启用的 Markdown 和 JUnit XML 报告会保存在同一目录。可以重新生成格式：

```bash
assumezero report <run-id> --format markdown
assumezero report <run-id> --format json
assumezero report <run-id> --format junit
assumezero explain <run-id>
```

退出码稳定：`0` 表示没有确认的发现；`1` 表示至少一个 `CONFIRMED`/`PROVEN` 发现；`2` 表示基线失败或不稳定；`3` 表示命令或配置错误；`4` 表示内部错误；`5` 表示用户中断。除非使用 `--suspected-is-failure`，否则 `SUSPECTED` 不会产生退出码 1。

隐藏环境变量、隐藏子工具、空主目录状态、含空格路径和不稳定基线的已验证夹具记录见[中文演示说明](docs/zh-CN/demo/README.md)。

## 隐私

AssumeZero 本身不会上传文件、调用外部 API、发送遥测、检查真实主目录的内容或持久化环境变量值。敏感环境值只在内存中用于输出脱敏，随后才写入报告。报告只包含名称、存在性/分类元数据和脱敏后的输出摘要。

用户提供的准备命令和被测命令仍可访问当前用户可访问的网络及其他资源。脱敏是纵深防御，模式匹配可能出现误报或漏报。

## 安全边界

> AssumeZero 使用当前用户权限，在项目副本中执行用户提供的命令。它用于保护源工作区，不是运行不受信任代码的安全沙箱。

被测命令不会以源项目作为工作目录，但仍保留当前用户的操作系统权限，因此可以故意访问副本之外的资源。不要用 AssumeZero 运行不受信任代码。除非显式选择 `--shell`，否则禁用 Shell 解析；启用时会显示警告。

默认拒绝指向外部的符号链接，并且不会读取目标。超时或中断后的进程树终止是 best effort，无法在所有平台上保证。

## 平台支持

已在 Windows、macOS 和 Linux 上测试。CI 矩阵会在每个平台运行格式检查、Clippy、完整测试和 Release 构建。当前证据见顶部 CI 徽章；平台能力差异见[已知限制](docs/zh-CN/LIMITATIONS.md)。

正式 Release 提供 Windows x86_64、Linux x86_64、macOS x86_64 和 macOS arm64 的原生构建，并在对应 GitHub Hosted Runner 上执行 `--version` 和 `--help` 冒烟测试。

## 已知限制

v0.1.0 只支持有限、非交互式命令。它不提供操作系统虚拟化、网络或数据库故障注入、系统调用/文件访问跟踪、自动源码修复、云账号、遥测、包注册表发布或穷举场景组合。`EMPTY_HOME` 可以确认条件，但不会找出具体 HOME 文件；时区修改是进程级 best effort。

完整列表见[已知限制](docs/zh-CN/LIMITATIONS.md)。

## 路线图

下一批候选方向包括：保持证据语义的两两场景约简、更强的跨平台后代进程控制、具有明确隐私边界的可选文件访问证据、缓存式 Copy-on-Write 加速和新增确定性 Oracle。详见[路线图](docs/zh-CN/ROADMAP.md)。

## 中文文档

- [文档索引](docs/zh-CN/README.md)
- [产品规格](docs/zh-CN/PRODUCT_SPEC.md)
- [架构](docs/zh-CN/ARCHITECTURE.md)
- [场景](docs/zh-CN/SCENARIOS.md)
- [安全与隐私模型](docs/zh-CN/SECURITY_MODEL.md)
- [JSON Schema](docs/zh-CN/JSON_SCHEMA.md)
- [CI 使用](docs/zh-CN/CI_USAGE.md)
- [市场定位](docs/zh-CN/MARKET_POSITION.md)
- [贡献指南](CONTRIBUTING.zh-CN.md)
- [安全政策](SECURITY.zh-CN.md)

## 参与贡献

请阅读[贡献指南](CONTRIBUTING.zh-CN.md)、[安全模型](docs/zh-CN/SECURITY_MODEL.md)和[行为准则](CODE_OF_CONDUCT.zh-CN.md)。Bug 报告应包含脱敏证据、平台信息和准确的 AssumeZero 版本，绝不要包含秘密。

## 许可证

[MIT](LICENSE)。英文许可证文本是规范性法律文件。

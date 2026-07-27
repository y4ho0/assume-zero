# 贡献指南

[English](CONTRIBUTING.md)

感谢你帮助改进 AssumeZero。

## 修改之前

涉及重要行为或 Schema 变更时，请先创建 Issue。必须保持稳定的场景 ID、退出码、隐私规则和报告兼容性。新增声明需要自动化证据支持。

## 开发

需要 Rust 1.80 或更新版本。

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --locked --release
```

测试必须使用显然无效的夹具凭据和临时伪造的主目录/缓存。绝不要读取开发者真实主目录的内容。场景或恢复测试必须在工作区副本中运行被测命令，并核对源工作区完整性。

不要为了让 CI 变绿而删除有价值的跨平台失败测试。对确实不支持的能力，应明确标记为跳过并说明原因。

## Pull Request

请描述用户可见变更、安全/隐私影响、Schema 兼容性和验证命令。保持提交聚焦，不要包含 `target`、`.assumezero`、`.env`、凭据或本机绝对路径。

参与项目须遵守[行为准则](CODE_OF_CONDUCT.zh-CN.md)。

# 媒体理解首版验证记录

日期：2026-10-24。运行环境为 Windows 桌面 Tauri 调试构建，独立的应用数据、缓存、WebView2 用户目录及本机 HTTP 测试服务；原媒体只有合成 PNG 和 65 秒合成视频。

| 检查 | 结果 |
| --- | --- |
| 前端 `pnpm build` | 通过 |
| 前端全量 Node 测试 | 797 通过，0 失败 |
| `cargo test --locked ai::services -- --test-threads=1` | 15 通过，0 失败 |
| `cargo check --locked`、`cargo build --locked` | 通过；保留仓库原有的未使用函数警告 |
| 新增 Rust 模块 `rustfmt --check` | 通过 |
| `git diff --check` | 通过 |
| 全仓库 `cargo fmt --check` | 未通过；原有 `build.rs` 等文件存在格式差异，本轮未全量重排 |

桌面验收通过实际 Tauri command、真实 SDK HTTP 请求、FFmpeg、系统凭据库和正式图库表执行。包括向量草稿检测不留下服务、无需聊天接口、检测维度和默认绑定、拒绝伪造能力回执、单图和视频片段分析、索引和召回、带引用问答、资料显式采用及并发人工修改保护、向量阶段重试复用分析、暂停继续、更换 Embedding 隔离旧空间、排队查询撤销许可、拒绝并发任务不产生空任务、跨图库拒绝，以及相邻视频片段的问答范围。

600×620 的真实设置窗口验证了顶部 Embedding 入口、表单滚动、检测及保存按钮可见，并实际从表单检测和保存默认 Embedding。

- [机器可读结果](media-ai-runtime.json)
- [实际设置窗口](media-ai-settings.png)
- [实际 Embedding 配置表单](media-ai-embedding.png)

测试 API 返回固定分析与向量，用于验证协议和业务状态流转；这不是模型质量或真实 GPU 推理验收。Mage／Qwen 的安装、显存、驱动、中文理解、检索相关性和视频内容覆盖仍需在目标模型服务器测试。测试服务、测试凭据和临时图库在验证结束后清理。

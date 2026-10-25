# LLM 服务共享文本 Embedding 验证

日期：2026-10-25。环境：Windows x64。测试使用合成数据、回环 HTTP 和临时 SQLite，未读取真实密钥或请求供应商账号。

## 实现与契约

- LLM 服务新增 `openaiEmbeddings`，旧配置缺省为 `false`。开启后可手动添加独立的文本嵌入模型，复用原服务的地址、凭据和传输许可。
- 嵌入模型的能力集合严格为 `embed_text`；生成与嵌入用途不能混入同一模型记录。共享路径限定 OpenAI 标准文本协议，原画面向量仍使用专用多模态服务。
- 开关关闭后保留模型、检测回执、维度和索引，解除该服务的默认文本向量绑定，并在候选、检测、调用和任务权限检查中停用嵌入能力。开关不影响生成用途的合法回执。
- Embedding 分类展示同一个服务及其嵌入模型；修改连接影响同服务的生成模型。该视图保留单模型移除，整个共享服务在 LLM 分类中移除。
- 新共享服务的任务快照不会被当成旧版混合服务迁移；共享身份的其他专用服务也不能绕过当前服务的开关撤销。

OpenAI 官方 [Models 列表](https://developers.openai.com/api/reference/resources/models/methods/list)没有承诺排除嵌入模型，标准返回字段没有能力分类；[Embeddings 接口](https://developers.openai.com/api/reference/resources/embeddings/methods/create)还引用该列表查看可用模型。因此，列表只用于发现，手动添加不依赖列表，能力必须由实际 `/embeddings` 检测确认。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| 共享服务与模型选择的定向前端测试 | 36 通过 |
| 前端全量 Node 测试 | 831 通过 |
| 前端生产构建 | 通过 |
| Rust 媒体服务测试 | 62 通过，FFmpeg 集成项默认忽略 |
| 显式图片／视频集成项 | 1 通过，内部覆盖 vLLM 与三家原生协议 |
| `cargo check --locked --message-format=short` | 通过 |
| 修改模块 `rustfmt --check`、`git diff --check` | 通过 |

HTTP 回归在专用／共享两种配置下使用同一地址和合成 Key，逐请求检查 Bearer，执行生成分析文本、正文嵌入、SQLite 保存及重开、查询嵌入、召回和带引用回答。另覆盖正文／查询批次乱序、模型类别、默认角色、关闭后保存与禁止调用、旧快照迁移及同身份服务的双向撤销隔离。

媒体集成保留真实 JPEG 和 65 秒长 GOP 视频、尾部蓝帧及原文件哈希断言。采样在回环服务启动前完成，避免本地 FFmpeg 耗时触发模拟接口的 10 秒请求等待期限；生产调用逻辑没有因此放宽。

全仓 `cargo fmt --check` 仍有既存的 `src-tauri/build.rs` 等格式差异，未进行无关格式化。测试链接保留既有 LIBCMT 警告；Cargo 检查保留既有未使用函数警告。

## 复现与边界

前端在 `src-vite` 执行：

```powershell
node --experimental-strip-types --test tests/mediaAi.test.ts tests/mediaAiSettings.test.ts
node --experimental-strip-types --test tests/*.test.ts tests/*.test.mjs
pnpm build
```

Rust 在 `src-tauri` 执行：

```powershell
cargo test --locked ai::services:: -- --nocapture
cargo test --locked ai::services::workflow::tests::multimodal_real_media_roundtrip -- --ignored --nocapture
cargo check --locked --message-format=short
rustfmt --edition 2024 --check src/ai/services/mod.rs
```

回环向量仅证明调用、存储及检索链路，不代表真实模型的语义召回效果。实际网关必须提供 `/embeddings` 路由及可用的嵌入模型；仅开启兼容标记不会新增服务端能力。真实账号权限／额度、模型效果及桌面点击尚未验收。操作步骤见 [Embedding 服务指南](../../embedding-services.md)。

# 国内 Embedding 服务接入验证

日期：2026-10-25。环境：Windows x64。官方协议核对使用公开文档；业务测试使用合成媒体、本机回环 API 和临时 SQLite，不读取真实密钥、不调用用户模型服务、不修改用户原媒体。

## 协议核对

| 服务 | 文本与多模态协议 | 本次处理 |
| --- | --- | --- |
| 百炼 | 文本可使用 OpenAI 兼容接口；原生多模态使用 `input.contents`，返回 `output.embeddings[].embedding` | 原生启用 `enable_fusion`，要求恰好一个稠密结果；融合结果的 `index` 不是独立输入排序依据，不强加 OpenAI 的 `index=0` 约定 |
| 方舟 | 原生 `/api/v3/embeddings/multimodal`，有序 `input`，返回 `data.embedding` | 文本和媒体使用同一模型接口，分别编码语料和查询指令，保留 `/api/v3` 基础路径 |
| 硅基 | `/v1/embeddings`；文本为字符串，原生多模态支持图片对象及列表，返回 `data[].embedding` | 图片批次的全部结果必须数量匹配、index 唯一且完整、维度有效；不会只保留第一条 |

方舟的 `Target_modality` 取决于被搜索的语料。分析文本索引使用 `text`；原画面索引包含纯图片及“时间文字＋多帧”的组合语料，使用 `image/text and image`。自定义字段只接受 `Instruction` 正文，完整模板在配置校验阶段拒绝。多模态能力检测同时核实文本正文、文本查询、原画面查询及三帧媒体的维度，之后才授予对应检测回执。

视频仍由 Lap 按最多 60 秒的片段提取三帧。百炼和方舟融合时间文字与图像；硅基逐帧编码后由 Lap 做 L2 归一化、均值及再次归一化。硅基的帧均值是本地检索策略，不是供应商的视频接口或时序事件理解。不同协议、编码规则及两种索引用途具有独立 profile；已有 OpenAI／vLLM 的合法旧回执及 profile 保持兼容。

官方来源均已读取正文：

- [百炼多模态 Embedding](https://help.aliyun.com/zh/model-studio/multimodal-embedding-api-reference)：原生请求、融合开关和输出结构。普通样例的 `index=0` 不用于推断融合结果元数据。
- [百炼文本 Embedding](https://www.alibabacloud.com/help/en/model-studio/text-embedding-synchronous-api)：OpenAI 兼容文本接口。
- [方舟向量化指南](https://docs.volcengine.com/docs/ark/vectorization?lang=zh)与[多模态接口](https://docs.volcengine.com/docs/ark/multimodal-vectorization-api?lang=zh)：独立语料用 `/`、组合语料用 `and`，以及原生请求／响应。
- [硅基 Embeddings](https://api-docs.siliconflow.cn/docs/api/embeddings-post)：VL 输入对象／列表及 `encoding_format=float`；当前公开接口不支持视频内容。三帧批次完整性由客户端检测，尚无真实账号回执。

## 场景覆盖

- 设置新增五个供应商预设，均只创建服务和可编辑示例模型，不自动请求、开启传输或更换默认用途。真实 Vue 脚本与模板回归覆盖预设、协议切换、能力候选、模型发现限制及查询指令展示。
- 三家分别执行 LLM 看图产出标题／说明／标签、原生文本 Embedding、SQLite 保存及重开、文字查询召回、LLM 引用证据回答，以及独立原图向量检索。请求中实际包含本地预处理后的 JPEG；原文件哈希不变。
- 显式 FFmpeg 集成项覆盖 vLLM 与三家原生协议。测试生成 JPEG 和 65 秒、1 fps、长 GOP 视频，前 60 秒红色、后 5 秒蓝色；实际发送采样帧、保存片段向量、编码文字查询，确认 60–65 秒片段可被检索。
- 集成项解码实际出站的尾部 JPEG 并确认蓝色，防止回退采样误取第 0 秒红色关键帧；同时检查原图与视频的原文件哈希。
- 回归包括单／多结果、完整 index、维度不匹配、零向量、非有限值、帧均值抵消、超限输入、远程图片 URL 拒绝、认证／路由及服务端错误正文脱敏。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| 前端全量 Node 测试 | 826 通过 |
| 最后 Ark 查询提示调整后的定向前端测试 | 31 通过 |
| 最新前端生产构建 | 通过 |
| Rust 媒体服务测试 | 58 通过；FFmpeg 集成项默认忽略 |
| 显式图片／视频集成项 | 1 通过，内部遍历 vLLM 及三家原生协议 |
| `cargo check --locked --message-format=short` | 通过 |
| 修改的媒体服务模块 `rustfmt --check`、`git diff --check` | 通过 |

全仓 `cargo fmt --check` 仍报告既存的 `src-tauri/build.rs` 等格式差异，未进行无关格式化；测试链接保留既有 LIBCMT 警告，Cargo 检查保留既有未使用函数警告。

## 复现与边界

前端在 `src-vite` 执行：

```powershell
node --experimental-strip-types --test tests/*.test.ts tests/*.test.mjs
pnpm build
```

Rust 在 `src-tauri` 执行，第二条需要 FFmpeg／FFprobe：

```powershell
cargo test --locked ai::services:: -- --nocapture
cargo test --locked ai::services::workflow::tests::multimodal_real_media_roundtrip -- --ignored --nocapture
cargo check --locked --message-format=short
rustfmt --edition 2024 --check src/ai/services/mod.rs
```

回环服务返回固定向量，用于验证接口、预处理、存储和调用链路。真实供应商账号的权限、地域、额度、模型可用性、实际推理与中文语义召回质量，以及桌面界面点击尚未验收。使用入口和参数见 [Embedding 服务指南](../../embedding-services.md)。

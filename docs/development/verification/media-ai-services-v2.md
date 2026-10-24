# LLM 与 Embedding 服务拆分验证

日期：2026-10-24。环境：Windows x64，使用合成媒体、本机回环 API 和临时 SQLite；未访问用户模型服务、读取真实密钥或修改用户原媒体。

## 结果

| 检查 | 结果 |
| --- | --- |
| 前端全量 Node 测试 | 815 通过 |
| 最后查询预设调整后的前端定向测试 | 20 通过 |
| 前端生产构建 | 通过 |
| Rust 媒体服务测试 | 35 通过；FFmpeg 集成项默认忽略 |
| 显式执行真实图片／视频集成项 | 1 通过，0 忽略 |
| `cargo check --locked --message-format=short` | 通过 |
| 部署脚本测试 | 20 通过，包含真实 FFmpeg 视频采样 |
| 新多模态 profile 的部署 dry-run | 通过 |
| 修改的媒体服务模块 `rustfmt --check`、`git diff --check` | 通过 |

全仓 `cargo fmt --check` 仍有既存格式差异，例如 `src-tauri/build.rs:526`；未进行无关的全仓格式化。链接及 Cargo 检查保留既有的 LIBCMT／未使用函数警告。

## 验证范围

- 设置保持原独立窗口，在“AI 模型 → 媒体理解”内按 LLM／Embedding 分类。真实 Vue 组件脚本与模板测试覆盖类型隔离、模型选择／删除、四类默认用途及向导参数。
- v1 配置纯内存迁移回归覆盖混合服务拆分、稳定凭据身份、旧文本索引 profile、合法检测回执、默认角色、共享密钥引用以及 128 服务／2048 模型全部三类拆分后的容量。前端不能伪造身份或检测回执。
- 文本与原画面索引使用独立 profile 和存储；原画面向量不依赖分析记录。回归覆盖媒体版本变化、隐藏／删除、文件类型过滤、SQLite 重开、视频片段、重复提交和未知媒体时长。
- vLLM 回环测试检查 `/embeddings` 的 `messages`、system 查询指令、`add_generation_prompt: true`、浮点向量、输入帧顺序和返回 index／维度；拒绝远程图像 URL、零向量及无效结果。服务停用、撤销许可、地址／凭据／协议变化仍会阻止调用。
- 显式 FFmpeg 集成测试生成一张 JPEG 和一段 65 秒、1 fps、仅有 0 秒关键帧的视频，前 60 秒为红色、后 5 秒为蓝色。它实际运行 Lap 的图片预处理／视频采样、发送回环多模态请求、保存原画面向量、编码文字查询并检索 SQLite，验证命中 60–65 秒片段及过期图片来源排除。
- 集成测试解码实际出站的尾部图片并确认是蓝色，避免低帧率尾部回退误取红色的 0 秒关键帧；两种原文件在测试前后的哈希相同。向量由回环服务提供，用于验证传输和存储契约。

## 复现

前端在 `src-vite` 执行：

```powershell
node --experimental-strip-types --test tests/*.test.ts tests/*.test.mjs
pnpm build
```

Rust 在 `src-tauri` 执行；第二条需 FFmpeg／FFprobe：

```powershell
cargo test --locked ai::services:: -- --nocapture
cargo test --locked ai::services::workflow::tests::multimodal_real_media_roundtrip -- --ignored --nocapture
cargo check --locked --message-format=short
rustfmt --edition 2024 --check src/ai/services/mod.rs
```

部署测试在仓库根目录执行：

```powershell
python -B -m unittest discover -s scripts/ai-server -p 'test_*.py' -v
python scripts/ai-server/deploy.py serve embedding-multimodal --dry-run
```

真实 GPU 依赖安装、模型推理效果、中文语义召回质量及桌面界面点击仍待验收。固定模型和部署协议见 [部署说明](../../../scripts/ai-server/README.md)，配置和业务入口见 [媒体理解说明](../../media-understanding.md)。

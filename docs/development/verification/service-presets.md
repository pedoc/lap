# 内置停用服务目录验证

日期：2026-10-25。环境：Windows x64。使用合成数据和本机回环服务；未读取用户密钥、调用供应商账号或修改媒体文件。

## 行为

- LLM 分类直接列出百炼、硅基流动、火山方舟、DeepSeek 和 LongCat；Embedding 分类直接列出原有五组预设。移除预设下拉，保留添加自定义服务及手动添加模型。
- 内置条目初始停用、禁止传输和后台调用，不保存密钥、不绑定默认用途；示例模型没有检测回执，输出 Token 使用服务端默认值。
- 打开设置仅在编辑草稿补齐缺失项，首次显示不标记未保存，不请求、不写配置。后续保存沿用现有身份、凭据及版本检查。
- 固定 ID 存在时保留原配置，模型删掉后不重新添加。规范化基础地址、类型和协议匹配的已有服务可覆盖目录项；共享 LLM 文本嵌入连接也可覆盖对应的文本 Embedding 预设。
- 内置服务保留条目，支持停用、清 Key 和移除模型；自定义服务可删除。原有选中服务优先保留，新增停用预设不抢占配置中的默认用途。
- 手动检测可以在停用状态进行，仍要求传输许可及正确的服务／模型能力。检测只更新相应回执；必须手动启用后，业务运行检查才放行。
- 百炼工作空间占位地址可以保存，但 Vue 检测入口和 Rust 统一 HTTP 客户端都会阻止其发出请求。用户需替换为控制台中的真实基础地址。

## 官方参数核对

预设参数及来源见 [服务指南](../../embedding-services.md#llm-预设)。Model ID 是 2026-10-25 官方示例，允许修改，不代表账号已开通或已经实测。

百炼当前使用工作空间域名；已有旧域名配置不被覆盖。百炼、方舟和 DeepSeek 的预设使用手动 Model ID，避免根据 Chat Completions 前缀推断模型发现接口。硅基和 LongCat 提供发现入口；LongCat 模型列表专页与旧概述的路径存在差异，按专页配置，实际账号仍需验收。

另静态核对了锁定的 `genai 0.7.0`：`qwen3.8-max` 在其保护名单中，当前 Lap 调用链会保留该示例 ID；没有据此宣称任意模型别名都不受 SDK 后缀解析影响。

## 验证

| 检查 | 结果 |
| --- | --- |
| 预设目录及设置组件定向测试 | 42 通过 |
| 前端全量 Node 测试 | 837 通过 |
| 前端生产构建 | 通过 |
| Rust 媒体服务测试 | 65 通过，既有 FFmpeg 集成项默认忽略 |
| `cargo check --locked --message-format=short` | 通过 |
| 修改模块 `rustfmt --check`、`git diff --check` | 通过 |

前端回归覆盖十组默认条目、首次展示不保存、ID／地址协议去重、已有参数保留、删除模型后重开、内置／自定义删除差异、两类目录筛选及未填写工作空间时零请求。后端回环覆盖停用 LLM／Embedding 检测成功但运行仍拒绝、启用后可执行、撤销传输许可前置拒绝、服务 flag 和模型类别不可绕过，以及合法占位 URL 无法构造 HTTP／SDK 客户端。

本次未改媒体预处理、原画面向量存储或检索算法，没有重复执行既有 FFmpeg 集成项。全仓 `cargo fmt --check` 仍有既存的 `src-tauri/build.rs` 等格式差异，未进行无关格式化；测试链接保留既有 LIBCMT 警告，Cargo 检查保留既有未使用函数警告。

## 复现

前端在 `src-vite` 执行：

```powershell
node --experimental-strip-types --test tests/mediaAi.test.ts tests/mediaAiSettings.test.ts
node --experimental-strip-types --test tests/*.test.ts tests/*.test.mjs
pnpm build
```

Rust 在 `src-tauri` 执行：

```powershell
cargo test --locked ai::services:: -- --nocapture
cargo check --locked --message-format=short
rustfmt --edition 2024 --check src/ai/services/mod.rs
```

真实账号权限、地域／工作空间、额度、模型推理效果和桌面点击未验收。所有预设保持可编辑，模型可用性以实际接口检测为准。

# 套餐接入通道与凭据隔离验证

日期：2026-10-25。环境：Windows x64。所有请求回归使用本机模拟服务及合成 Key，未读取实际 Keyring 密钥、使用截图中的凭据或调用真实模型服务。

## 实现范围

- 方舟按量、Agent Plan、Coding Plan，以及百炼按量、Coding Plan、Token Plan 个人／团队分别配置。原按量 ID 保持不变，新增项默认停用、未授权传输、未检测、不绑定用途。
- 目录共十个 LLM 接入预设、六个 Embedding 预设，后者包含方舟 Agent Plan 原生向量。每条记录使用独立 provider ID；切换列表条目不改写另一条目的 URL 或复制 Key。
- Token Plan 个人与团队即使地址和模型相同也不按 endpoint 去重。已有同地址自定义服务也不会吞掉两个不同凭据范围的默认入口。
- 接入方式和使用范围仅作说明，不参与模型能力筛选。生成模型的文本、图片、视频采样帧和结构化输出仍可手动声明、检测；不因套餐名称硬禁某种能力。
- 所有新增共享文本 Embedding 标记默认关闭。Agent Plan 原生向量使用独立服务和 `ark_multimodal` 协议，不把原生接口误当标准文本 `/embeddings`。
- 保留原参数、模型删除、草稿补齐及自定义服务删除规则。没有新增后端配置字段、数据库迁移或生产调用分支。

## 官方核对与边界

地址、示例模型及条款来源集中在 [服务接入指南](../../embedding-services.md#接入通道凭据与模型能力)。套餐使用范围和模型能力分开描述：官方工具适用条款不等于模型缺少某种模态能力，也不能仅凭工具名单推断 Lap 一律获准或禁止。配置和检测不代表已确认账号的任意业务使用范围。

方舟 `ark-code-latest` 是套餐路由别名，不保证固定模型。Agent Plan 的原生向量接入采用以下官方静态证据链：

1. [Agent Plan 向量专页](https://ark.volcengine.com/region:cn-beijing/docs/agent-plan-personal-vectorized-models)提供 OpenViking 的 `/api/plan/v3`、`provider=volcengine` 及 `doubao-embedding-vision` 配置。
2. [官方 OpenViking 固定版本](https://github.com/volcengine/OpenViking/blob/4ab22a4923452362b60aef2826c2083ce36168d2/openviking/models/embedder/volcengine_embedders.py)原样传递 Base URL／模型，调用 `multimodal_embeddings.create` 并读取 `data.embedding`。
3. [官方 Ark SDK 固定版本](https://github.com/volcengine/volcengine-python-sdk/blob/db6f22be1e404e58bfe55a1c8153f26fbf96a510/volcenginesdkarkruntime/resources/multimodal_embeddings.py)定义 `/embeddings/multimodal`、有序 text／image 输入及 `instructions`。

上述链路支持复用已有原生适配器，但不证明真实套餐账号已接受 Lap 的全部三帧组合。该能力仍需实际检测。

硅基免费／Pro 模型、DeepSeek 充值与赠送余额、LongCat 资源包与按量属于同一普通 API 下的模型／计费差异，本次不人为增加重复套餐入口。LongCat Responses 的图像格式与现有 Chat 链路分开核对，没有因此宣称所有协议均已接入。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| 接入目录与设置组件定向测试 | 46 通过 |
| 前端全量 Node 测试 | 841 通过 |
| 前端生产构建 | 通过 |
| Rust 媒体服务测试 | 68 通过，既有 FFmpeg 集成项默认忽略 |
| `cargo check --locked --message-format=short` | 通过 |
| 修改模块格式与 diff 检查 | 通过 |

新增回归检查：

- 对五种新增 LLM 配置断言完整 `POST {prefix}chat/completions`、model 和各自 Bearer，确保 `/api/plan/v3`、`/api/coding/v3` 等前缀不会多出 `/v1`。
- 百炼 Token 个人／团队在同一回环地址连续发送请求，分别校验两把合成 Key；`qwen3.8-max` 原样出站，没有额外注入推理参数。
- 对相同 URL、相同合成凭据修订号的两个 provider 验证独立 identity 和凭据引用清理范围，不访问实际系统密钥。
- Agent Plan 原生向量精确使用 `/api/plan/v3/embeddings/multimodal`，保留模型别名和 Bearer；检查三帧输入顺序、语料压缩指令、MediaQuery 目标模态及 `data.embedding` 归一化。
- 前端检查个人／团队不会合并或复制 Key、旧 ID／配置与已删模型不被覆盖、所有普通能力仍可编辑和检测，以及 Agent Plan 向量接入方式未误标按量。

本次仅改预设、说明和回归测试，未改生产媒体预处理或向量索引算法，未重复执行既有 FFmpeg 集成项。全仓 `cargo fmt --check` 仍有既存 `src-tauri/build.rs` 等格式差异；本次修改模块格式检查通过，未进行无关格式化。测试链接保留既有 LIBCMT 警告，Cargo 检查保留既有未使用函数警告。

## 复现与未验收范围

前端在 `src-vite` 执行全量 Node 测试和 `pnpm build`；Rust 在 `src-tauri` 执行：

```powershell
cargo test --locked ai::services:: -- --nocapture
cargo check --locked --message-format=short
rustfmt --edition 2024 --check src/ai/services/mod.rs
```

真实账号权限、套餐额度与适用范围、推理效果及桌面点击尚未验收。没有模拟其他客户端身份，也没有通过改 URL 或共享 Key 绕过服务端权限。

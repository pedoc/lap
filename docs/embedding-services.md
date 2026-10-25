# 模型服务接入与 Embedding 使用指南

核对日期：2026-10-25。Lap 的 Rust 后端随桌面程序启动；`scripts/ai-server` 用于在另一台机器部署模型 API。也可以直接使用在线 API，二者使用相同的服务／模型配置流程。

## 服务列表中的内置预设

在 **设置 → AI 模型 → 媒体理解**切换 **LLM 服务**或 **Embedding 服务**，即可直接选择列表中的内置服务。预设默认停用，未保存密钥、未授权传输、未检测，也不绑定默认用途。打开设置只补齐当前草稿中的缺失预设，不发送请求；填写配置、检测或保存时才按原有流程持久化。

填写 API Key，按需调整基础地址与 Model ID，并允许传输后，可以保持服务停用进行检测。检测成功不会自动启用服务。启用并选择顶部默认用途、保存后，业务才可调用；批量任务另需允许后台处理。

内置服务保留固定入口，可以停用、清除密钥或移除模型；自定义服务仍可删除。重新打开设置不会覆盖已保存的内置参数，也不会补回已移除的模型。一般配置按类型、基础地址和协议去重；同地址但凭据范围不同的 Token Plan 个人／团队入口始终独立。

### LLM 预设

五家均采用 OpenAI 兼容的 Chat Completions 调用。下表 Model ID 是核对日官方文档中的可编辑示例，不代表账号已经开通，也不自动授予检测回执。

| 服务 | API 基础地址 | 示例 Model ID |
| --- | --- | --- |
| 阿里百炼 · 按量 API | `https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/compatible-mode/v1` | `qwen3.8-max` |
| 阿里百炼 · Coding Plan | `https://coding.dashscope.aliyuncs.com/v1` | `qwen3.7-plus` |
| 阿里百炼 · Token Plan 个人 | `https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1` | `qwen3.8-max` |
| 阿里百炼 · Token Plan 团队 | 同个人版地址，使用团队凭据 | `qwen3.8-max` |
| 硅基流动 | `https://api.siliconflow.cn/v1` | `deepseek-ai/DeepSeek-V4-Flash` |
| 火山方舟 · 按量 API | `https://ark.cn-beijing.volces.com/api/v3` | `doubao-seed-2-1-pro-260915` |
| 火山方舟 · Agent Plan | `https://ark.cn-beijing.volces.com/api/plan/v3` | `ark-code-latest` |
| 火山方舟 · Coding Plan | `https://ark.cn-beijing.volces.com/api/coding/v3` | `ark-code-latest` |
| DeepSeek | `https://api.deepseek.com/v1` | `deepseek-flash` |
| LongCat | `https://api.longcat.chat/openai/v1` | `LongCat-2.0` |

百炼内置地址使用可解析的 `your-workspace-id` 占位主机，需将其替换为自己的 Workspace ID，或直接填写控制台提供的完整基础地址。占位地址可以随停用预设保存，检测前会提示填写。旧 `dashscope.aliyuncs.com` 域名用于存量兼容，官方说明 2026-09-30 后不再增加新特性；已有配置保留，新模型优先使用工作空间地址。

聊天协议兼容不等于模型列表接口一致。百炼模型列表使用独立的原生路由；方舟与 DeepSeek 的预设也保留手动 Model ID 入口。硅基支持同前缀的模型列表；LongCat 按当前模型列表专页使用 `/openai/v1/models`，其旧概述存在另一条路径说明，实际账号仍需验证。所有服务均可手动添加模型，模型能力仍需检测。

### 接入通道、凭据与模型能力

各版本直接列为独立停用服务，分别保存地址、密钥和模型。选择另一条目不会修改原条目的 URL、复制 API Key 或迁移默认用途。百炼 Token Plan 个人与团队虽然使用同一地址，但官方明确凭据不可互用；不能仅按相同 URL 合并。方舟 Agent Plan 也要求对应套餐凭据，不能自动沿用其他方舟 Key。

`ark-code-latest` 是套餐路由别名，实际模型由套餐控制台配置，也可以按官方范围填写具体 Plan Model Name。它不保证始终对应某个固定模型；不要把按量模型 ID 不加核对地复制到套餐通道。百炼 Coding Plan、Token Plan 也各有模型目录。

控制台改变别名所指向的模型时，可更新高级选项中的模型版本标记并重新检测，按需重新分析或补建向量，避免沿用旧模型产物；原有索引保留。

接入方式不参与模型能力过滤。示例能力不是检测回执，所有生成模型仍可按实际情况声明并检测文本、图片、视频采样帧及结构化输出。共享文本 Embedding 则需要实际实现标准 `/embeddings`；套餐名本身既不证明支持，也不证明不支持。

官方另有套餐**使用范围**说明：

- 方舟 [Agent Plan 套餐权益](https://ark.volcengine.com/region:cn-beijing/docs/agent-plan-personal-plan-overview#6e51df70)说明 AI 工具范围及非工具 API 调用的额度限制；[Coding Plan 适用范围](https://ark.volcengine.com/region:cn-beijing/docs/coding-plan-personal-plan-overview#926a93ec)面向个人编码与支持的 AI 编程工具。
- 百炼 [Coding Plan 订阅前须知](https://help.aliyun.com/zh/model-studio/coding-plan)、[Token Plan 个人使用规则](https://help.aliyun.com/zh/model-studio/token-plan-personal-faq)和[团队订阅前须知](https://help.aliyun.com/zh/model-studio/token-plan-team-overview)对工具交互、自动化脚本、应用后端及非交互批量另有限定。

这些条款描述套餐权益的适用范围，不等同于模型缺少某种模态能力，也不能仅凭工具兼容名单判断 Lap 一律获准或一律禁止。Lap 提供独立配置和用途提示，不伪装其他客户端、不绕过服务端权限，也不因套餐名屏蔽模型能力。实际使用应核对账号约定；配置入口和检测通过不代表已确认任意业务都在套餐权益内。

其他预设的核对结果：硅基免费／`Pro/...` 模型属于模型 SKU、计费与配额差异，使用同一普通 API；DeepSeek 充值／赠送余额属于同一 API 的计费来源；LongCat 资源包会先于按量余额抵扣，同样无需新建一套接入。它们在编程工具中的兼容协议或模型版本变化，也不自动构成另一种套餐。

## 复用 LLM 服务的文本嵌入接口

如果现有 LLM 服务同时提供 OpenAI 兼容的 `/embeddings`，可以复用它的基础地址和 API Key：

1. 在 **LLM 服务**中选中服务，开启 **兼容 OpenAI 文本 Embeddings**。
2. 点 **手动添加嵌入模型**，填写服务端实际提供的 Embedding Model ID，例如 `text-embedding-3-small`。它是独立的嵌入模型记录，生成模型仍配置自己的 Model ID。
3. 配置查询编码规则并检测文本嵌入能力。检测使用 `/embeddings`，检查正文／查询返回数量、维度和向量数值。
4. 切换到 **Embedding 服务**，在顶部 **文本向量**中选择该模型并保存。同一服务会显示在该分类中，共享原有连接与密钥，只展示其嵌入模型。

开关声明服务的接口兼容性，模型记录区分生成和嵌入用途。关闭开关会解除对应文本向量默认绑定并阻止新的嵌入调用，保留模型配置与已生成的索引。服务和模型仍需具备可用的权限并通过检测。

OpenAI 的 `/models` 用于返回当前可用模型，官方 Embeddings 接口也建议用它查看模型；返回结果没有用于可靠分类的能力字段。兼容网关可能只列出部分模型，因此嵌入模型始终保留手动添加入口，不按名称猜测类别，也不要求先获取列表。网关必须实际实现或转发 `/embeddings`；仅开启 Lap 的兼容标记不会增加网关的接口能力。

该共享方式用于分析文本和文字查询。图片／视频由多模态 LLM 先产出标题、说明、标签和片段文本，再由嵌入模型编码入库；搜索时只需编码查询文字并匹配已有向量。直接编码原图或视频采样帧时，继续使用下文的专用多模态 Embedding 协议。

## 国内在线服务

在 **设置 → AI 模型 → 媒体理解 → Embedding 服务**侧栏直接选择内置服务，填写自己的平台 API Key；自定义端点使用 **添加服务**。内置条目带入基础地址、协议和可编辑示例模型。允许传输后检测，再启用服务，在顶部“文本向量”／“原画面向量”中选择并保存。预设不会自动请求、绑定用途或扫描全库。

| 服务预设 | API 基础地址 | 示例 Model ID | 接口协议 |
| --- | --- | --- | --- |
| 阿里百炼 · 文本 | `https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/compatible-mode/v1` | `text-embedding-v4` | OpenAI 标准文本 |
| 阿里百炼 · 多模态 | `https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/api/v1` | `qwen3-vl-embedding` | 百炼原生融合 |
| 火山方舟 · 多模态 | `https://ark.cn-beijing.volces.com/api/v3` | `doubao-embedding-vision-251215` | 方舟原生融合 |
| 火山方舟 · Agent Plan 多模态 | `https://ark.cn-beijing.volces.com/api/plan/v3` | `doubao-embedding-vision` | 方舟原生融合 |
| 硅基流动 · 文本 | `https://api.siliconflow.cn/v1` | `BAAI/bge-m3` | OpenAI 标准文本 |
| 硅基流动 · 多模态 | `https://api.siliconflow.cn/v1` | `Qwen/Qwen3-VL-Embedding-8B` | 硅基原生图像／文本 |

百炼文本与多模态预设均需填写自己的工作空间地址：文本保留 `/compatible-mode/v1`，多模态保留 `/api/v1`。密钥需要属于所用地域／工作空间。方舟填写推理 API Key，使用当前已开通的模型 ID 或推理接入点 ID；基础地址保留 `/api/v3`。网关地址和模型别名均可编辑，Lap 不按模型名称猜测实际能力。

百炼文本与硅基文本预设均使用文本 `input` 和稠密浮点向量，默认普通文本；实际维度由检测读取。多模态预设同样可编码分析文本及文字查询，因此方舟原生模型可以同时用于“文本向量”和“原画面向量”。这两个用途保留独立索引身份。

Agent Plan 向量预设来自其官方工具接入说明，并核对了官方 OpenViking → Ark SDK 的原生 `/embeddings/multimodal` 调用链。它使用独立配置和套餐模型别名，不自动复制 LLM 服务凭据。文本／查询／三帧输入仍须通过本机检测；公开的工具示例不代表已用真实账号验证 Lap 的任意媒体组合或套餐适用性。

查询规则按协议区分：

- 百炼原生：正文采用供应商默认编码；显式填写查询指令时通过原生 `parameters.instruct` 发送。
- 方舟原生：面向 251215 及支持 `instructions` 的后续模型；适配器按文本／图像／混合语料设置对应原生指令。分析文本查询的 `Target_modality` 为 `text`；原画面库包含纯图片和“时间文字＋采样图”的视频片段，查询目标为 `image/text and image`。高级选项仅填写 `Instruction` 正文，适配器补齐用途对应的模板；空值使用 Lap 推荐检索指令。
- 硅基原生：当前公开接口没有单独的查询指令字段，采用供应商默认编码；界面不展示该选项。切换至此协议会清除旧指令并要求重新检测。

各协议不互相拼接查询前缀。更改模型、协议、编码规则或模型版本后重新检测，并为对应配置补建向量；旧索引保留。实际账号权限、额度及检索质量需要在所选平台验收。

可以继续用现有 CliProxyAPI 处理媒体理解／文本生成，同时由其中一家在线服务生成分析文本向量。为已有分析补建向量后，在自然语言检索里选择“分析文本”。两种用途不要求来自同一供应商。

### 多模态接口并不相同

| 协议 | 最终路由 | 请求与响应 |
| --- | --- | --- |
| OpenAI 文本 | 基础地址＋`/embeddings` | 文本 `input`；`data[].embedding` |
| 百炼多模态 | `/api/v1/services/embeddings/multimodal-embedding/multimodal-embedding` | 有序 `input.contents`，`enable_fusion=true`；`output.embeddings[0].embedding` |
| 方舟多模态 | `/api/v3/embeddings/multimodal` | 有序文本／`image_url` 内容块及原生 `instructions`；`data.embedding` |
| 硅基多模态 | `/v1/embeddings` | 字符串、图片对象或图片对象批次；`data[].embedding` |
| vLLM 多模态 | `/v1/embeddings` | `messages` 扩展及 system 指令；`data[0].embedding` |

视频继续由 Lap 按 60 秒片段取三张图像。百炼／方舟将有序采样帧和时间文字融合为一个向量；硅基官方不提供原生视频输入，Lap 将三帧作为独立图像样本编码，逐帧 L2 归一化后取均值，再归一化成一个片段向量，时间保存在数据库中。这个聚合方法不表示时序事件理解。返回向量数量、index、维度与非零范数必须全部有效，不能挑第一条或混用不同空间。

三家适配器均接入已有分析、批量任务、索引补建与自然语言搜索：LLM 产出标题／说明／标签，再由选定向量服务编码分析文本；原画面索引可独立执行。细节问答继续由 LLM 使用分析证据或本次画面复核完成。已有 CLIP 图文检索保留原入口和索引。

百炼／方舟原生接口不假设提供标准模型列表，预设带示例 ID，其他模型手动填写；其余协议继续提供模型发现入口。所有协议都必须通过本机的实际检测才能执行媒体任务。

## 自部署模型 API

将 [scripts/ai-server](../scripts/ai-server/README.md) 整个目录上传到具备相应 NVIDIA GPU／驱动的 Linux 服务器。每个 profile 使用独立环境、模型版本、端口和 API Key：

| profile | 用途 | 端口 | Model ID |
| --- | --- | --- | --- |
| `mage` | 媒体理解／文本生成 | 8000 | `mage-vl` |
| `qwen` | 媒体理解／文本生成 | 8001 | `qwen3-vl-4b-instruct` |
| `embedding` | 分析文本向量 | 8002 | `qwen3-embedding-0.6b` |
| `embedding-multimodal` | 原图／视频采样帧向量 | 8003 | `qwen3-vl-embedding-2b` |

已有可用的 LLM API 时，可以只部署需要的向量 profile。以原画面向量为例，在服务器的 `ai-server` 目录执行：

```bash
python3.12 deploy.py doctor embedding-multimodal
python3.12 deploy.py install embedding-multimodal
python3.12 deploy.py serve embedding-multimodal
```

`doctor` 检查环境，`install` 创建环境并下载固定版本，`serve` 在前台启动 API。该 profile 固定 vLLM 0.14.0，要求驱动支持 CUDA 12.9。其他 profile 的驱动条件不同，完整要求和参数见部署说明。脚本不修改系统 Python 或 GPU 驱动。

服务默认只监听服务器回环地址。可在自己的电脑执行 SSH 端口转发：

```bash
ssh -N -L 8003:127.0.0.1:8003 用户@服务器
```

随后在 Lap 配置：

| 项目 | 值 |
| --- | --- |
| 服务分类 | Embedding 服务 |
| 接口类型 | vLLM 多模态 |
| API 基础地址 | `http://127.0.0.1:8003/v1` |
| API Key | 服务器 `~/lap-ai/embedding-multimodal/server-api-key` 文件中的值；使用自定义 `--root` 时在对应目录 |
| Model ID | `qwen3-vl-embedding-2b` |
| 查询预设 | Qwen3-VL Embedding |
| 顶部默认用途 | 原画面向量 |

检测通过并保存后，在单媒体信息面板或批量入口补建原画面向量，搜索时选择“原画面”。视频由 Lap 按片段提取三张带时间的采样帧；无需先通过 LLM 生成说明。服务端依赖安装、显存容量与模型检索质量需在实际 GPU 上验收。

纯文本自部署将 profile 改为 `embedding`、转发端口改为 8002，Lap 选择 OpenAI 标准文本和 Qwen3 Embedding 查询预设。

## 官方资料

- [百炼 OpenAI 兼容说明](https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope)
- [硅基 Chat Completions](https://api-docs.siliconflow.cn/docs/api/chat-completions-post)
- [方舟 Chat 调用示例](https://docs.volcengine.com/docs/ark/deep-thinking?lang=en)
- [方舟 Agent Plan Chat 配置](https://ark.volcengine.com/region:cn-beijing/docs/agent-plan-personal-hermes-agent#bd524f99)
- [方舟 Coding Plan Chat 配置](https://ark.volcengine.com/region:cn-beijing/docs/coding-plan-personal-ai-hermes-agent#bd524f99)
- [方舟 Agent Plan 向量接入](https://ark.volcengine.com/region:cn-beijing/docs/agent-plan-personal-vectorized-models)
- [百炼 Coding Plan](https://help.aliyun.com/zh/model-studio/coding-plan)
- [百炼 Token Plan 个人](https://help.aliyun.com/zh/model-studio/token-plan-personal-faq)
- [百炼 Token Plan 团队](https://help.aliyun.com/zh/model-studio/token-plan-team-faq)
- [DeepSeek API](https://api-docs.deepseek.com/)
- [DeepSeek 计费来源说明](https://api-docs.deepseek.com/zh-cn/quick_start/pricing)
- [LongCat Chat](https://longcat.chat/platform/docs/zh/api/chat)
- [LongCat 模型列表](https://longcat.chat/platform/docs/zh/api/models)
- [LongCat API 按量与资源包扣抵](https://longcat.chat/platform/docs/zh/api-pay-as-you-go)
- [OpenAI 可用模型列表](https://developers.openai.com/api/reference/resources/models/methods/list)
- [OpenAI 创建 Embedding](https://developers.openai.com/api/reference/resources/embeddings/methods/create)
- [硅基流动 Embeddings 接口](https://api-docs.siliconflow.cn/docs/api/embeddings-post)
- [BGE-M3 官方模型说明](https://huggingface.co/BAAI/bge-m3)
- [百炼文本 Embedding 接口](https://www.alibabacloud.com/help/en/model-studio/text-embedding-synchronous-api)
- [百炼地域与基础地址](https://www.alibabacloud.com/help/en/model-studio/regions)
- [百炼多模态 Embedding 接口](https://www.alibabacloud.com/help/en/model-studio/multimodal-embedding-api-reference)
- [方舟多模态向量化说明](https://docs.volcengine.com/docs/ark/vectorization?lang=en&redirect=1)
- [方舟向量化指令说明](https://www.volcengine.com/docs/82379/2121931)

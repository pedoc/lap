# Embedding 服务使用指南

核对日期：2026-10-25。Lap 的 Rust 后端随桌面程序启动；`scripts/ai-server` 用于在另一台机器部署模型 API。也可以直接使用在线 API，二者使用相同的服务／模型配置流程。

## 国内在线服务

在 **设置 → AI 模型 → 媒体理解 → Embedding 服务**侧栏选择服务预设，再点 **添加服务**。预设会创建一个新服务和一个可编辑的示例模型，带入对应基础地址与协议；不会修改已有服务、自动发送请求、绑定默认用途或扫描全库。填写自己的平台 API Key，启用服务并允许传输后检测，再在顶部“文本向量”／“原画面向量”中选择并保存。

| 服务预设 | API 基础地址 | 示例 Model ID | 接口协议 |
| --- | --- | --- | --- |
| 阿里百炼 · 文本 | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `text-embedding-v4` | OpenAI 标准文本 |
| 阿里百炼 · 多模态 | `https://dashscope.aliyuncs.com/api/v1` | `qwen3-vl-embedding` | 百炼原生融合 |
| 火山方舟 · 多模态 | `https://ark.cn-beijing.volces.com/api/v3` | `doubao-embedding-vision-251215` | 方舟原生融合 |
| 硅基流动 · 文本 | `https://api.siliconflow.cn/v1` | `BAAI/bge-m3` | OpenAI 标准文本 |
| 硅基流动 · 多模态 | `https://api.siliconflow.cn/v1` | `Qwen/Qwen3-VL-Embedding-8B` | 硅基原生图像／文本 |

百炼预设采用官方示例域名；控制台提供工作空间专属域名时，可将主机改为 `https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com`，文本保留 `/compatible-mode/v1`，多模态保留 `/api/v1`，替换其中的 `WorkspaceId`。密钥需要属于所用地域／工作空间。方舟填写推理 API Key，使用当前已开通的模型 ID 或推理接入点 ID；基础地址保留 `/api/v3`。网关地址和模型别名均可编辑，Lap 不按模型名称猜测实际能力。

百炼文本与硅基文本预设均使用文本 `input` 和稠密浮点向量，默认普通文本；实际维度由检测读取。多模态预设同样可编码分析文本及文字查询，因此方舟原生模型可以同时用于“文本向量”和“原画面向量”。这两个用途保留独立索引身份。

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

- [硅基流动 Embeddings 接口](https://api-docs.siliconflow.cn/docs/api/embeddings-post)
- [BGE-M3 官方模型说明](https://huggingface.co/BAAI/bge-m3)
- [百炼文本 Embedding 接口](https://www.alibabacloud.com/help/en/model-studio/text-embedding-synchronous-api)
- [百炼地域与基础地址](https://www.alibabacloud.com/help/en/model-studio/regions)
- [百炼多模态 Embedding 接口](https://www.alibabacloud.com/help/en/model-studio/multimodal-embedding-api-reference)
- [方舟多模态向量化说明](https://docs.volcengine.com/docs/ark/vectorization?lang=en&redirect=1)
- [方舟向量化指令说明](https://www.volcengine.com/docs/82379/2121931)

# Embedding 服务使用指南

核对日期：2026-10-25。Lap 的 Rust 后端随桌面程序启动；`scripts/ai-server` 用于在另一台机器部署模型 API。也可以直接使用在线 API，二者使用相同的服务／模型配置流程。

## 在线文本 Embedding

在 **设置 → AI 模型 → 媒体理解 → Embedding 服务**添加服务，协议选择 **OpenAI 标准文本**。填写平台 API Key，添加实际模型，查询预设选择下表值；检测成功后，再在顶部“默认用途”的“文本向量”中选择并保存。

| 服务 | API 基础地址 | Model ID | 查询预设 |
| --- | --- | --- | --- |
| 硅基流动 | `https://api.siliconflow.cn/v1` | `BAAI/bge-m3` | 普通文本 |
| 阿里百炼北京 | 以控制台对应工作空间的基础地址为准，形式为 `https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/compatible-mode/v1` | `text-embedding-v4` | 普通文本 |

百炼旧基础地址 `https://dashscope.aliyuncs.com/compatible-mode/v1` 仍有兼容支持；新配置优先采用控制台给出的工作空间专属地址，替换其中的 `WorkspaceId`。密钥需要属于所用地域／工作空间。

两者的官方文本接口均支持文本 `input` 和稠密浮点向量。BGE-M3 官方说明无需额外查询指令；百炼的可选 `instruct` 是供应商接口参数，不等同于 Lap 的 Qwen3 文本前缀，所以以上配置选择普通文本。实际维度由检测读取。这里核对的是接口文档，未使用真实账户发起推理，也不承诺账户额度或免费使用。

可以继续用现有 CliProxyAPI 处理媒体理解／文本生成，同时由其中一家在线服务生成分析文本向量。为已有分析补建向量后，在自然语言检索里选择“分析文本”。两种用途不要求来自同一供应商。

### 国内多模态服务

百炼提供专用多模态模型，接口使用 `input.contents` 和独立的多模态路径；硅基流动的 VL 向量接口使用带文本／图片的 `input` 对象或数组，官方当前不支持视频。这些格式与本轮已实现的 **vLLM messages 扩展**不同，尚未接入对应适配器，不能只修改基础地址就宣称兼容。

要在现版使用原画面向量检索，可以使用下面自部署的 vLLM 多模态服务；国内供应商专用接口需要后续显式适配。已有 CLIP 图文检索继续使用原来的入口与索引。

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

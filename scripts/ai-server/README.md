# 部署生成模型、文本与多模态 Embedding

本目录可单独上传到任意 Linux GPU 服务器，不要求安装 Lap。完整描述语义检索需要 **一个生成模型（Mage 或 Qwen-VL）＋一个文本 Embedding 模型**。生成模型同时承担媒体理解和文本生成，不必再部署一个文字 LLM。

原图／视频采样帧向量检索使用独立的 **Qwen3-VL-Embedding** profile，可直接编码媒体与文本查询，不依赖生成模型先写描述。Mage-VL、Qwen3-VL-Instruct 是生成模型，不与专用 Embedding 模型混称。

Lap 已接入 Chat Completions、标准文本 Embeddings、本目录固定的 vLLM 多模态协议，以及百炼、方舟和硅基的原生向量适配器，可配置媒体理解、资料建议、批量任务、两类向量检索和问答。操作入口见 [媒体理解使用说明](../../docs/media-understanding.md)。真实 GPU 安装与模型效果仍需在目标服务器验收。

## 1. 模型与接口

固定版本见 [profiles.json](profiles.json)，核对日期为 2026-10-24。设计原型直接读取此文件，不另写一套模型 ID 和端口。

| profile | 模型 → 服务端 model ID | 服务接口与端口 | 固定环境 |
| --- | --- | --- | --- |
| `mage`，与 qwen 二选一 | `microsoft/Mage-VL` → `mage-vl` | Chat Completions，8000 | 官方 SGLang fork 固定提交；Python 3.12；本包要求驱动支持 CUDA ≥13、CUDA 13 开发工具、Rust 1.90、protoc |
| `qwen`，与 mage 二选一 | `Qwen/Qwen3-VL-4B-Instruct` → `qwen3-vl-4b-instruct` | Chat Completions，8001 | vLLM 0.11.0；Python 3.12；驱动支持 CUDA ≥12.8 |
| `embedding`，分析文本检索 | `Qwen/Qwen3-Embedding-0.6B` → `qwen3-embedding-0.6b` | `/v1/embeddings`，8002 | vLLM 0.11.0 pooling runner；Python 3.12；驱动支持 CUDA ≥12.8；1024 维 |
| `embedding-multimodal`，原媒体向量 | `Qwen/Qwen3-VL-Embedding-2B` → `qwen3-vl-embedding-2b` | `/v1/embeddings` 的 `messages` 扩展，8003 | vLLM 0.14.0 pooling runner；Python 3.12；驱动支持 CUDA ≥12.9；2048 维 |

各 profile 均为回环监听、独立 venv 和独立 API Key。服务名称由用户自定义，不存在自建／团队／托管平台分类。一个进程通常提供一个模型；需要同一个地址调度多个模型时可另接网关，脚本不要求网关。

Mage 的 CUDA 13 要求来自本脚本固定的 fork／PyTorch 组合，并非模型的通用最低要求。默认显存比例为生成服务 0.8、文本 Embedding 0.15、多模态 Embedding 0.35，只是起点；不能据此保证同一卡可同时常驻。请在实际 GPU 上检查权重、上下文、KV cache 和并发容量，可用 `--memory`、`--context` 调整或分机部署。

## 2. 安装与启动

上传目录后，默认资源在 `~/lap-ai`。数据盘或既有环境可用 `--root /path/to/lap-ai`；此前安装在其他路径时继续显式指定原目录，脚本不会移动或删除它。

先在任意本机查看计划，不下载、不创建环境、不生成密钥：

```bash
python deploy.py install qwen --dry-run
python deploy.py serve qwen --dry-run
python deploy.py install embedding --dry-run
python deploy.py serve embedding --dry-run
```

服务器上的 Qwen 路线，在一个终端执行：

```bash
python3.12 deploy.py doctor qwen
python3.12 deploy.py install qwen
python3.12 deploy.py serve qwen
```

另一个终端启动文本向量服务：

```bash
python3.12 deploy.py doctor embedding
python3.12 deploy.py install embedding
python3.12 deploy.py serve embedding
```

Mage 路线把第一组 `qwen` 换为 `mage`，仍使用同一个 `embedding` profile。Mage 源码构建需先按上游准备 git、g++、protoc、CUDA 13 开发工具及 Rust 1.90；`doctor` 会检查。图片探测不依赖 FFmpeg，视频探测需额外安装 `ffmpeg` 和 `ffprobe`。

服务前台运行，可放在自己的 tmux 会话中。Ctrl+C 只停止自己启动的服务。权重按固定 revision 下载；也可用 `--model-path /path/to/complete/model` 指定可信的完整模型目录，其版本由部署者核对。脚本不修改系统 Python、GPU 驱动或其他服务。

## 3. 先验收模型 API

只成功列出模型，不能证明图文、结构化输出、视频顺序或向量能力可用。分别探测实际接口：

```bash
python3.12 probe.py --base-url http://127.0.0.1:8001/v1 \
  --model qwen3-vl-4b-instruct --key-file "$HOME/lap-ai/qwen/server-api-key" \
  --image /path/to/public-example.jpg --question "用中文描述可见内容。"

python3.12 probe.py --base-url http://127.0.0.1:8002/v1 \
  --model qwen3-embedding-0.6b --key-file "$HOME/lap-ai/embedding/server-api-key" \
  --protocol embeddings --question "湖泊和木屋"
```

API Key 首次启动生成到对应 profile 的 `server-api-key` 文件，后续复用；不打印实际值，也不放进进程命令行。`--key-env` 可代替 `--key-file`。请只使用公开或已授权的测试媒体。

### 多模态 Embedding 的独立部署与探测

```bash
python3.12 deploy.py install embedding-multimodal --dry-run
python3.12 deploy.py serve embedding-multimodal --dry-run
python3.12 deploy.py doctor embedding-multimodal
python3.12 deploy.py install embedding-multimodal
python3.12 deploy.py serve embedding-multimodal
```

该 profile 固定 vLLM 0.14.0 和专用模型 revision。它使用该版的 `--pooler-config`、LAST pooling 与归一化；最多接收 8 张图片，原生 `video_url` 输入关闭。视频由本机采样为按时间排列的图片，一个片段的多帧合成一个向量，不把每帧当成独立媒体记录。旧的 vLLM 0.11.0 profile 不会升级或复用其环境。

分别验证文本查询、原图和视频采样帧；后两条不需要生成模型，也不发送聊天生成预算：

```bash
python3.12 probe.py --base-url http://127.0.0.1:8003/v1 \
  --model qwen3-vl-embedding-2b --protocol vllm-multimodal \
  --key-file "$HOME/lap-ai/embedding-multimodal/server-api-key" \
  --question "湖边的木屋" --instruction "Retrieve images or text relevant to the user's query."

python3.12 probe.py --base-url http://127.0.0.1:8003/v1 \
  --model qwen3-vl-embedding-2b --protocol vllm-multimodal \
  --key-file "$HOME/lap-ai/embedding-multimodal/server-api-key" --image /path/to/public-example.jpg

python3.12 probe.py --base-url http://127.0.0.1:8003/v1 \
  --model qwen3-vl-embedding-2b --protocol vllm-multimodal \
  --key-file "$HOME/lap-ai/embedding-multimodal/server-api-key" --video /path/to/public-example.mp4 --frames 3
```

固定的请求契约是 `POST /v1/embeddings`，单个 `messages` 会话对应一个稠密向量：

```json
{
  "model": "qwen3-vl-embedding-2b",
  "encoding_format": "float",
  "add_generation_prompt": true,
  "messages": [
    {"role": "system", "content": [{"type": "text", "text": "Represent the user's input."}]},
    {"role": "user", "content": [{"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,..."}}]}
  ]
}
```

文本查询把 user 内容换成 `text`，检索指令放入 system；不套用 Qwen3 文本 Embedding 的 `Instruct: ...\nQuery:...` 前缀。图片使用 data URL，避免服务端读取本机路径。视频采样帧在每个 `image_url` 前加入对应原视频秒数的 `text`；时间是采样位置，不代表精确事件边界。查询与媒体必须使用同一模型、版本、维度和匹配的编码规则，向量长度相同不证明处在同一空间。

这是 **vLLM 多模态扩展适配器**，不是通用 OpenAI 文本 `input` 契约；本脚本不使用更新版本才出现的 conversation `input` 别名，也不实现 `/v2/embed` 或其他服务商多模态格式。探测只检查单向量数量、index、有限值与范数，不证明语义对齐、模型效果或正式 Lap 验收。原视频采样依赖 FFmpeg／FFprobe，临时帧在请求结束后清理，原文件不修改。

上游依据：[Qwen 官方模型与 vLLM 版本要求](https://github.com/QwenLM/Qwen3-VL-Embedding)、[vLLM 0.14.0 EmbeddingChatRequest](https://github.com/vllm-project/vllm/blob/v0.14.0/vllm/entrypoints/pooling/embed/protocol.py)、[vLLM 0.14.0 CUDA 镜像版本](https://github.com/vllm-project/vllm/blob/v0.14.0/docker/Dockerfile)。Qwen 官方示例使用 `add_generation_prompt=True`；vLLM HTTP 协议默认是 false，因此请求中显式指定 true。

## 4. 验证完整模型链路

`pipeline_probe.py` 使用标准库调用实际服务，依次校验：

1. 图片或视频采样帧进入生成模型，返回标题、说明和标签 JSON。
2. 分析文字作为正文送入文本 Embedding。
3. 用户查询按匹配的指令方案编码，校验结果数量、index、维度、有限值和范数。
4. 在独立临时 SQLite 保存分析与向量，关闭并重开数据库，按相同索引身份召回。
5. 将召回证据交给生成模型回答，检查回答结构与引用 ID。

Qwen 完整图片链路：

```bash
python3.12 pipeline_probe.py \
  --vision-base http://127.0.0.1:8001/v1 --vision-model qwen3-vl-4b-instruct \
  --vision-key-file "$HOME/lap-ai/qwen/server-api-key" \
  --embedding-base http://127.0.0.1:8002/v1 \
  --embedding-key-file "$HOME/lap-ai/embedding/server-api-key" \
  --image /path/to/public-example.jpg --query "湖边有哪些建筑？"
```

Mage 路线仅替换三个生成参数：端口改为 `8000`、model ID 为 `mage-vl`、密钥文件为 `$HOME/lap-ai/mage/server-api-key`。

视频使用相同命令，将 `--image ...` 改为 `--video /path/to/public-example.mp4 --frames 4`。脚本通过 FFmpeg 采样，原片时间随输入传递；不把采样命中当成精确事件边界。图片模式可重复 `--image` 测试最多 8 个媒体；视频最多采样 8 帧。

报告只输出阶段、索引身份、维度、记录 ID 和分数，不输出密钥、完整分析或原始向量。`result: passed` 表示 API／结构／保存与读取契约通过；`semanticQuality: manual-review-required` 表示准确性仍需人工审查；`lapRuntime: not-tested` 明确表示没有验收正式 Lap。临时 SQLite 不是 Lap 图库数据库，结束后清理，不修改原文件或正式标题／标签。

如测试其他标准文本 Embedding，可提供 `--embedding-model`、`--embedding-dimensions`、`--embedding-revision`、`--query-instruction`。默认 Qwen3 方案为正文原文、查询 `Instruct: ...\nQuery:...`；空指令表示纯文本查询。此脚本不把 Jina／vLLM 多模态扩展伪装成标准文本输入，也不支持稀疏或 token 多向量结果。

上述 `pipeline_probe.py` 仍专用于“生成描述 → 文本向量”的链路。原媒体多模态入口使用上一节的 `probe.py --protocol vllm-multimodal`；两种输入和索引身份不能混用。

## 5. 在本机 Lap 配置

使用百炼、方舟或硅基在线 API 时，不必运行本目录脚本；在 Embedding 服务选择供应商预设并填写自己的 Key，再检测和绑定默认用途即可。原生多模态的协议、指令与视频聚合区别见 [Embedding 服务指南](../../docs/embedding-services.md)。本目录继续负责自建 GPU 服务，和在线服务可以混用。

保持回环监听，通过自己的 SSH 连接转发所需端口，例如 Qwen 路线：

```bash
ssh -N -L 8001:127.0.0.1:8001 -L 8002:127.0.0.1:8002 -p SSH端口 用户@SSH主机
```

| 配置项 | 生成服务 | 文本向量服务 |
| --- | --- | --- |
| 名称 | 任意自定义名称 | 任意自定义名称 |
| API 地址 | `http://127.0.0.1:8001/v1`，Mage 用 8000 | `http://127.0.0.1:8002/v1` |
| model ID | `qwen3-vl-4b-instruct`，或 `mage-vl` | `qwen3-embedding-0.6b` |
| 能力 | 图片、视频采样帧、文本生成、结构化分析 | 文本 Embedding |
| 默认用途 | 媒体理解；文本生成可选择“与媒体理解相同” | 分析文本 Embedding |
| 向量设置 | 不需要 | 1024 维；正文纯文本；查询指令来自固定 profile |

两个服务可在不同机器，也可在容量足够的同一机器。使用公网或网关时配置实际 TLS／认证地址；无需绑定任何托管平台。

在正式 Lap 的独立设置窗口进入“AI 模型 → 媒体理解”，按“配置服务 → 同步或手填模型 → 勾选并检测能力 → 选择默认用途 → 创建媒体任务 → 查询已建立索引”操作。模型服务工具不修改用户图库，资料采用由 Lap 的事务负责。

在该页签内选择 **Embedding 服务 → 添加服务**，或选中已有服务，填写上表的向量 API 地址和 `embedding/server-api-key` 文件中的 Key，协议选择 OpenAI 标准文本。添加模型 `qwen3-embedding-0.6b`，选择 Qwen3 查询预设，点击能力旁的“检测”；随后在默认用途选择它并保存。维度由实际检测读取；模型版本和自定义指令在高级设置中。默认用途旁的“配置”只定位现有表单。保存后再创建媒体索引任务，不会自动分析全库。

原画面检索使用独立 `embedding-multimodal` profile。转发 8003 端口后，在 **Embedding 服务**添加服务，选择 **vLLM 多模态**，填写 `http://127.0.0.1:8003/v1` 和该 profile 的 Key，添加模型 `qwen3-vl-embedding-2b`；使用 Qwen3-VL 查询预设，检测后选为“多模态 Embedding（原画面）”并保存。在单媒体或批量入口补建原画面向量，再在自然语言检索中切换到“原画面”。无需先配置 LLM 或生成媒体说明；视频按 60 秒片段、每片三张带时间的采样帧编码，结果时间是片段范围。

| 场景 | 只有生成服务 | 再接文本 Embedding |
| --- | --- | --- |
| 图片／视频采样内容理解 | 可以，需实测 | 可以 |
| 标题、说明、标签建议及显式采用 | 可以；资料写入由 Lap 负责 | 可以 |
| 当前媒体问答／有限范围视觉复核 | 可以，需实测 | 可以，另可检索已有片段证据 |
| 日期／评分等自然语言字段条件解析 | 首版未接入；可继续使用现有组合搜索 | 同左 |
| 分析描述语义检索 | 尚不可用；已有分析可保留 | 建立匹配索引后可用 |
| 原图片／视频直接多模态向量检索 | 未覆盖 | 文本组合不覆盖；使用独立的 `embedding-multimodal` 服务与 vLLM 多模态协议 |

## 6. 已验证与待验收

```powershell
python -B -m unittest discover -s scripts/ai-server -p 'test_*.py' -v
```

本机测试包括部署计划、固定版本与认证、真实回环 HTTP、错误正文脱敏、结构化校验、文档／查询编码、向量批次顺序、SQLite 重开召回、证据引用，以及有 FFmpeg 时的真实小视频采样。它们不安装 GPU 依赖、不产生在线模型 API 费用。

仍需在真实 GPU 上完成依赖安装、显存／驱动、图片与视频推理及中文检索效果验收。正式 Lap 已用隔离图库、合成媒体和本机测试 API 验收数据库链路、恢复和资料写入，证据见 [运行验证](../../docs/development/verification/media-ai-runtime.json)；这不代表已经验证真实 Mage／Qwen 的推理效果。协议调查见 [ai-service-protocols.md](../../docs/development/ai-service-protocols.md)。

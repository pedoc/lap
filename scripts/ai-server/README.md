# 部署生成模型与文本 Embedding，验证媒体业务链路

本目录可单独上传到任意 Linux GPU 服务器，不要求安装 Lap。完整描述语义检索需要 **一个生成模型（Mage 或 Qwen-VL）＋一个文本 Embedding 模型**。生成模型同时承担媒体理解和语言生成，不必再部署一个文字 LLM。

本轮交付是设计稿与模型服务工具。正式 Lap 尚未实现新增业务接入；原型的检测、填充、数据库与检索均为模拟。工具可先验证真实模型 API 契约，待正式业务实现后使用同样的配置验收。

## 1. 模型与接口

固定版本见 [profiles.json](profiles.json)，核对日期为 2026-10-23。设计原型直接读取此文件，不另写一套模型 ID 和端口。

| profile | 模型 → 服务端 model ID | 服务接口与端口 | 固定环境 |
| --- | --- | --- | --- |
| `mage`，与 qwen 二选一 | `microsoft/Mage-VL` → `mage-vl` | Chat Completions，8000 | 官方 SGLang fork 固定提交；Python 3.12；本包要求驱动支持 CUDA ≥13、CUDA 13 开发工具、Rust 1.90、protoc |
| `qwen`，与 mage 二选一 | `Qwen/Qwen3-VL-4B-Instruct` → `qwen3-vl-4b-instruct` | Chat Completions，8001 | vLLM 0.11.0；Python 3.12；驱动支持 CUDA ≥12.8 |
| `embedding`，语义检索需要 | `Qwen/Qwen3-Embedding-0.6B` → `qwen3-embedding-0.6b` | `/v1/embeddings`，8002 | vLLM 0.11.0 pooling runner；Python 3.12；驱动支持 CUDA ≥12.8；1024 维 |

三个服务均为回环监听、独立 venv 和独立 API Key。服务名称由用户自定义，不存在自建／团队／托管平台分类。一个进程通常提供一个模型；需要同一个地址调度多个模型时可另接网关，脚本不要求网关。

Mage 的 CUDA 13 要求来自本脚本固定的 fork／PyTorch 组合，并非模型的通用最低要求。默认生成服务显存比例 0.8、Embedding 0.15，只是起点；不能据此保证同一卡可同时常驻。请在实际 GPU 上检查权重、上下文、KV cache 和并发容量，可用 `--memory`、`--context` 调整或分机部署。

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

## 5. 在本机 Lap 配置

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
| 默认用途 | 媒体理解；语言生成可选择“与媒体理解相同” | Embedding |
| 向量设置 | 不需要 | 1024 维；正文纯文本；查询指令来自固定 profile |

两个服务可在不同机器，也可在容量足够的同一机器。使用公网或网关时配置实际 TLS／认证地址；无需绑定任何托管平台。

在设计稿中按“启用服务 → 配置密钥与地址 → 同步或手填模型 → 检测模型能力 → 选择默认用途 → 验证业务链路 → 创建媒体任务 → 查询已建立索引”操作。正式 Lap 的对应业务仍需按 [开发蓝图](../../design/media-understanding/SPEC.md) 实现和验收。

| 场景 | 只有生成服务 | 再接文本 Embedding |
| --- | --- | --- |
| 图片／视频采样内容理解 | 可以，需实测 | 可以 |
| 标题、说明、标签建议及显式采用 | 可以；资料写入由 Lap 负责 | 可以 |
| 当前媒体问答／有限范围视觉复核 | 可以，需实测 | 可以，另可检索已有片段证据 |
| 日期／评分等字段条件解析 | 可复用生成模型；由应用校验条件 | 同左 |
| 分析描述语义检索 | 尚不可用；已有分析可保留 | 建立匹配索引后可用 |
| 原图片／视频直接多模态向量检索 | 未覆盖 | 本组合仍不覆盖，需要另行适配多模态向量服务 |

## 6. 已验证与待验收

```powershell
python -B -m unittest discover -s scripts/ai-server -p 'test_*.py' -v
```

本机测试包括部署计划、固定版本与认证、真实回环 HTTP、错误正文脱敏、结构化校验、文档／查询编码、向量批次顺序、SQLite 重开召回、证据引用，以及有 FFmpeg 时的真实小视频采样。它们不安装 GPU 依赖、不产生在线模型 API 费用。

仍需在真实 GPU 上完成依赖安装、显存／驱动、图片与视频推理及中文检索效果验收；正式 Lap 另验收 Tauri、图库索引、批量恢复、资料写入与权限边界。协议调查见 [ai-service-protocols.md](../../docs/development/ai-service-protocols.md)。

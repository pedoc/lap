# 远程 GPU 上部署 Mage／Qwen，向 Lap 提供标准 API

适用场景：在 AutoDL 或其他 Linux GPU 机器运行模型，本机 Lap 通过 API 调用。脚本目录可单独上传，不需要在服务器安装整个 Lap。

**验证范围**：本目录已做离线计划、协议序列化、认证／错误处理和回环 HTTP 测试。尚未在真实 AutoDL GPU 上安装与推理，不应视为已验收镜像。首次依赖编译和权重下载时间取决于服务器与网络。

## 1. 选择环境

固定配置见 [profiles.json](profiles.json)，核对日期为 2026-10-23。

| profile | 模型与服务别名 | 后端 | 此脚本默认环境要求 | 默认端口 |
| --- | --- | --- | --- | --- |
| `mage` | `microsoft/Mage-VL` → `mage-vl` | 官方 README 指定的 SGLang fork，固定提交 | Linux x86_64、Python 3.12、驱动支持 CUDA ≥13.0、CUDA 13 开发工具、Rust 1.90.0、protoc | 8000 |
| `qwen` | `Qwen/Qwen3-VL-4B-Instruct` → `qwen3-vl-4b-instruct` | vLLM 0.11.0，固定兼容依赖 | Linux x86_64、Python 3.12、驱动支持 CUDA ≥12.8 | 8001 |

Mage 的 CUDA 13 要求来自**本脚本选定的 fork／PyTorch 2.11 默认 wheel 组合**，不是宣称 Mage 模型只能在 CUDA 13 上运行。上游其他组合需另外验证；不把下载到的任意 SGLang 最新版当作替代品。

建议选择与上述 CUDA 组合一致的开发镜像。安装器用独立 venv，不依赖镜像预装的系统 PyTorch。驱动不能由普通容器内的 pip 升级；租用前需确认。显存取决于图像尺寸、帧数、上下文、并发和引擎，并不只由模型参数量决定，本目录不承诺某张卡必定可运行。

先将本目录上传到服务器，例如 `/root/lap-ai-server`，进入该目录。AutoDL 的 `autodl-tmp` 为数据盘位置；默认资源保存在 `~/autodl-tmp/lap-ai`，可用 `--root` 修改。不要放进上传的代码目录。

Mage 源码构建的前置工具按上游说明准备。Debian／Ubuntu、具备管理员权限的服务器示例：

```bash
apt-get update
apt-get install -y git build-essential protobuf-compiler curl ffmpeg
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/lap-rustup-init.sh
sh /tmp/lap-rustup-init.sh -y --profile minimal --default-toolchain 1.90.0
source "$HOME/.cargo/env"
```

CUDA 开发工具与驱动来自所选 GPU 镜像／宿主机；脚本不替换它们。Qwen 路线不要求 Rust 和 protoc。

## 2. 检查、安装、启动

先用 dry-run 查看计划，不下载、不创建环境、不生成密钥；也可在本机查看：

```bash
python3.12 deploy.py install mage --dry-run
python3.12 deploy.py serve mage --dry-run
```

在服务器实际执行 Mage：

```bash
python3.12 deploy.py doctor mage
python3.12 deploy.py install mage
python3.12 deploy.py serve mage
```

Qwen 对应为：

```bash
python3.12 deploy.py doctor qwen
python3.12 deploy.py install qwen
python3.12 deploy.py serve qwen
```

两套 venv、权重缓存和固定版本分别管理。默认每个服务只并发一个请求，8192 上下文，显存比例 0.8；这只是起始配置，不能替代真实显存测试。可以通过 `--context`、`--memory`、`--port` 调整。显存不足时先停一个服务，再验证另一个，避免误以为两个模型能在同一 GPU 同时常驻。

权重按固定 Hugging Face revision 下载到服务器缓存；可通过当前环境的 Hugging Face 凭据／网络配置下载，或手工准备完整目录后启动：

```bash
python3.12 deploy.py serve mage --model-path /root/autodl-tmp/models/mage-vl
```

使用自备目录时，其检查点和处理器版本由你核对；目录需包含完整权重、配置、分词器及自定义代码。Mage 启动会执行该模型包的 remote code，请使用可信的固定版本。

服务前台运行，启动失败返回非零退出码；Ctrl+C 停止自己启动的服务。可在自己的 `tmux` 会话中运行以避免 SSH 断开导致退出，本脚本不会按进程名清理其他服务。

## 3. 在服务器验收

首次启动自动生成独立 API Key 文件，权限为 0600，后续启动复用。脚本不打印密钥、不把实际密钥放进进程命令行；服务端日志仍按对应后端规则管理。

Mage 列表与真实图片测试：

```bash
python3.12 probe.py --base-url http://127.0.0.1:8000/v1 \
  --model mage-vl --key-file "$HOME/autodl-tmp/lap-ai/mage/server-api-key" --list-only

python3.12 probe.py --base-url http://127.0.0.1:8000/v1 \
  --model mage-vl --key-file "$HOME/autodl-tmp/lap-ai/mage/server-api-key" \
  --image /root/public-example.jpg --question "用中文描述画面，只写可见内容。"
```

Qwen 用端口 `8001`、模型 `qwen3-vl-4b-instruct`、密钥文件 `qwen/server-api-key`。上传的例子请使用公开或已授权测试媒体。

先验证文本，再验证图片，再验证多图采样帧。可多次给出 `--image`，最多 8 张；在问题中明确各帧原视频时间及顺序。例如前两帧分别来自 00:24 与 00:28。该测试验证多图输入，不负责抽帧，也不把近似采样时间当作精确事件边界。

Mage 此 API 路径使用抽样帧，不包含其离线 codec／主动流式完整能力。生成合法 JSON、遵循 schema、正确理解图片、多图顺序和视频片段时间，都是需要另行验证的能力，不能只以列表成功或 HTTP 200 为通过。

`probe.py` 还提供 `--protocol responses`、`--protocol messages`、`--protocol embeddings`，用于检查明确支持这些接口的服务；它不会为后端添加接口。标准 `/v1/embeddings` 测试只发送文本，不能把图片向量扩展伪装成标准文本输入。

Messages 默认使用 `anthropic-version: 2026-10-21`，兼容旧网关时可明确传 `--anthropic-version 2023-06-01`。版本和认证不能因为接口路径相同就忽略。

## 4. 本机连接服务器

默认仅监听服务器 `127.0.0.1`，通过 AutoDL 提供的 SSH 地址和端口建立隧道。在**本机**另开终端：

```bash
ssh -N -L 8000:127.0.0.1:8000 -p SSH端口 root@SSH主机
# Qwen 可另映射 8001；本机端口被占用时可改 -L 左侧端口
```

| Lap 服务字段 | Mage | Qwen |
| --- | --- | --- |
| 名称 | AutoDL Mage | AutoDL Qwen |
| 协议 | OpenAI Chat Completions | OpenAI Chat Completions |
| API 基础地址 | `http://127.0.0.1:8000/v1` | `http://127.0.0.1:8001/v1` |
| 模型 ID | `mage-vl` | `qwen3-vl-4b-instruct` |
| API Key | 从该服务的密钥文件复制到凭据输入框 | 同左，使用 Qwen 的密钥 |
| 实际执行位置 | 远程 GPU，经本机 SSH 隧道 | 同左 |

同步模型后，在默认模型中选择图片／视频理解及语言生成。Embedding 选择真正的向量模型；上面两个生成服务不是通用 embedding 服务。

原型中的 `8002` 私有向量服务仅为配置示例，本工具不会启动它；向量模型需另行选择与部署。

**当前 Lap 正式代码尚未实现这些新增生成协议**：本轮更新的是设计、协议调查和部署工具。服务可先用探测脚本或其他标准客户端验收，待 Lap 对应适配器实现后按相同配置接入。不要把新原型里的模拟“检测成功”视为真实调用。

如使用平台 HTTP 转发或公网反向代理，按平台说明配置端口、TLS 和认证，再填写相应基础地址。单个进程通常服务单个模型；确需同一地址调度多模型时再增加网关，初次部署不强制安装额外协议转换层。

## 5. 本机可运行的工具测试

```powershell
python -B -m unittest discover -s scripts/ai-server -p 'test_*.py' -v
```

包括真实回环 HTTP 的三种生成协议与文本 embedding 请求／响应、密钥处理、URL 拼接、错误正文脱敏、向量校验、部署计划和驱动预检。不会连接租用服务器、安装 GPU 依赖或产生模型 API 费用。

调查依据与固定上游提交见 [标准 API 与 Cherry Studio 配置调查](../../docs/development/ai-service-protocols.md)。

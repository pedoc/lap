# 生成式模型本地运行与 API 接入调查

调查日期：2026-10-23。范围为 Mage-VL、Qwen2.5-VL／Qwen3-VL 与 Lap 的接入方式；不代表所有同系列模型、量化包、平台或设备已经通过验收。

本轮只核对官方文档、发布记录和源码，没有下载大模型、安装 Python 环境或执行真实模型推理。

## 结论

- 需要同时支持本机原生推理、本机受管服务和外部 API；ONNX、GGUF、PyTorch 是模型包与引擎选择，不是业务用途分类。
- Qwen-VL 已有正式的 ONNX Runtime GenAI 支持，可评估无 Python 的用户端运行方式；也存在 llama.cpp 的原生 GGUF 路线。
- Mage-VL 有三组件 ONNX 导出，但 Mobius 当前明确不提供可供 ORT GenAI 直接运行的导出包。它与完整官方 PyTorch 路线、API 抽帧路线的能力覆盖要分开验收。
- 没有合适的 ONNX 包时，不必直接要求用户安装系统 Python；先判断是否有适配的原生引擎，否则使用版本隔离的 Python 服务。
- ONNX 导出阶段使用 Python，不等于用户推理阶段必须安装 Python。建议由开发／发布流程准备并验证模型包，用户下载配套产物。

## 已确认的支持情况

| 路线 | 证据 | 对 Lap 的含义 |
| --- | --- | --- |
| Mage-VL + 普通 ONNX | Mobius 主分支提供 `vision`、`embeddings`、`decoder_model` 三组件导出；配置输出明确拒绝 ORT GenAI | 可研究专用适配器，但不能只把 ONNX 文件加入当前目录就完成问答 |
| Mage-VL + 官方 PyTorch | 官方提供离线推理；依赖包括 PyTorch、Transformers、FlashAttention、mamba-ssm 等 | 完整功能优先沿官方路径验证；Windows 原生、WSL2、Linux 等运行环境分别确认 |
| Mage-VL + SGLang API | 官方提供专用 SGLang 分支与 `/v1` 服务示例；API 客户端使用图片或抽样视频帧 | 服务可部署在本机或其它机器；编码视频与主动流式功能不能由普通 API 调用直接推定 |
| Qwen2.5-VL／Qwen3-VL + ORT GenAI | v0.16.0 正式发布记录明确加入两类架构；当前 v0.17.0 提供原生多模态示例与 C/C++ API | 有无 Python 用户端路线；仍需选定 checkpoint、导出包、版本和设备做契约测试 |
| Qwen3-VL + llama.cpp | 官方仓库已有语言模型和视觉投影实现；多模态运行要求对应 GGUF 与 `mmproj` | 可评估打包原生 `llama-server`，减少 Python 环境管理成本 |
| Qwen-VL + 官方 PyTorch／服务后端 | 官方提供 Transformers、服务部署和图片／视频处理方式 | 原生运行包不成熟时，使用独立环境或外部服务；不影响上层业务流程 |

### Mage 的 ONNX 导出不等于 ORT GenAI 支持

Mobius 对 Mage 的说明指出，当前 ORT GenAI 配置不能表达其 packed visual-token gather 与 M-RoPE 预填充布局。导出器虽然能生成三个图，也明确拒绝生成 ORT GenAI 包；其源码还忽略 proactive gate 相关权重。

因此，Mage 的普通 ONNX 路线需要自己完成媒体预处理、视觉／文字拼接、位置与掩码、tokenizer、KV 缓存、生成循环和停止条件，并单独验证视频与流式能力。它不是现有 OCR 或对象检测适配器的简单权重替换。

### Qwen 的正式 ONNX 支持及其限制

ORT GenAI v0.16.0 于 2026-09-22 发布，记录中明确包含 Qwen2.5-VL 和 Qwen3-VL；v0.17.0 于 2026-09-28 发布。

v0.17.0 的 `model-mm.py` 示例接受图片和音频参数，没有视频输入参数。该证据确认多模态生成运行链，但不能用于宣称视频整段输入、原时间戳映射及事件定位已经完整打通。视频仍需要适配原片／帧序列、采样范围和时间信息。

支持矩阵列有 Windows、Linux、macOS，以及 CPU、CUDA、DirectML 等后端；这不是对任意模型包、操作系统和 EP 的所有组合承诺。应以选定组合的实际加载和推理结果为准。

### GGUF 路线

llama.cpp 提供原生多模态推理与 `llama-server`。模型包需要匹配的语言权重和 `mmproj`，不能随意拼接不同检查点的投影文件。其多模态文档包含视频输入，但每种架构、版本和视频处理方式仍需分别验收。

这条路线可以在终端用户没有 Python 的情况下工作；GGUF 转换工具使用 Python，不影响交付预转换模型包和原生运行程序。

## 与 Lap 现有体系的复用边界

当前 `src-tauri/Cargo.toml` 固定 `ort = 2.0.0-rc.10`。`ai/runtime.rs` 已负责普通 ONNX 会话、执行提供者、设备选择、回退与算子报告；`ai/assets.rs` 已负责模型资源生命周期。它们可以继续服务人脸、OCR、对象检测和图文向量。

生成式模型需要增加专用后端，不能认为 Rust `ort` 已自动提供 ORT GenAI 的生成循环。

| 可以复用 | 需要新增／扩展 |
| --- | --- |
| 模型清单、下载代理、SHA-256、进度、取消和原子安装 | 模型包要求的引擎、版本、平台、量化格式与组件角色 |
| 设备列表与普通 ONNX 会话经验 | 多模态生成、tokenizer／processor、生成缓存和时间映射 |
| 当前图库与源文件版本校验 | 生成任务、分析记录、标签／标题／说明提议与条件保存 |
| 通用网络客户端与系统凭据管理 | 生成式 HTTP 协议、媒体传输、增量响应和能力探测 |

当前 `ai/remote.rs` 是 Jina-compatible embedding 协议，不是通用对话／多模态生成客户端，不能直接用于 Mage 的聊天服务。

现有 CUDA bridge 已隔离两套 ORT 实例，并禁止跨实例混用对象或分配器。接入 GenAI 时同样要固定配套运行库；必要时采用独立 worker 或明确的原生边界，不能将独立下载的 ORT DLL 当作可任意互换组件。

## 后端选择规则

1. 已有经过验证的 ONNX／GenAI 包，且当前平台和设备组合可用：选择原生 ONNX／GenAI 后端。
2. 没有适用 ONNX 包，但对应架构有可用原生 GGUF 实现：可选择受管原生服务，例如 `llama-server`。
3. 模型完整功能依赖官方 PyTorch／定制组件：使用隔离的 Python worker，或连接已有服务。
4. 服务部署在另一台机器或第三方平台：使用 API 配置，本机不下载服务端权重，也不安装该服务端的 Python 环境。

模型“可导出”、引擎“支持架构”、部署“已配置”、本机“实测通过”是不同状态。当前机器设备不足只能使该本机部署不可用，不能删除模型目录中的其它平台方案。

## 模型、模型包、部署配置

需要分开记录：

- **模型定义**：家族、实际检查点、版本及能力契约。
- **模型包**：ONNX／GGUF／PyTorch、量化方式、所有权重和 tokenizer／processor／模板、引擎版本、资源哈希、允许的平台和设备，以及验收过的输入方式。
- **部署配置**：本机受管引擎或 API；API 再记录协议、地址、认证、模型 ID、实际服务版本、执行位置及媒体传输方式。
- **用途绑定**：图片理解、视频理解、文本生成、文本向量、多模态向量等分别引用部署配置。

同一 Mage／Qwen 检查点可以同时配置“本机”和“工作站 API”两条部署。模型资源可以按内容去重，但不同服务版本或量化／处理契约不能只因模型名字相同就混用缓存。

## 本地环境与进程管理

本机模式的资源安装应区分“运行时包”和“模型包”。Python 路线还需：

- 按引擎和版本隔离环境，锁定 Python、PyTorch、依赖和 CUDA 组合，不向用户全局 Python 安装依赖。
- 同一引擎版本可复用基础环境，模型权重独立管理，避免每个模型重复安装一套 PyTorch。
- 引导安装、校验、启动、就绪测试、运行日志、停止和失败恢复；服务只准备好后才允许提交推理任务。
- 只执行已适配、固定版本的后端代码；模型清单不承载可任意执行的安装脚本。
- 父进程管理明确归属的 worker，不因进程名相同而结束用户已有服务。
- 本机受管服务默认绑定回环地址；服务器部署的监听范围和认证由明确配置决定。

原生服务与 Python 服务可以共享 Lap 的 HTTP 推理适配器；进程管理器负责本机安装和生命周期，协议适配器负责请求、响应、取消和结果契约。

## API 必须验证的边界

- 模型列表（没有时允许手填 ID）、认证、文字请求、图片请求、视频处理方式和向量接口分别校验，不能仅凭“OpenAI-compatible”推定全部能力。
- 视频可能上传原片由服务端处理，也可能由客户端抽帧并带时间信息；服务必须能读取实际媒体，不能把本机路径当成远端可访问的文件。
- Mage 官方 API 目前采用抽样帧；需要 codec-native 或主动流式能力时使用相应适配路径并单独声明。
- API 模式默认不下载本机权重。远端资源安装只有服务明确提供管理接口时才可委托，不假设通用推理 API 支持模型下载。
- “仅本机”模式不自动转发内网或公网服务；本机、指定内网和外网是分别控制的数据边界。
- 回环地址不自动证明推理发生在本机；可能转发云端的本地 API 服务需要另行声明实际数据边界。

## 建议的验证顺序

1. 先完成通用生成 HTTP 适配和部署配置：用已部署的 Mage／Qwen 服务验证单图、片段、结构化结果、取消和错误处理。
2. 选定一个具体 Qwen checkpoint，比较 ORT GenAI 与原生 GGUF 的可交付模型包：同一组公开样片、同一任务与采样方案，记录正确性、内存／显存、实际设备和平台条件。先交付一条验收通过的本机路线。
3. 为 Mage 固定官方 Python 与服务代码版本，验证用户端受管环境；保持服务端部署也可独立使用。专用 ONNX 后端作为单独研究项。
4. 接入现有离线分析队列、结果索引和条件写入。模型可运行不等于标题、标签、说明及片段定位业务已经验收。

所有能力都需要图片／视频公共夹具及实际执行测试；“EP 已注册”“导出成功”或“页面显示就绪”不能替代推理验证。本轮尚未完成这些测试。

## 官方依据

- [Mage-VL 使用说明：离线、编码视频、SGLang 与主动流式路径](https://github.com/microsoft/Mage/blob/main/mage_vl/README.md)
- [Mage-VL 官方 Python 依赖](https://github.com/microsoft/Mage/blob/main/mage_vl/requirements.txt)
- [Mobius：Mage 的 ONNX 导出与 ORT GenAI 限制](https://github.com/onnxruntime/mobius)
- [Mobius Mage 导出器源码](https://github.com/onnxruntime/mobius/blob/main/src/mobius/models/mage_vl.py)
- [ORT GenAI 正式发布记录](https://github.com/microsoft/onnxruntime-genai/releases)
- [ORT GenAI v0.17.0 多模态运行示例](https://github.com/microsoft/onnxruntime-genai/blob/v0.17.0/examples/python/model-mm.py)
- [ORT GenAI 支持矩阵和原生 API](https://github.com/microsoft/onnxruntime-genai)
- [Qwen3-VL 官方推理与部署说明](https://github.com/QwenLM/Qwen3-VL)
- [llama.cpp 多模态输入与模型包](https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md)
- [llama.cpp Qwen3-VL 语言模型实现](https://github.com/ggml-org/llama.cpp/blob/master/src/models/qwen3vl.cpp)
- [llama.cpp Qwen3-VL 视觉投影实现](https://github.com/ggml-org/llama.cpp/blob/master/tools/mtmd/models/qwen3vl.cpp)

`main`／`master` 链接代表本轮调查到的上游实现；正式交付时还需固定实际使用的标签、提交和资源哈希。

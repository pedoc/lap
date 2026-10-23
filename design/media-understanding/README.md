# Lap AI 媒体理解设计预览

本版按用户提供的 **1437×833 浅色设置截图**修订。设置图直接使用当前 `Settings.vue`、`AiModelSettings.vue`、`CudaRuntimeSettings.vue`、`TitleBar.vue` 的模板及当前前端构建出的 CSS；组件脚本和 Tauri 调用由样例状态代替。

AI 模型默认仍打开原图文语义检索与 CLIP 配置。原四项任务保持原位，**只追加一个媒体理解标签页**：上方管理多个 OpenAI 兼容服务及每个服务的多个模型，下方分别选择媒体理解、语言生成、Embedding 默认模型。服务名称与任意部署平台无关。首版只接 API，Mage／Qwen 本机运行延后；现有本机识别不受此范围调整影响。600×620 作为最小窗口另外验证。

先看 [媒体理解原位扩展](17-media-model.png) 与 [现状／原位扩展对照](settings-comparison.png)，再打开 [全部设计图](gallery.html)。PNG 仅导出应用窗口，设计说明单独位于 [NOTES.md](NOTES.md)。交互原型使用当前前端依赖及生成的样式，先运行下方渲染命令，再打开 `index.html`，无需启动 Lap 或模型服务。

- [BASELINE.md](BASELINE.md)：当前源码布局、证据和组件对应关系。
- [SPEC.md](SPEC.md)：业务场景、模型抽象、索引与问答闭环、数据边界、开发顺序及验收条件。
- [生成式后端调查](../../docs/development/generative-backends.md)：本机后端、资源包与 API 的前期调查。
- [标准 API 与配置调查](../../docs/development/ai-service-protocols.md)：单页配置、OpenAI 接口与实际 Mage／Qwen 服务路径；包含历史协议调查。
- [远程 GPU 部署工具](../../scripts/ai-server/README.md)：固定版本安装、启动和协议探测；尚待真实 GPU 验收。
- [Rust SDK 可行性与选型](../../docs/development/ai-client-libraries.md)：成熟客户端对比、Tauri IPC、现有依赖边界与实际回环检查。

## 设计图

| 界面 | 文件 |
| --- | --- |
| 当前设置布局复核（源码重建） | [00](00-current-settings.png) |
| AI 模型默认入口原位扩展 | [01](01-model-management.png) |
| 媒体理解：多个服务、模型、三个默认选择 | [17](17-media-model.png) |
| 同页展开服务连接配置 | [08](08-api-connection.png) |
| 模型 ID、能力标记与参数 | [16](16-model-properties.png) |
| 同一页面的最小窗口适配 | [19](19-media-compact.png) |
| 理解与内容索引工作区 | [05](05-library-workspace.png) |
| 现有多选面板中的入口 | [15](15-selection-entry.png) |
| 选中媒体批量处理对话框 | [04](04-batch-fill.png) |
| 当前图库批量处理对话框 | [06](06-library-batch.png) |
| 图片理解与资料采用 | [02](02-image-viewer.png) |
| 图片问答与当前图片复核 | [10](10-image-qa.png) |
| 视频片段问答与时间引用 | [03](03-video-viewer.png) |
| 原搜索面板增加自然语言模式 | [11](11-natural-search.png) |
| 阶段进度与部分失败 | [12](12-task-progress.png) |
| 三条业务链路与持久数据 | [13](13-flow-guide.png) |
| 全部页面总览 | [overview](overview.png) |

## 可演示的交互

- 顶部评审导航切换全部 16 个视图；产品设置只新增媒体理解一个标签页，页内完成服务、模型及默认选择。
- 展开服务配置名称、地址与密钥；模型只配置 ID、能力和参数，连接检测不能代替能力检测。
- 仅使用 OpenAI Chat Completions 与文本 Embeddings；界面不再提供协议选择、本机生成引擎和资源下载。
- 默认用途可复用同一部署；任务创建后保留当时的配置快照。
- 选中、筛选、当前图库三个处理范围；生成向量和填充标题、说明、标签可分别选择。
- 采用标题／说明、拒绝标签、采用其余标签；操作仅改变原型内存状态。
- 图片提问 → 既有证据不足 → 仅复核当前图片 → 回答引用证据；不自动写正式资料。
- 视频证据跳转到原片时间示意；搜索仅读取索引，不创建媒体分析任务。
- 任务停止／恢复和仅重试向量示意。

原型不是完整业务实现：非关键按钮只显示示例反馈。模型编辑、下载、推理、视频播放、真实条件解析和检索均未接入；刷新清除示例状态。API 示例地址不会被请求。

## 设计约定

媒体理解、语言生成、Embedding 是用途与能力，**不是互斥的模型类别，也不是三个标签页**。一个多模态生成模型可承担前两项；Embedding 独立声明向量契约。首版使用文本向量建立分析描述索引；多模态 Embedding 保留能力标记，未验收服务的输入扩展时保持不可选。服务来源在抽象上分 API 与本机，首版只实现 API；服务器部署、模型下载和运行环境由服务器侧工具负责。

离线分析保存可读的结构化证据；Embedding 产生检索索引；自然语言搜索检索已有数据。单媒体问答在当前范围内使用证据，必要时复核当前媒体，生成新的回答。分析、向量、会话与正式用户资料分别保存，只有采用或显式填充任务才写正式资料。

## 渲染与检查

使用 Node 22+、已安装的 Edge／Chrome，以及 `src-vite` 已安装的前端依赖。首次先在 `src-vite` 运行 `pnpm install`，然后：

```powershell
node design/media-understanding/render.mjs
```

脚本先将当前前端构建到临时目录，提取真实 CSS 和四个组件模板，写入被 Git 忽略的 `native-source.generated.js`。模板中的后端状态与处理器由安全的样例替代，不加载真实组件脚本、不调用 Tauri。生成文件不提交；源模板和生产 CSS 保持原样。

可用 `LAP_DESIGN_BROWSER` 指定浏览器。主设置图按用户截图为 **1437×833 浅色窗口**，另外验证 **600×620**；其他主窗口图为 1536×850。保留当前原生滚动条。评审导航、右侧说明和页脚不进入 PNG。结果与样式来源哈希见 [render-check.json](render-check.json)。

检查包括：脚本异常、图片加载、横向溢出、原设置分类顺序、两种窗口尺寸、原 CLIP 默认入口、仅新增一个标签页、多个服务与模型、三个默认项、能力过滤及失效、显式采用、标签拒绝、问答范围、视频时间跳转、搜索不创建分析任务、任务配置快照。它不代表真实 Tauri 后端、数据库、模型或设备验收。

## 素材与范围

照片复用 `../home/assets/`，来源见 [photo-sources.txt](../home/photo-sources.txt)。`assets/building-detail.jpg` 是既有照片的局部裁剪，本轮没有下载新的照片或模型。

本轮只修改设计原型、导出图和开发说明；未修改生产 UI、AI 后端、真实模型配置或图库。

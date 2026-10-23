# Lap AI 媒体理解设计预览

本版参考 Cherry Studio 改为“模型服务 → 服务内模型 → 默认模型”，同时以当前 Lap 界面为基础。设置保持独立 **600×620** 窗口、原左侧分类和右侧滚动内容区；业务入口沿用文件信息、多选面板、对话框和搜索面板。

先看 [现状与修订并排对照](settings-comparison.png)，再打开 [全部设计图](gallery.html) 或 [交互原型](index.html?screen=models)。直接用浏览器打开即可，无需启动 Lap。

- [BASELINE.md](BASELINE.md)：当前源码布局、证据和组件对应关系。
- [SPEC.md](SPEC.md)：业务场景、模型抽象、索引与问答闭环、数据边界、开发顺序及验收条件。
- [生成式后端调查](../../docs/development/generative-backends.md)：本机后端、资源包与 API 的前期调查。
- [标准 API 与配置调查](../../docs/development/ai-service-protocols.md)：Cherry Studio 源码、三种生成协议、实际 Mage／Qwen 服务路径。
- [远程 GPU 部署工具](../../scripts/ai-server/README.md)：固定版本安装、启动和协议探测；尚待真实 GPU 验收。

## 设计图

| 界面 | 文件 |
| --- | --- |
| 当前设置布局复核（源码重建） | [00](00-current-settings.png) |
| 模型服务列表 | [01](01-model-management.png) |
| 本地模型与资源下载入口 | [07](07-local-runtime.png) |
| 服务连接与协议 | [08](08-api-connection.png) |
| 默认模型绑定 | [09](09-task-routing.png) |
| 服务下的模型列表 | [14](14-model-catalog.png) |
| 模型 ID、能力与接口 | [16](16-model-properties.png) |
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

- 顶部导航切换全部 17 个视图；设置原分类固定，AI 页内提供模型服务、默认模型、本地模型。
- 服务列表进入连接与模型；模型页不重复密钥；协议切换预览实际请求路径，连接检测不能代替模型能力验证。
- 本机 Mage／Qwen 示例包按包过滤引擎，模拟资源准备；API 保存与测试分别演示。
- 默认用途可复用同一部署；任务创建后保留当时的配置快照。
- 选中、筛选、当前图库三个处理范围；生成向量和填充标题、说明、标签可分别选择。
- 采用标题／说明、拒绝标签、采用其余标签；操作仅改变原型内存状态。
- 图片提问 → 既有证据不足 → 仅复核当前图片 → 回答引用证据；不自动写正式资料。
- 视频证据跳转到原片时间示意；搜索仅读取索引，不创建媒体分析任务。
- 任务停止／恢复和仅重试向量示意。

原型不是完整业务实现：非关键按钮只显示示例反馈。模型编辑、下载、推理、视频播放、真实条件解析和检索均未接入；刷新清除示例状态。API 示例地址不会被请求。

## 设计约定

媒体理解、语言生成、Embedding 是用途与能力，**不是互斥的模型类别**。一个多模态生成模型可承担前两项；文本／多模态 Embedding 独立声明向量契约。服务、服务内模型、默认用途分开管理，本机资源继续使用适配包。

离线分析保存可读的结构化证据；Embedding 产生检索索引；自然语言搜索检索已有数据。单媒体问答在当前范围内使用证据，必要时复核当前媒体，生成新的回答。分析、向量、会话与正式用户资料分别保存，只有采用或显式填充任务才写正式资料。

## 渲染与检查

使用 Node 22+ 与已安装的 Edge／Chrome，无额外 npm 依赖：

```powershell
node design/media-understanding/render.mjs
```

可用 `LAP_DESIGN_BROWSER` 指定浏览器可执行文件。脚本以独立临时配置启动无界面浏览器，导出 1600×1000 设计图、总览和设置对照，结果见 [render-check.json](render-check.json)。

检查包括：脚本异常、图片加载、横向溢出、主要操作裁切、600×620 设置尺寸与分类顺序、用途切换、资源选项、显式采用、标签拒绝、问答范围、视频时间跳转、搜索不创建分析任务、任务配置快照。它不代表真实 Tauri 界面、数据库、模型或设备的验收。

## 素材与范围

照片复用 `../home/assets/`，来源见 [photo-sources.txt](../home/photo-sources.txt)。`assets/building-detail.jpg` 是既有照片的局部裁剪，本轮没有下载新的照片或模型。

本轮只修改设计原型、导出图和开发说明；未修改生产 UI、AI 后端、真实模型配置或图库。

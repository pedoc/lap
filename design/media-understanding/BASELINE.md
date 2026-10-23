# 当前界面依据

核对日期：2026-10-23。设置基准更新为用户附图的 **1437×833 浅色界面**。`00-current-settings.png` 直接渲染当前 Vue 模板和前端 CSS，模型及设备状态用附图样例填充；不运行组件脚本或真实 Tauri 后端。颜色、控件、字号与间距来自项目，不再使用原型自己的深色配色。

## 设置窗口

| 已核对的现状 | 源码依据 | 本次设计处理 |
| --- | --- | --- |
| 单独的 `settings` WebviewWindow；再次打开优先显示并聚焦已有窗口 | [Home.vue](../../src-vite/src/views/Home.vue#L806) | 保留独立窗口，不移入主窗口 |
| 初始与最小尺寸按 600×620 和设置比例计算；不可最大化，创建参数允许原生调整大小 | [Home.vue](../../src-vite/src/views/Home.vue#L845) | 主图使用附图宽窗口 1437×833，另外检查最小窗口，不能把初始尺寸当作固定布局 |
| 设置缩放同时调整字体、窗口尺寸和最小尺寸 | [Settings.vue](../../src-vite/src/views/Settings.vue#L1360) | 沿用缩放机制 |
| 40px 自定义标题栏；设置页传入 `resizable=false`，Windows 标题栏只显示关闭 | [Settings.vue](../../src-vite/src/views/Settings.vue#L4)、[TitleBar.vue](../../src-vite/src/components/TitleBar.vue#L7) | 保留标题栏形态；不使用 macOS 三色按钮 |
| 左侧 `w-40 m-1 p-2`，右侧 `p-2 mr-1 mb-2` 独立滚动 | [Settings.vue](../../src-vite/src/views/Settings.vue#L8) | 左栏 160px，保留间距与右侧单列滚动 |
| 分类依次为通用、网络、布局、查看器、RAW、AI 模型、高级、快捷键、关于 | [Settings.vue](../../src-vite/src/views/Settings.vue#L599)、[中文翻译](../../src-vite/src/locales/zh-CN.json) | 原分类与顺序不变；不增加第二套 AI 左导航 |

## AI 模型页

来源：[AiModelSettings.vue](../../src-vite/src/components/AiModelSettings.vue)。

当前顺序为：介绍 → 图文检索／人脸／OCR／对象任务按钮 → 模型选择与当前全局模型 → 该模型配置、设备、参数、保存并测试 → 模型目录／下载 → 导入清单。设置切换和关闭还会处理未保存草稿。

修订稿仅扩展这个区域，参考 Cherry Studio 调整配置组织：

- 默认入口、原四项任务、CLIP 模型选择与配置保持原位；任务栏只追加媒体理解。
- 媒体理解页上方管理多个 OpenAI 兼容服务的地址、凭据和模型；模型可标记多项能力。
- 同页下方分别选择媒体理解、语言生成、Embedding 默认模型，不再拆成三个标签页。
- `17-media-model.png` 展示整页；`08-api-connection.png` 展示同页展开连接；`16-model-properties.png` 展示能力详情。本机生成模型与多协议选择不在本期界面出现。
- 保存与测试按钮保留在原表单中，随内容滚动，不新增固定底栏。保存参数、测试、启用继续保持不同含义。

渲染入口为 `prepare-native.mjs` 与 `native-frame.js`，复用 Settings、TitleBar、AiModelSettings、CudaRuntimeSettings 模板。新增媒体理解表单在设计夹具中使用实际组件样式与原设置外壳；生产文件未修改。

当前后端并不是通用生成式模型系统：配置按模型 ID 保存，每任务一个全局选择；远程适配器目前是 Jina embedding 协议。新增部署、多能力和生成式 API 都属于后续开发，见 [SPEC.md](SPEC.md)。

## 其他入口

| 当前组件 | 已有结构 | 设计中的增量 |
| --- | --- | --- |
| [Home.vue](../../src-vite/src/views/Home.vue#L406) | 64px 图标栏＋动态左侧组件面板，默认展开总宽约 260px | 新增“媒体理解”业务面板，遵循 OCR／对象等现有模式；示意图省略部分无关图标 |
| [FileInfo.vue](../../src-vite/src/components/FileInfo.vue#L426) | 可折叠常规资料、OCR、对象等信息 | 增加媒体理解区块，包含分析与针对当前媒体的问答 |
| [ImageViewer.vue](../../src-vite/src/views/ImageViewer.vue#L164) | 独立查看器右侧 FileInfo，宽 23rem、最大 48vw | 设计使用 368px 面板；保留现有文件信息与播放器 |
| [SelectionPanel.vue](../../src-vite/src/components/SelectionPanel.vue#L93) | 选择、整理、文件操作、更多操作分组 | 整理区增加“媒体理解”按钮，打开批量对话框 |
| [ModalDialog.vue](../../src-vite/src/components/ModalDialog.vue) | 居中、可拖动的应用内对话框 | 批量理解复用该形态，不单独开一套管理应用 |
| [ImageSearch.vue](../../src-vite/src/components/ImageSearch.vue#L12) | 左侧“文本／组合搜索”模式与历史 | 同处增加自然语言模式；结果继续进入现有主内容区 |

业务图是对这些组件位置的扩展设计，并非现有界面的逐像素截图。新增区块在开发时应使用当前主题变量、公共按钮和原有面板容器。

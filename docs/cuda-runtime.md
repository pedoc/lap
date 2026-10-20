# CUDA 执行组件

## 使用

- Windows x64 的“设置 → AI 模型”提供 CUDA 执行组件卡片。使用 NVIDIA 驱动枚举 CUDA 设备，不把 DirectML／虚拟显示适配器编号当成 CUDA 编号。
- 点击下载时显示约 1.7 GiB 的下载量；安装还需要解压空间。组件从官方 PyPI 发布的 ONNX Runtime／NVIDIA 包下载，逐包验证固定 SHA-256；支持应用代理、进度、取消、重开设置恢复，以及复用已完整校验的下载缓存。
- 点击“检测 CUDA”执行小型矩阵运算，校验数值结果和 `CUDAExecutionProvider` 算子记录。此结果不等于所有模型都兼容；请再测试具体模型。
- 首次加载大型库可能较慢；检测状态由后端保存，关闭后重开设置仍能看到状态，不重复启动检测，也不与组件安装同时进行。
- 本地模型可选择 CUDA 或自动。自动模式优先尝试 CUDA，再尝试 DirectML／CPU；显式指定 CUDA 时错误直接报告，不伪装成 GPU 成功。
- CUDA 设备编号、显存池上限和是否允许更大 cuDNN 工作区按模型保存。显存池上限对每个 ONNX 会话分别生效，不是整个进程的硬显存限额；部分 CUDA/cuDNN 分配不计入该池。默认使用保守的工作区和按需扩展策略，适合显存较小的设备。

不安装驱动、不修改全局 PATH、不上传照片。不要求用户安装完整 CUDA Toolkit。组件位于应用数据目录的 `runtimes/cuda/<catalog-sha256>`，第三方许可随组件保存。安装和修复使用暂存目录、校验回执及原子替换；加载代码前复查文件校验和。CUDA 不可用时会区分缺驱动、缺桥接模块、未安装和运算验证失败。

## 架构与构建

主程序保留原有静态 CPU／DirectML ONNX Runtime。`crates/cuda-bridge` 是默认安装包携带的小型 DLL，独立动态加载按需下载的 CUDA ONNX Runtime，避免两个 ORT 实例混用对象或分配器。跨 DLL 只交换有版本与大小限制的张量字节包，释放内存仍由分配它的模块负责。模型输入／输出、RapidOCR 字典元数据和执行报告通过同一会话接口接回已有模型适配器。

`node scripts/prepare-native-runtime.mjs` 构建桥接模块；Tauri hooks 自动调用。Windows 发布／PR 工作流另有显式准备步骤，因为 Windows CI 会覆盖前端构建命令。Linux/macOS hook 不构建桥接模块；原有后端不变。其他平台原有手动 `ai-cuda` feature 保留。

不将大型 CUDA/cuDNN 运行库加入默认发布包。运行库、桥接 DLL 和当前使用的 ORT 版本必须配套；换用另一个已加载的 CUDA runtime 版本时需要重启，不能在进程内随意替换已加载库。

## 验证

- 无硬件依赖：wire 协议、源码固定下载清单、安装路径／重复文件／取消／回执、前端状态恢复及错误分支回归。
- `cargo test --locked --bin Lap ai::cuda_assets::tests::` 默认不运行 GPU 测试。
- 真实 GPU 测试需设置工作区 `target` 内的 `LAP_CUDA_TEST_ROOT` 和 `LAP_AI_TEST_MODEL_ROOT`，再运行 `actual_cuda_models_use_the_optional_component_without_changing_profiles --ignored --nocapture`。可用 `LAP_CUDA_OCR_FIXTURE` 指向工作区 OCR 测试图。只使用合成／夹具图片，不修改图库。
- Windows x64／GTX 1650（4 GiB）实测：RapidOCR PP-OCRv5 Mobile 读出中英文夹具，YOLOX-S 检出测试图中的人和公交车；算子记录确认使用 CUDA。相同对象夹具的 CPU／DirectML 结果也通过。生产解压器从固定官方包提取 21 个 DLL 并保留第三方许可。
- 尚未验收新安装包、原生设置窗口的完整下载操作或全部目录模型的 CUDA 兼容性；未向用户应用数据目录安装测试组件。桥接当前支持 f32/f64/i64/i32/u8/bool 张量，不承诺任意第三方导出的类型或算子均可加速。
- 本次不包含多图批推理或 CPU/GPU 流水线改造；支持 CUDA 不代表 GPU 利用率必然达到 100%。

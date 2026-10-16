# 对象识别（第一阶段）

本轮范围：本地闭集对象检测、版本化类别、下载与设备选择、结果浏览和人工修订、数量条件与智能相册。暂不包含开放词汇检测、分割、细粒度身份识别或视觉问答。

## 模型与来源

| 模型 | 原生契约 | 权重来源 | 固定 SHA-256 |
| --- | --- | --- | --- |
| YOLOX-S | RGB 0–255；640px 左上角 letterbox；`[1,8400,85]` 网格输出；同类 NMS | OpenCV Zoo `47534e27c9851bb1128ccc0102f1145e27f23f98` | `c5c2d13e59ae883e6af3b45daea64af4833a4951c92d116ec270d9ddbe998063` |
| RT-DETRv2 R18 | RGB 0–1；640px 拉伸；logits + normalized cxcywh | ONNX Community `936f90b6a476c6da4dfe053fc521af55285976ba`，基于 PekingU 权重 | `583a236ac21c95a7fd94f284fc21485e42355bfef82c27011ba78fbc09ee87e2` |

两者均声明 Apache-2.0。YOLOX 来源许可见 `model-licenses/YOLOX-Apache-2.0.txt`；RT-DETR 项目许可见 `model-licenses/RT-DETR-Apache-2.0.txt`（项目提交 `df2a96c8c3a098567a1149bf0248da6efe6d495d`）；导出模型卡：[固定版本](https://huggingface.co/onnx-community/rtdetr_v2_r18vd-ONNX/tree/936f90b6a476c6da4dfe053fc521af55285976ba)，[原模型](https://huggingface.co/PekingU/rtdetr_v2_r18vd)。许可核对只针对这里列出的具体实现和权重，不代表所有 YOLO／检测模型都采用相同授权。模型按需下载到应用模型目录，不随安装包打包；地址固定版本并验证大小和 SHA-256，沿用应用代理。

## 类别与结果契约

首批模型均提供 COCO 80 类，不支持任意概念、车牌、票据分类或对象身份。类别键由标签集命名空间、版本和原始类别 ID 组成；相同命名空间版本不允许声明冲突映射。未知第三方类别保留原始名称，不强行套用穷举词典。

检测结果使用原图归一化坐标，保存模型／适配器／参数／标签集指纹。分数不跨模型比较，不是校准概率；达到候选或数量上限会标为不完整，不能把截断结果当作确认未检出。GPU 自动策略和显式设备选择沿用既有运行时，自动执行失败可回退 CPU 并显示原因，显式 GPU 不静默更换。

## 实施记录

- [x] 模型抽象、原生适配、固定下载源和设备验证（两模型 CPU／DirectML 原生样片通过）。
- [ ] 对象索引、原图版本约束、人工结果保护和资源协调。
- [ ] 对象入口、照片操作、计数和智能相册。

桌面交互验收需要可连接的运行实例，构建／自动化测试不等价于实机验收。测试模型和公开样片仅放在忽略的 `src-tauri/target/objects-validation/`，不提交照片或权重。

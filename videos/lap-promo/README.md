# Lap 产品宣传片

约 33.2 秒的横屏产品片：**找到 → 看懂 → 整理 → 重访 → 大屏分享 → 掌控**。中文标题、原生产品界面、电子配乐、少量电影音效；没有旁白。用户已选择自主创作，直接完成成片。

| 文件 | 用途 |
| --- | --- |
| `out/lap-promo-4k.mp4` | 主片，3840×2160、30fps、H.264 / AAC |
| `out/lap-promo-1080p.mp4` | 方便观看和分享的 1080p 版本 |
| `out/lap-promo-4k-nobgm.mp4` | 同时间线、保留音效、关闭 BGM，方便自行配乐 |
| `out/qa/encoded-contact-sheet.jpg` | 从实际成片抽取的九宫格 |
| `out/qa/delivery-verification.json` | 编码、逐版画面一致性、音画时间与峰值证据 |
| `out/qa/independent-review.md` | 独立审查报告 |

精确时长为 997 帧 / 30fps，即 33.233 秒。容器内音频尾部可能多出不足一帧的 AAC 长度。

## 预览与编辑

在本目录执行（Node 22+，本次使用 Node 24）：

```powershell
npm ci
npm run dev
```

Remotion Studio 中打开 `Lap-Promo-4K`。8 个镜头都有独立轨道和 `Scenes` 里的独立合成，可修改文案、时长和位置；BGM 可通过输入属性 `bgm` 开关。本次 Studio 开在 `http://localhost:3027/Lap-Promo-4K`。

```powershell
npm run lint
npm run verify:timeline
npm run render
npm run render:nobgm
```

`public/` 已冻结所有渲染依赖，不需要运行 Lap、读取相册或调用模型。`scripts/prepare-assets.py` 是本次机器上的素材采集记录，重新执行需要仓库 `.codex/readme-demo-media` 和本机 video-shotcraft 音频库；日常编辑、渲染直接使用冻结的 `public/` 即可。

`npm run qa:frames` 导出镜头与转场关键帧。三个 MP4 都存在后，运行 `npm run verify:delivery` 核对实际编码文件。所有构建、视频与 QA 产物都在忽略目录，不纳入 Git。

无配乐版的声音通过 `bgm=false` 从 Remotion 同一时间线独立渲染为 WAV。`render:nobgm` 在最终封装时复用主片图像流，保证两个版本逐帧完全一致；它使用独立渲染的音效，没有从混合音乐中移除或分离声音。音效原始渲染保留在 `out/qa/nobgm-sfx.wav`。

## 功能与制作依据

- 语义检索与组合筛选，本地 OCR 和物体识别，媒体对话，智能相册，原生照片地图、DLNA 电视投屏。
- 真实 Windows 截图及实际 OCR / RF-DETR / 模型问答结果，出处见仓库 `docs/public/screenshots/current/README.md`。
- “媒体对话需自行配置模型服务”“地图服务需联网”已写入对应镜头。DLNA 投屏注明同一局域网、兼容 DLNA 的电视，画面为效果示意，未声称电视实机录像。
- 已按用户要求删除片尾 GitHub 链接；片尾只有品牌与产品主张。
- 4K 是输出分辨率；原生界面截图为 1440×1080。中文标题按输出分辨率重新绘制，原生界面没有 AI 重绘。
- 摄影、图标和音频来源见 [ASSETS.md](ASSETS.md)，分镜、品牌 tokens 与取舍见 [DESIGN.md](DESIGN.md)。
- 字体使用 Windows 已安装的 Microsoft YaHei；其他平台可安装 Noto Sans SC 后复核版式。工程不分发商业字体文件。

## 后续使用

本片保留 `src/workbench.ts` 供镜头工作台读取，也可按需要导出剪映工程，继续改字幕、镜头顺序或配乐。

本机附带工作台的示例索引生成器存在语法错误，当前使用 Remotion Studio / CLI 预览和导出。验证证据与范围见 [QA.md](QA.md)。

制作使用 remotion-create 与 video-shotcraft。发布时可自愿提及 video-shotcraft 作者：X [@VincentWei93](https://x.com/VincentWei93)、抖音 [@Vincent](https://www.douyin.com/user/MS4wLjABAAAAK1pkjBxilk2Oi_9h_vFyD-lTAu9CTlvhmOtkosDvvxg)、小红书 [@Vincent](https://xhslink.cn/m/At9iP2d5C1V)。这是自愿致谢。

也可把成片提交到[作品展示页](https://vincentwei1021.github.io/video-shotcraft/showcase.html)，在[提交表单](https://github.com/Vincentwei1021/video-shotcraft/issues/new?template=showcase.yml)里留下自己的账号，方便其他人找到你。

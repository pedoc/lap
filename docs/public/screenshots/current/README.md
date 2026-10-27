# 当前版本 README 截图与素材来源

本目录保存 2026 年 10 月 27 日从 Lap **0.4.9 当前源码**运行的 Windows 桌面应用中截取的真实界面。应用使用 Tauri / WebView2，加载生产构建的前端；截图尺寸为 **1440 × 1080**。界面没有重绘、拼接或替换区域，最终文件仅由原始 PNG 无损转换为 WebP。

截取时的应用源码基线为 `0450b930`，版本号来自应用配置，不代表已发布版本。

These are genuine screenshots of Lap 0.4.9 running natively on Windows with the production frontend. Only CC0 demo photos are used. The UI is unchanged; PNG captures were converted losslessly to WebP.

## 截图内容

| 文件 | 展示内容 |
| --- | --- |
| [home.webp](home.webp) | 图库首页、时间轴及往年今日 |
| [smart-albums.webp](smart-albums.webp) | 按规则组织的智能相册 |
| [combined-filters.webp](combined-filters.webp) | 组合条件筛选与结果预览 |
| [media-chat.webp](media-chat.webp) | 媒体理解中的两图问答 |
| [objects.webp](objects.webp) | 本地物体识别与图片中的检测结果 |
| [map.webp](map.webp) | 按地理位置浏览示例照片 |

## 示例图库与真实性边界

截图使用独立的 `LAP_TEST_DATA_DIR` 和 `WEBVIEW2_USER_DATA_FOLDER`，从公开摄影素材建立示例图库，没有使用个人相册。导入共 **26 个文件：24 张不同的 CC0 照片，以及其中两张照片的逐字节重复副本**。

评分、收藏、标签，以及示例副本中的日期、相机和镜头字段是为了展示功能而设置的数据，不代表原摄影作品的真实拍摄时间或器材。示例日期覆盖 2023–2026 年，并包含往年 10 月 27 日。仅在来源页面提供明确坐标时写入 GPS；9 张不同照片的坐标来自对应 Wikimedia Commons 文件页。

`media-chat.webp` 展示在实际配置的模型上进行两图问答的真实结果；模型输出不代表对照片地点或事实的独立核验。`objects.webp` 展示本地 RF-DETR 的真实推理结果。公开资产中不包含 API 密钥、私有服务地址、临时数据库或个人媒体。

## 重新截取

1. 根据下表来源下载示例照片，并核对对应文件页的许可；为副本设置展示用日期等元数据，保留有来源依据的 GPS。
2. 使用独立数据目录和独立 WebView2 用户目录启动当前源码构建的 Lap 桌面应用，加载生产前端。
3. 导入示例照片和两个重复副本，在应用中设置评分、收藏和标签，建立演示所需的智能相册及筛选条件。
4. 实际打开各功能页面；媒体问答需要配置可用模型，物体识别需要安装并运行本地模型。
5. 等待缩略图、模型结果和地图加载完成，在 1440 × 1080 的应用内容区域截图，保留原始 PNG 并无损转换为 WebP。
6. 复核画面中的照片来源、地图署名和隐私信息，仅将最终截图及本说明纳入版本控制，不提交临时原图、数据库和凭据。

## 摄影素材来源

下表列出本次使用的全部 24 张不同照片。许可已从 Wikimedia Commons 文件页的许可元数据核对为 [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/)。其中标有 Unsplash 的作品使用的是 Commons 记录的早期 CC0 授权，具体来源及许可记录以各文件页为准。下表文件名是示例副本名称。

| 示例副本 | 摄影者 / 来源署名 | 原始作品及许可记录 |
| --- | --- | --- |
| `2023-03-12_01_mountain-wilderness.jpg` | David Marcu davidmarcu | [Alone in the unspoilt wilderness (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Alone_in_the_unspoilt_wilderness_(Unsplash).jpg) · CC0 |
| `2023-03-13_02_lofoten-norway.jpg` | Johannes Groll followhansi | [Lofoten, Norway (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Lofoten,_Norway_(Unsplash).jpg) · CC0 |
| `2023-07-18_03_summer-portrait.jpg` | Autumn Goodman auttgood | [Mother Nature, Summer (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Mother_Nature,_Summer_(Unsplash).jpg) · CC0 |
| `2023-07-19_04_matterhorn-sunset.jpg` | Sam Ferrara samferrara | [Matterhorn sunset 2016 (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Matterhorn_sunset_2016_(Unsplash).jpg) · CC0 |
| `2023-10-27_05_atacama-bus.jpg` | Pablo Garcia Saldaña pagsa_ | [Abandoned bus in San Pedro de Atacama (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Abandoned_bus_in_San_Pedro_de_Atacama_(Unsplash).jpg) · CC0 |
| `2023-10-27_06_toronto-skyline.jpg` | Scott Webb scottwebb | [CN Tower, Toronto, Canada (Unsplash DJ kOgH5u0o).jpg](https://commons.wikimedia.org/wiki/File:CN_Tower,_Toronto,_Canada_(Unsplash_DJ_kOgH5u0o).jpg) · CC0 |
| `2024-03-12_07_countryside-dog.jpg` | Janko Ferlič thepootphotographer | [Sheep dog and fence Lipoglav (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Sheep_dog_and_fence_Lipoglav_(Unsplash).jpg) · CC0 |
| `2024-03-13_08_garden-light.jpg` | Artsy Vibes artsyvibes | [Boho garden (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Boho_garden_(Unsplash).jpg) · CC0 |
| `2024-07-18_09_city-street.jpg` | Anthony Indraus aindraus | [Bleak city street (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Bleak_city_street_(Unsplash).jpg) · CC0 |
| `2024-07-19_10_field-adventure.jpg` | Linh Nguyen linhnguyen | [Boy running in field (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Boy_running_in_field_(Unsplash).jpg) · CC0 |
| `2024-10-27_11_plants-and-books.jpg` | Scott Webb scottwebb | [Plants and Books (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Plants_and_Books_(Unsplash).jpg) · CC0 |
| `2024-10-27_12_forest-bridge.jpg` | Marc-Antoine Dépelteau autumnsgrief | [Bridge into a green thicket (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Bridge_into_a_green_thicket_(Unsplash).jpg) · CC0 |
| `2025-03-12_13_blue-reading-corner.jpg` | Ben Garratt wstn | [Bookshelf and blue couch (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Bookshelf_and_blue_couch_(Unsplash).jpg) · CC0 |
| `2025-03-13_14_city-apartments.jpg` | chuttersnap chuttersnap | [Brown Apartment Building (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Brown_Apartment_Building_(Unsplash).jpg) · CC0 |
| `2025-07-18_15_blue-and-yellow.jpg` | Tim Gouw punttim | [Blue truck and yellow building (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Blue_truck_and_yellow_building_(Unsplash).jpg) · CC0 |
| `2025-07-19_16_frozen-bubble.jpg` | aaronburden Aaron Burden from Baltimore, Maryland, USA. | [Frozen soap bubble behind fir twigs (Unsplash BojuZpqw4zM).jpg](https://commons.wikimedia.org/wiki/File:Frozen_soap_bubble_behind_fir_twigs_(Unsplash_BojuZpqw4zM).jpg) · CC0 |
| `2025-10-27_17_winter-landscape.jpg` | Maarten Deckers maartendeckers | [Winter (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Winter_(Unsplash).jpg) · CC0 |
| `2025-10-27_18_shanghai-roads.jpg` | Denys Nevozhai dnevozhai | [Yan'an East Road Interchange, Shanghai, China (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Yan%27an_East_Road_Interchange,_Shanghai,_China_(Unsplash).jpg) · CC0 |
| `2026-03-12_19_iowa-trail-bridge.jpg` | Tony Webster tonywebster | [High Trestle Trail Bridge, Madrid, Iowa, United States (Unsplash F9o7u-CnDJk).jpg](https://commons.wikimedia.org/wiki/File:High_Trestle_Trail_Bridge,_Madrid,_Iowa,_United_States_(Unsplash_F9o7u-CnDJk).jpg) · CC0 |
| `2026-03-13_20_iceland-waterfall.jpg` | Robert Lukeman robertlukeman | [Idyllic landscape with a waterfall (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Idyllic_landscape_with_a_waterfall_(Unsplash).jpg) · CC0 |
| `2026-07-18_21_morning-coffee.jpg` | Andy Li | [Breakfast at Coffee 1, North Street, Taunton 2025-07-28.jpg](https://commons.wikimedia.org/wiki/File:Breakfast_at_Coffee_1,_North_Street,_Taunton_2025-07-28.jpg) · CC0 |
| `2026-07-19_22_sunny-afternoon.jpg` | rawpixel.com rawpixel | [Smiling woman with a phone (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Smiling_woman_with_a_phone_(Unsplash).jpg) · CC0 |
| `2026-10-27_23_warm-city-facade.jpg` | Rene Bieder renebieder | [Brown apartment building (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Brown_apartment_building_(Unsplash).jpg) · CC0 |
| `2026-10-27_24_facade-pattern.jpg` | Osman Rana osmanrana | [Black and white facade (Unsplash).jpg](https://commons.wikimedia.org/wiki/File:Black_and_white_facade_(Unsplash).jpg) · CC0 |

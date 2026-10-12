# 地图与地点服务

## 当前实现与原问题

之前普通底图使用 OpenStreetMap，卫星底图使用 Esri World Imagery；天地图是另一组选项。天地图缺少 Token 或任一瓦片失败时会静默回退默认服务。照片地点由内置 GeoNames 近邻城市数据解析，不是在线街道地址。

现在将地图显示与地点解析作为独立能力配置，参考旅行项目的 renderer/data service 分离思路，但保留 Lap 的 Leaflet 照片聚合、视窗查询、缩放、主题与标记行为，不引入行程规划或三维地图。

## 配置与能力

入口：**设置 → 高级 → 地图与地点服务**，保存后主窗口及图片窗口立即应用。

| 服务 | 地图显示 | 地点解析 | 必要参数 |
| --- | --- | --- | --- |
| OpenStreetMap / Esri | 普通图 OSM，卫星图 Esri | — | 无 |
| Esri | World Street Map / World Imagery | — | 当前公共服务不要求 Key |
| 天地图 | 矢量／影像，各含注记层 | 支持 | 开发者 Token，授权 API／来源配置 |
| MapTiler | 栅格街道／卫星 | — | API Key，可选普通图样式 ID |
| Mapbox | 栅格街道／卫星 | — | `pk.` Public Token；普通及卫星图分别配置所有者和样式 ID |
| 自定义 XYZ／WMTS | 标准图，可选卫星图 | — | HTTP(S) URL、署名；可选 Token、子域、最大缩放 |
| 离线 GeoNames | — | 近邻城市，不保证精确地址 | 无 |
| 高德 | 官方 JavaScript SDK 2.0 | 逆地理编码 | 显示：JS Key + securityJsCode；解析：独立 Web 服务 Key |
| 腾讯位置服务 | 官方 JavaScript API GL | 逆地理编码 | 显示：JS Key；解析：Web 服务 Key，启用签名校验时另需 SK |

自定义模板必须包含 `{z}`、`{x}`、`{y}`，可使用 `{s}`、`{token}`。用户署名按纯文本转义，瓦片只接受有解码大小上限的 PNG／JPEG／WebP，不执行供应商 HTML／SVG。

有来源绑定时，Referer 可配置为**本应用获得授权的 HTTP(S) 地址**，不得用第三方地址绕过限制。接口权限、白名单、代理出口 IP、额度及商用条款仍由服务商控制；控制台链接在对应字段旁。Mapbox 原版字标来自参考项目安装的官方 `mapbox-gl` SDK CSS，仅在 Mapbox 地图中展示，商标及地图版权归原权利人。

## 网络与错误

瓦片通过 Rust IPC 获取图片，再以 Blob URL 展示；密钥不进入 WebView 的远程瓦片 URL。瓦片和在线逆地理编码使用现有应用代理，本地环回瓦片服务直连。地图瓦片并发上限 8；临时连接、超时或 5xx 最多重试两次，401／403／429 不自动反复重试。

错误保留当前供应商并显示诊断，可点击“重试当前服务”，不自动换到另一家公司。切换或卸载图层时取消尚未完成的瓦片请求、释放 Blob URL，并忽略旧图层迟到的错误。切换不重置地图实例、照片聚合或视图中心。

瓦片按服务端 Cache-Control／Expires 缓存在系统应用缓存目录的 `map-tiles` 下，以配置摘要及瓦片坐标区分；摘要不暴露密钥。OSM 无缓存头时使用其建议的七天缓存，ETag 支持重新验证。这里只缓存正常浏览请求，不提供离线整图下载／批量预取。自建服务及账号须允许对应地图使用和缓存。

## 地点与数据安全

图片信息中“重新解析拍摄地点”使用当前配置的地点服务；成功只更新图库数据库中的地点名称／行政区／国家，**不改写原图、EXIF、原始 GPS、脸框或人名**。图库、设置、GPS 或文件元数据版本在请求中改变时丢弃结果，失败不覆盖旧地点。更新通知复用地点侧栏的元数据刷新机制。

批量导入／元数据刷新默认仍用离线解析。用户勾选自动在线解析后才发送照片经纬度；可能减慢扫描并产生费用，失败使用离线近邻结果。切换服务不自动重算所有历史地点；可单独解析或使用既有的批量元数据刷新入口。原始坐标统一保存为 WGS84：高德请求单独转 GCJ02，腾讯通过 `coord_type=1` 接收原始 GPS，天地图使用原始坐标。

配置在用户目录的 `app-config.json`，按供应商保留非激活参数；前端持久化不包含新服务凭据。默认配置、文档、测试不含真实 Key。普通测试不使用用户保存的在线地理服务账号；代理传输测试使用本机临时 HTTP 代理与合成瓦片，不调用付费服务。

## 完整地图渲染器切换

- `mapRenderer.js` 是业务视图接口，`mapEngines.js` 是原生 SDK 适配层，`mapRasterEngine.js` 是独立栅格适配层。高德和腾讯分别实例化 **AMap.Map、TMap.Map**；栅格供应商才实例化 Leaflet。一个视窗只有一个活动地图，不再创建 SDK iframe、透明 Leaflet 覆盖层或双镜头同步。
- 公共坐标、查询边界和像素对象由 `mapGeometry.js` 提供，不依赖 Leaflet 类型；两个 Vue 地图视图不导入 Leaflet。只有选择栅格来源时才异步加载 Leaflet 及其 CSS；高德／腾讯路径无需加载 Leaflet。OSM／自定义 XYZ 本身是瓦片源，并不具有可切换的供应商官方 SDK。
- 高德照片由 **AMap.Marker** 承载，腾讯照片由 **TMap.DOMOverlay** 承载；位置、缩放、拖拽、旋转、俯仰均跟随当前 SDK 的原生投影。应用仍提供照片聚合、选图、双击预览、聚合打开和视野内数据库查询，这些是业务逻辑而不是第二套地图。
- 主照片地图与图片信息地图复用同一接口。切换渲染器迁移 WGS84 中心、缩放和已有照片 DOM，销毁旧地图／原生标记／事件／计时器；同引擎主题切换不重建地图。迟到的旧 SDK 初始化和旧瓦片错误不会安装或修改新视图。
- SDK 只在显示边界转换 GCJ02；原生边界／屏幕反投影结果转回 WGS84 后用于图库查询。聚合索引采用当前显示坐标系，保持原始照片坐标不变。
- SDK 用官方加载器直接接入应用页面，原生缩放、拖拽、3D 视图及方向控件不被 Leaflet 拦截；SDK 署名／版权保留。适配层提供 `getNativeMap()` 与能力描述，后续扩展可使用供应商专有 API，不需要退回最低公共功能集。
- SDK 请求沿用 WebView 启动代理；修改显式代理需重启，栅格瓦片和逆地理编码的 Rust 代理仍立即生效。JS Key／安全码与 Web 服务 Key／SK 分离，不向 SDK 传入原图路径或人脸信息。
- SDK 是可信供应商脚本，运行于应用页面；此方案没有 iframe 沙箱隔离，也不应声称第三方脚本无法访问页面。纯回归测试使用假 SDK，不消费用户 API 额度；真实鉴权、GPU 渲染、交互及标记对齐另需桌面验收。

## 坐标系与转换模块

`src-vite/src/common/mapCoordinates.js` 复用旅行项目 `src/map/coordinates.ts` 的通用算法，支持 **WGS84、GCJ02、BD09** 三种经纬度坐标系互转，不引入行程数据结构或第三方网络依赖。

```js
import { convertCoordinate, convertPathCoordinates } from '@/common/mapCoordinates.js'

// 统一元组顺序：[经度, 纬度]，不是 [纬度, 经度]。
const gcj = convertCoordinate([116.397470, 39.908823], 'WGS84', 'GCJ02')
const bd = convertCoordinate(gcj, 'GCJ02', 'BD09')
const gps = convertCoordinate(bd, 'BD09', 'WGS84')
const path = convertPathCoordinates([gcj, gcj], 'GCJ02', 'WGS84')
```

- 转换返回新数组，不修改输入；同坐标系转换也复制数据。非法元组、非有限数值及未知坐标系会报错。业务输入仍需校验经纬度范围；显示投影允许跨日期变更线的连续经度。
- 原有 `(经度, 纬度)` 方向函数继续可用，同时支持元组。GCJ02 反算默认使用现有高精度迭代算法；`gcj02ToWgs84Approx` 仅提供快速近似。BD09 的常规反算有微小数值误差；`Exact` 别名不会消除百度算法自身的近似误差。
- GCJ02 沿用示例的中国区域矩形判断，范围外保持原坐标；它不是精确国界判断。BD09 的附加偏移不套用该判断。不要因切换地图而改写数据库或 EXIF，已经转换的坐标须标明来源，避免重复偏移。
- `createCoordinateCrs(L, 'GCJ02' | 'BD09')` 可供后续自定义栅格渲染器适配投影。高德／腾讯直接使用原生 SDK 的 GCJ02 视图；BD09 能力是公共转换工具，**不表示已接入百度地图服务**。EPSG3857 是米单位的显示投影，不作为第四种经纬度坐标系。
- 地点解析继续遵循供应商协议：高德转换请求坐标，腾讯使用原始 GPS + `coord_type=1`，天地图使用 WGS84。前端与 Rust 高德请求通过同一组独立参考测试向量校验，测试不调用真实供应商。

## 缩略图就绪与协议诊断

- `thumbnail_ready` 会跳过分页空槽／占位行；异步元数据返回后再次检查图库和页面请求，避免更新已切换的内容。处理失败有明确日志，不再形成未处理的 Promise 异常。
- 缩略图协议缓存缺失时在后台工作线程等待现有生成流程，按 URL 请求的尺寸生成，不先报 404 再生成 200px 图片；并发受现有四任务信号量约束，图库锁覆盖读取和缓存写入，不改动原图。
- 缺失文件／无可用缓存返回 404；无法解码返回 422；图库已切换返回 409；实际读取或工作线程异常返回 500。错误响应带文件 ID 和诊断原因且不缓存。Pinia store 安装与浏览器懒加载提示是开发／浏览器信息，不属于这些错误。

## 地点中文名称与已有图库

- 左侧地点列表、地点筛选标题和照片信息的名称由 `locationNames.js` 提供显示层本地化。中文界面优先使用 GeoNames 中文别名；无可靠别名、同名歧义或其他语言界面保留原名称。不尝试从拼音猜汉字。
- `location-aliases-zh.json` 当前覆盖中国大陆 8,296 个旧地点键及 46 个行政区名称／别名；不是全球地点的完整中文翻译。数据库中的 `geo_name`／`geo_admin1` 不因显示语言改变，已有图库无需重建、联网或迁移即可使用别名。名称与计数一起排序；搜索同时匹配原名称及中文名称。
- “地点”面板标题右侧的刷新按钮打开**批量重新解析地点**。默认选择当前选中的地点，也可明确选择当前图库全部有有效 GPS 的照片；没有地点选择时不会隐式扩大一个地点任务。排除搜索的文件夹与 Live Photo／RAW 配对的附属文件不重复处理。
- 地图显示与地点解析独立。切换底图或解析服务不会自动重写已有地点；导入时在线解析只影响后续导入。在线解析使用 Web 服务 Key，不使用地图 JS Key。
- 批量任务必须主动启动；在线服务另需确认坐标发送及 API 配额。任务固定图库、服务配置版本与地点范围，串行请求（最多约 4 次／秒），沿用全局代理。只有数据库地点列被更新，原媒体、EXIF、GPS 和人脸结果保持不变。
- 每个成功结果立即保存，进度事件按图库／任务隔离。停止、切换图库或服务后丢弃未完成响应；网络请求上限为 15 秒。元数据或 GPS 发生变化时保留旧地点并报告错误。连续五项失败会提前停止本次任务以避免持续消耗配额；完成／停止后刷新地点列表，不逐张刷新整个页面。

### 离线中文别名数据的来源与更新

中文别名派生自 [GeoNames](https://www.geonames.org/) 的 `CN.zip`、`alternatenames/CN.zip` 和 `admin1CodesASCII.txt`，依据 GeoNames ID、原名称和经纬度关联旧 `cities.csv`。只采用明确标记为 `zh`／`zh-CN`／`zh-Hans` 且非历史的名称，保留区域作用域和同名歧义检查。

GeoNames 数据许可为 [Creative Commons Attribution 4.0](https://creativecommons.org/licenses/by/4.0/)。本项目对其筛选、关联并生成显示别名表；源数据版权与署名归 GeoNames。输出记录输入文件 SHA-256，原始大文件只缓存于忽略的 `src-tauri/target/geonames`，不随软件发布。

```powershell
python scripts/build-location-aliases.py --cache src-tauri/target/geonames
node --experimental-strip-types --test src-vite/tests/locationNames.test.ts src-vite/tests/locationBatch.test.ts
```

脚本不修改 `cities.csv` 或用户数据库。更新后检查覆盖数量、歧义及回归测试；精确复现时使用输出记录的相同输入文件，而不是不同日期的 GeoNames 每日转储。

## SOCKS5 地点解析连接诊断

- 代理对 Google 可用，不代表对高德 REST 接口也可用。`socks5://` 由本机解析 DNS；当前 reqwest 的 SOCKS 本机解析连接只尝试首个目标地址，代理的 IPv6 路径异常时可能出现 `unexpected EOF during handshake`。
- `t_network.rs` 对显式 SOCKS5 本机 DNS 的同步／异步客户端使用 IPv4 优先排序，保留全部解析地址与 IPv6-only 目标支持；不改写代理 URL、不静默改用代理 DNS 或直连，也不关闭 TLS 证书校验。`socks5h://` 仍由代理解析，HTTP 代理和本地 API 的直连规则不变。
- 地点解析错误包含服务主机及底层 DNS／代理／TLS 错误链。请求 URL、API Key、签名、代理凭据与 URL 中的坐标先脱敏再显示；普通回归测试只使用固定假 DNS 和本机 SOCKS 服务，不调用用户账户。
- 手工诊断可先对不带 Key 的 REST 接口探测：返回 HTTP 200＋`INVALID_USER_KEY` 只证明传输连通，不证明实际 Key 有效。验证真实 Key 必须明确使用固定测试坐标且不更新图库，不自动重跑整个图库。

```powershell
cargo test --manifest-path src-tauri/Cargo.toml t_network::tests
```

# DLNA 协议依赖

DLNA 的通用协议实现优先复用开源库，Lap 负责媒体库业务和访问控制：

| 能力 | 实现 |
| --- | --- |
| SSDP 搜索、响应、设备公告和撤销 | libupnp |
| 播放控制、SOAP action XML | libupnp |
| GENA 订阅、续订、超时、序号和通知 | libupnp |
| 媒体和目录 HTTP 服务、请求解析、传输编码、背压 | Hyper |
| HTTP 读写超时 | tokio-io-timeout |
| HTTP Range 解析 | http-range |
| 设备描述、SOAP 参数的 XML 解析 | roxmltree，禁用 DTD 并限制体积和深度 |
| XML 文本转义 | quick-xml |
| 设备描述下载 | 项目已有的 reqwest，保持禁代理、禁重定向和大小限制 |
| 默认网络接口及跨平台接口枚举 | netdev |

媒体列表、DIDL 元数据、ContentDirectory 和 ConnectionManager 的操作映射、
所选内容与相册范围、可见性检查、原文件校验、JPEG 预览和会话撤销由 Lap 提供。
目录的 SOAP 响应保留在 Hyper 路由中：完整相册按数据库行写入匿名临时文件，再
以已知长度发送；避免为了构造 SDK 的响应 DOM 而将整个相册放进内存。

## 构建

- `third_party/libupnp` 固定在上游 `release-22.1.7`。
- Windows MSVC 使用固定提交的 `third_party/pthreads4w`；macOS 和 Linux 使用系统线程库。
- Windows ARM64 交叉编译从 CMake 目标预置 pthreads 的架构探测结果，避免在 x64 主机运行 ARM64 程序。
- 全部静态链接，不新增需要用户安装或随安装包查找的 DLL。
- Cargo 的 `cmake` 构建依赖沿用目标架构和工具链；现有 CI 已启用递归子模块检出。

首次构建先执行 `git submodule update --init --recursive`。Windows 的原生准备步骤
仍是仓库根目录的 `node scripts/prepare-native-runtime.mjs`。

## 本地兼容与访问策略

不修改上游子模块内容。CMake 在构建目录生成两处源文件替换：

1. Windows 日志使用 `GetCurrentThreadId()`，修正 SDK 对新版 pthreads4w 的
   `pthread_t` 类型和导出宏的旧假设。
2. GENA 回调限为一个 IPv4 地址，且必须与订阅请求来源 IP 相同。SDK 默认允许
   其他私有网段主机；Lap 保留原来的同主机限制。订阅协议、续订、超时、序号和
   通知仍由 SDK 完成。

升级子模块后必须重新检查这两处替换、许可证及下面的网络回归，不能只更新版本号。

SDK 控制点在进程内复用，初始化失败允许重试。每次共享按网卡单独注册设备，关闭共享时
注销全部设备；它没有文件系统根目录，原始媒体只能经带会话票据的 Rust 路由访问。
C 桥只转接 SDK 的 opaque handle、回调和内存所有权，不复制 SDK 的结构布局。

首次使用时通过 netdev 选择系统默认网络接口，显式传给 `UpnpInit2`；不能依赖 SDK
枚举到的第一张网卡。Windows 使用适配器 GUID，避免 VMware 等虚拟网卡抢先被选中，
并避免中文网卡名称的字符集转换问题。

设备发现默认扫描所有可用 IPv4 网卡。libupnp 的接口状态是进程全局的，因此每张
网卡的发现使用一个短时运行的 `Lap --lap-dlna-discover <interface-id>` 搜索进程。
入口在 Tauri、图库和数据库初始化之前处理；搜索仍由同一个 SDK 完成，不复制
SSDP 协议实现。父进程依次调用搜索进程、聚合并去重结果，取消或超时会终止子进程。
搜索进程使用独立的动态 HTTP 端口，避免影响正在运行的媒体服务。

电视浏览默认在所有可用网卡发布，也可指定一张网卡。每张网卡使用一个
`Lap --lap-dlna-publish <interface-id>` 进程；通过标准输入接收本次共享名称、媒体
端口和会话 ID，通过 JSON 行返回真实接口地址与 SDK 事件地址。SDK 使用
`UpnpRegisterRootDevice2(UPNPREG_BUF_DESC)` 托管设备描述，负责该接口的公告、发现
响应和 GENA。各接口使用相同 UDN，设备描述中的控制与 SCPD 地址指向父进程的
共享 HTTP 服务，事件地址则指向本接口的 SDK。媒体 URL 使用 HTTP 连接的本机地址，
不依赖 Host 请求头或系统默认路由。

父进程监测发布进程并汇总成功网络和失败原因；停止时关闭输入管道，让 SDK 注销
设备及订阅、发送 byebye，超时后才终止进程。父进程退出也会通过管道 EOF 触发撤销。
子进程不初始化 Tauri、图库或数据库。网络地址变化后需要重新开启共享。

投放顺序复用 `AFile` 的字段排序、分组排序和随机种子规则。开始时生成有序 ID
快照，主动播放和未指定排序的 ContentDirectory 浏览使用同一份快照。

## 验证

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --locked
cargo test --manifest-path src-tauri/Cargo.toml --locked --bin Lap t_dlna:: -- --test-threads=1
```

多网卡进程集成测试需要先构建实际应用，然后显式运行：

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --locked --bin Lap
$env:LAP_DLNA_TEST_WORKER = (Resolve-Path src-tauri/target/debug/Lap.exe).Path
cargo test --manifest-path src-tauri/Cargo.toml --locked --bin Lap all_interfaces_advertise_browse_notify_and_withdraw -- --ignored --nocapture --test-threads=1
```

该测试仅发布合成的测试目录与预览，逐网卡检查 SDK 发现、接口地址、媒体 Range、
GENA 初始及更新通知，以及停止后所有地址与公告的撤销。

网络测试会注册临时标准 MediaRenderer / MediaServer，检查发现、撤销、SOAP 控制、
GENA 通知和回调限制，并验证目录浏览、媒体 Range、完整大目录及停止后的访问失效。
这些是协议与模拟接收端测试，不代表真实电视的解码或互操作验收。

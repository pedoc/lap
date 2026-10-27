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

SDK 控制点在进程内复用，初始化失败允许重试。每次共享单独注册设备，关闭共享时
注销设备；它没有文件系统根目录，原始媒体只能经带会话票据的 Rust 路由访问。
C 桥只转接 SDK 的 opaque handle、回调和内存所有权，不复制 SDK 的结构布局。

首次使用时通过 netdev 选择系统默认网络接口，显式传给 `UpnpInit2`；不能依赖 SDK
枚举到的第一张网卡。Windows 使用适配器 GUID，避免 VMware 等虚拟网卡抢先被选中，
并避免中文网卡名称的字符集转换问题。

设备发现默认扫描所有可用 IPv4 网卡。libupnp 的接口状态是进程全局的，因此每张
网卡的发现使用一个短时运行的 `Lap --lap-dlna-discover <interface-id>` 搜索进程。
入口在 Tauri、图库和数据库初始化之前处理；搜索仍由同一个 SDK 完成，不复制
SSDP 协议实现。父进程依次调用搜索进程、聚合并去重结果，取消或超时会终止子进程。
搜索进程使用独立的动态 HTTP 端口，避免影响正在运行的媒体服务。电视浏览服务继续
使用主进程的默认联网接口，界面显示实际发布地址。

投放顺序复用 `AFile` 的字段排序、分组排序和随机种子规则。开始时生成有序 ID
快照，主动播放和未指定排序的 ContentDirectory 浏览使用同一份快照。

## 验证

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --locked
cargo test --manifest-path src-tauri/Cargo.toml --locked --bin Lap t_dlna:: -- --test-threads=1
```

网络测试会注册临时标准 MediaRenderer / MediaServer，检查发现、撤销、SOAP 控制、
GENA 通知和回调限制，并验证目录浏览、媒体 Range、完整大目录及停止后的访问失效。
这些是协议与模拟接收端测试，不代表真实电视的解码或互操作验收。

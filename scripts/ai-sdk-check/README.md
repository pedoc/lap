# Rust AI SDK 可行性验证

本目录是独立验证工具，使用真实 `genai = 0.7.0`，只请求测试临时启动的回环 HTTP 服务。没有真实 API Key，不访问云端模型，也不改变 Lap 的正式 Cargo 依赖。

运行：

```powershell
cargo test --manifest-path scripts/ai-sdk-check/Cargo.toml --locked
```

验证内容：

| 检查 | 实际验证的边界 |
| --- | --- |
| Chat Completions | 自定义 `mage-vl` 模型 ID、配置端点、Bearer 认证、图片请求与统一文本返回 |
| Responses | 独立接口、`input_image` 转换、`store=false` 和统一文本返回 |
| Anthropic Messages | 独立接口、base64 图片转换、`x-api-key` 和统一文本返回 |
| 文本 Embedding | `/v1/embeddings`、独立输入与向量响应 |
| 结构化请求 | SDK 把同一 JSON Schema 映射为 Responses 的 `text.format`；不是验证模型能正确生成 JSON |
| 自定义 HTTP 客户端 | 注入的 reqwest 客户端保留 HTTP 代理路由，不绕过代理直连测试域名 |
| 模型列表 | 从配置的 `/v1/models` 获取模型 ID，保留生成模型和向量模型 ID |
| 流式与取消 | 在 HTTP/1.1 chunked SSE 结束前获得文本；本地取消后丢弃流，测试连接提前关闭 |

SDK 和依赖版本由本目录 `Cargo.lock` 固定。核对环境为 Windows x86_64、Rust/Cargo 1.99.1；仅此检查不代表 macOS、ARM64、Tauri 集成、TLS 端点、真实 SGLang／vLLM 以及模型能力均已验收。

取消的验证只针对本地消费及本测试连接。远程推理是否立即停止、API 是否继续计费，取决于服务端；不能由客户端丢弃流直接推导。

检查使用配置指定 `AdapterKind`，不依赖模型名猜服务商。测试中的 HTTP 服务是协议夹具，不是将来给 Lap 使用的另一套协议实现。

前一轮 [Python 部署探测](../ai-server/README.md) 用于服务器安装与验收；本目录用于检验 Lap Rust 后端是否可直接复用成熟 SDK。生产接入建议见 [Rust SDK 可行性与选型](../../docs/development/ai-client-libraries.md)。

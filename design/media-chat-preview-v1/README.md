# 媒体对话设计预览

在浏览器打开 [index.html](index.html)，切换“会话窗口”“附加媒体”“写回预览”。无需启动 Lap 或安装依赖。

- [01-chat.png](01-chat.png)：独立媒体对话窗口、会话历史和当前媒体。
- [02-attach.png](02-attach.png)：紧凑布局、附加媒体和本轮提问范围。
- [03-review.png](03-review.png)：标题、说明和标签的写回预览。

这是供确认的独立交互原型，不连接模型，不保存真实会话，不修改资料库。设计说明放在预览页下方，不出现在截图中。示例照片来源见 [SOURCES.md](assets/SOURCES.md)。

需要重新导出 PNG 时，在 Windows 上用 Node 24+ 运行：

```powershell
node design/media-chat-preview-v1/render-preview.mjs
```

脚本使用已安装的 Microsoft Edge，无需额外依赖，退出时关闭临时浏览器和 HTTP 服务。运行报告 `render-report.json` 不纳入版本控制。

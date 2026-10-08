# Codex MCP 配置

`config.toml` 是可提交的共享配置，不得包含 API 密钥、访问令牌或密码。

## Context7 API 密钥

Context7 的 `CONTEXT7_API_KEY` 请求头通过 `env_http_headers` 从同名环境变量读取；配置文件中的字符串只是变量名，不是密钥。

1. 在本机配置 `CONTEXT7_API_KEY` 环境变量。在 Windows 中可通过“编辑账户的环境变量”添加用户变量；不要把真实密钥写进仓库文件或 shell 命令历史。
2. 完全退出并重新启动 Codex 及其启动终端/IDE，使新进程继承环境变量。
3. 未配置密钥时，请先完成上述配置，再启用 Context7。仅将密钥写入 `.env` 不代表 Codex 会自动加载它。

可在 PowerShell 中检查变量是否存在，而不显示其值：

```powershell
[bool]$env:CONTEXT7_API_KEY
```

若真实密钥曾被提交或推送，应首先到供应商处撤销并重新生成密钥。删除当前文件里的密钥并不会清除 Git 历史；必要时使用 `git filter-repo` 清理历史，并协调所有协作者更新仓库副本。

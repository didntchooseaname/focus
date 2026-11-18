errors-core-io = 无法访问 { $path }：{ $detail }。请确认文件存在且你有写入权限。
errors-core-outside-vault = 路径 { $path } 在库之外。请选择库内的位置。
errors-core-already-exists = { $path } 已存在。请换一个名称。
errors-core-unknown-note = 库中已不存在这篇笔记。请刷新目录树。
errors-core-conflict = { $path } 在上次读取后已在磁盘上发生变化。请重新载入或保留你的版本。
errors-core-other = { $message }

errors-ai-missing-key = 没有 API 密钥。请在“设置”的“AI”中添加 DeepSeek 密钥。
errors-ai-invalid-key = DeepSeek 拒绝了该 API 密钥（401）。请在“设置”的“AI”中检查。
errors-ai-insufficient-balance = DeepSeek 余额不足（402）。请为账户充值后重试。
errors-ai-rate-limited = 已达到速率限制（429）。请在 { $retry_after_secs } 秒后重试。
errors-ai-bad-request = DeepSeek 拒绝了该请求（{ $status }）：{ $message }
errors-ai-server = DeepSeek 服务器不可用（{ $status }）。请稍后重试。
errors-ai-network = 网络错误：{ $detail }。请检查网络连接。
errors-ai-protocol = 无法解析响应：{ $detail }
errors-ai-cancelled = 已取消

errors-ai-mcp-invalid-config = MCP 服务器“{ $server }”：配置无效：{ $reason }
errors-ai-mcp-spawn = MCP 服务器“{ $server }”：无法启动 `{ $command }`：{ $reason }。请在“设置”的“AI”中检查该命令。
errors-ai-mcp-timeout = MCP 服务器“{ $server }”：`{ $method }` 在 { $secs } 秒后仍无响应。{ $stderr }
errors-ai-mcp-exited = MCP 服务器“{ $server }”：进程已停止。{ $detail }
errors-ai-mcp-protocol = MCP 服务器“{ $server }”：{ $reason }
errors-ai-mcp-cancelled = 已取消

errors-ops-io = 无法访问 { $path }：{ $detail }。请确认文件存在且你有写入权限。
errors-ops-already-exists = 项目 { $path } 已存在。请换一个名称。
errors-ops-parse = 无法读取 { $file }：{ $message }
errors-ops-invalid = { $message }
errors-ops-conflict = { $path } 在读取后已在磁盘上改变。请先重新加载再保存。

errors-md-unsupported = 不支持此图表类型：{ $kind }。已改为显示源码。
errors-md-syntax = Mermaid 语法无效：{ $detail }
errors-md-render = 无法绘制图表：{ $detail }

errors-search-index-build = 无法建立搜索索引：{ $detail }
errors-search-index-write = 无法保存搜索索引：{ $detail }
errors-mcp-not-running = Focus 应用未在此库上打开。请先打开应用，然后重试。
errors-mcp-failed = { $message }

errors-vault-open = 无法打开库：{ $detail }
errors-folder-picker = 文件夹选择器出错：{ $detail }
errors-theme = 无法应用主题：{ $detail }
errors-settings-read = 无法读取设置文件：{ $detail }。已使用默认设置。
errors-settings-write = 无法保存设置：{ $detail }
errors-update-network = 版本服务器没有响应。请检查网络连接后重试。
errors-update-unsigned = 最新版本没有签名。未安装任何内容。
errors-update-signature = 该版本不是用 Focus 发布密钥签名的。未安装任何内容。
errors-update-damaged = 下载的文件与已签名的版本不符。未安装任何内容。
errors-update-missing = 此版本没有适用于当前安装方式的文件。请从版本页面获取。
errors-update-write = Focus 无法写入 { $folder }（{ $detail }）。请从版本页面获取新版本。
errors-translation-file = 已忽略翻译文件（{ $file }）：{ $detail }
errors-ai-keystore = 无法将 API 密钥存入系统钥匙串：{ $detail }
errors-copy-note = 无法复制笔记：{ $detail }
errors-export-report = 无法导出报告：{ $detail }
clipboard-timeout = 剪贴板在 3 秒内没有响应。
clipboard-too-large = 剪贴板内容超过 { $size }：未粘贴任何内容。
clipboard-failed = { $tool } 无法读取剪贴板。
clipboard-start = 无法启动 { $tool } 来读取剪贴板。

ai-title = AI
ai-input-placeholder = 询问你的笔记
ai-send = 发送
ai-stop = 停止
ai-retry = 重试
ai-copy = 复制
ai-insert-at-cursor = 插入到光标处
ai-save-as-note = 保存为笔记
ai-new-conversation = 新对话
ai-history = 历史记录
ai-history-empty = 还没有对话。
ai-reasoning = 推理
ai-reasoning-show = 显示推理
ai-reasoning-hide = 隐藏推理
ai-generating = 正在生成…
ai-tool-call = 工具调用：{ $name }
ai-tool-search-notes = 正在搜索笔记
ai-tool-read-note = 正在读取笔记
ai-tool-list-folder = 正在列出文件夹
ai-tool-get-backlinks = 正在读取反向链接
ai-tool-get-outline = 正在读取大纲
ai-tool-list-tags = 正在列出标签
ai-confirm-title = 确认此修改
ai-confirm-apply = 应用
ai-confirm-reject = 拒绝
ai-confirm-always-allow = 始终允许 { $server } 使用此工具
ai-diff-title = 差异
ai-context-note = 当前笔记：{ $path }
ai-context-selection = 选中内容
ai-context-added = 通过 @ 添加：{ $path }
ai-context-remove = 从上下文中移除
ai-tokens-approx = ~{ $count }
ai-tokens-approx-thousands = ~{ $count }千
ai-context-tokens =
    { $count ->
       *[other] 约 { $count } 个 token
    }
ai-usage-tokens =
    { $count ->
       *[other] { $count } 个 token
    }
ai-usage-cached = { $percent } % 来自缓存
ai-usage-cost = 预估费用 { $cost }
ai-balance = 余额：{ $amount }
ai-redaction-notice =
    { $count ->
       *[other] 发送前已屏蔽 { $count } 项机密信息
    }
ai-connection-ok = 连接正常。余额：{ $amount }。
ai-connection-testing = 正在测试连接…
ai-mcp-state-running = 运行中
ai-mcp-state-stopped = 已停止
ai-mcp-state-error = 错误
ai-mcp-tools =
    { $count ->
       *[other] { $count } 个工具
    }
ai-actions-edit = 编辑操作
ai-action-duplicate = 复制
ai-action-hide = 隐藏
ai-action-restore = 恢复
ai-action-builtin = 内置
ai-action-ops-only = 仅限 Ops 模式

ai-action-summarize-label = 总结
ai-action-summarize-desc = 所选内容的核心思想和要点。
ai-action-refine-label = 润色
ai-action-refine-desc = 在不改变内容的前提下，让表达更清晰、结构更好。
ai-action-rephrase-label = 改写
ai-action-rephrase-desc = 用不同的措辞表达同样的意思。
ai-action-correct-label = 纠错
ai-action-correct-desc = 修正拼写、语法和标点。
ai-action-translate-fr-label = 翻译为法语
ai-action-translate-fr-desc = 保持 Markdown、代码和链接不变。
ai-action-translate-en-label = 翻译为英语
ai-action-translate-en-desc = 保持 Markdown、代码和链接不变。
ai-action-continue-label = 续写
ai-action-continue-desc = 以相同的语气写出下一段。
ai-action-explain-label = 解释
ai-action-explain-desc = 向熟悉该领域的读者解释这段文字。
ai-action-tasks-label = 提取任务
ai-action-tasks-desc = 把文字整理成 Markdown 任务列表。
ai-action-title-tags-label = 建议标题和标签
ai-action-title-tags-desc = 为笔记提供一个简短的标题和三到六个标签。
ai-action-mermaid-label = 生成 Mermaid 图表
ai-action-mermaid-desc = 把文字的结构绘制成图表。
ai-action-note-from-outline-label = 根据大纲生成笔记
ai-action-note-from-outline-desc = 根据所选大纲写出完整的笔记。
ai-action-ops-finding-label = 撰写发现项
ai-action-ops-finding-desc = 根据你的笔记生成描述、影响、证据、修复建议和参考资料。
ai-action-ops-exec-summary-label = 执行摘要
ai-action-ops-exec-summary-desc = 面向非技术读者的项目摘要。
ai-action-ops-next-enum-label = 下一步枚举
ai-action-ops-next-enum-desc = 按价值从高到低排列的后续步骤。

ai-output-chat = 显示在对话中
ai-output-replace = 替换文本
ai-output-insert = 插入到下方
ai-output-note = 创建笔记

ai-model = 模型
ai-actions = 操作
ai-add-context = 将笔记加入上下文
ai-empty-title = 询问你的笔记
ai-empty-hint = 用 @ 把笔记加入上下文，或对当前笔记运行一个操作。
ai-no-key-title = 没有 API 密钥
ai-no-key-body = 在设置中添加 { $provider } API 密钥，才能提问和运行操作。
ai-no-key-open = 打开设置
ai-history-untitled = 未命名对话
ai-history-delete = 删除对话
ai-tool-refused = 已拒绝
ai-tool-failed = 失败
ai-tool-no-output = 该工具没有返回内容。
ai-proposal-replace = 建议的替换
ai-confirm-applied = 已应用
ai-confirm-rejected = 已拒绝
ai-diff-more = 未显示的行数：{ $count }
ai-usage-input = 输入 { $count }
ai-usage-output = 输出 { $count }
ai-usage-cache = 缓存 { $percent } %
ai-stopped = 已停止
ai-copied = 已复制
ai-scroll-bottom = 滚动到底部
ai-scroll-bottom-new = 滚动到底部：下方有新内容
ai-action-needs-selection = 请先选中文本
ai-action-needs-note = 请先打开笔记
ai-needs-key = 请在设置中添加 API 密钥
ai-actions-running = 请等待回答结束
ai-masking-active = 发送前会遮盖密钥
ai-mention-empty = 没有匹配的笔记
ai-actions-target-selection = 应用于所选内容
ai-actions-target-note = 应用于整篇笔记
ai-actions-no-target = 打开笔记或选中文本后才能使用操作。
ai-error-missing-key = 尚未设置 { $provider } API 密钥。请在“设置 › AI”中添加。
ai-error-invalid-key = { $provider } 拒绝了该 API 密钥（401）。请在“设置 › AI”中检查。
ai-error-balance = { $provider } 账户余额不足（402）。请充值后重试。
ai-error-rate-limit = 发给 { $provider } 的请求过多（429）。请在 { $seconds } 秒后重试。
ai-error-bad-request = { $provider } 拒绝了该请求（{ $status }）：{ $message }
ai-error-server = { $provider } 暂时不可用（{ $status }）。请稍后重试。
ai-error-network = Focus 无法连接 { $provider }：{ $message }。请检查网络后重试。
ai-error-protocol = { $provider } 返回了 Focus 无法读取的响应：{ $message }。请重试。
ai-error-refused = 模型拒绝了此请求（类别：{ $category }）。请改写或选择其他模型。
ai-error-no-model = 未为 { $provider } 设置模型，也无法自动选择。请在设置 › AI 中设置模型，然后重试。
ai-tools-disabled = 此模型不接受工具：Focus 已在不带工具的情况下重新发送请求。
ai-link-unresolved = 没有笔记匹配 { $target }。
ai-mcp-failed = MCP 服务器 { $server } 未能启动：{ $reason }
ai-usage-conversation = 本次对话：{ $tokens } 个 token，估算费用 { $cost }。
ai-ops-reminder = Ops 模式：问题、上下文和工具结果会发送给 { $provider }。
ai-ops-reminder-detail = Ops 模式：您的问题、上下文标签和工具结果会发送给 { $provider }。密码、哈希、密钥和令牌会先被屏蔽；仅在开启该选项时才屏蔽 IP 地址。
ai-ops-reminder-local = Ops 模式：问题、上下文和工具结果会发送到本机上的 { $provider }（{ $url }），不会离开本机。
ai-ops-reminder-local-detail = Ops 模式：你的问题、上下文标签和工具结果会发送到本机上的 { $provider }（{ $url }），不会离开本机，也不会经网络发送。
ai-provider-local = 请求保留在本机（地址 { $url }）。

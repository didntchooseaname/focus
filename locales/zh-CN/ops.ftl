ops-title = Ops
ops-new-engagement = 新建项目
ops-switch-engagement = 切换项目
ops-no-engagement = 没有进行中的项目
ops-dashboard = 项目仪表盘
ops-set-target = 设为目标
ops-target = 目标
ops-variables = 变量
ops-report-export-sysreptor = 导出到 SysReptor
ops-sysreptor-written = SysReptor 项目已写入 { $path }。请在 SysReptor 中将其作为项目导入。被省略或调整的内容：
ops-sysreptor-failed = 导出到 SysReptor 失败：{ $detail }
ops-proofs-name = 证明名称（local.txt、proof.txt……）
ops-proofs-value = 值
ops-proofs-machine = 考试机器
ops-proofs-title = 证明（{ $count }）
ops-proofs-add = 添加证明
ops-timer = 计时器
ops-timer-start = 开始计时
ops-timer-pause = 暂停
ops-timer-resume = 继续
ops-timer-stop = 停止
ops-timer-stopped = 已停止，共工作 { $duration }。
ops-trash = 移到回收站
ops-trash-title = 要将 { $name } 移到回收站吗？
ops-trash-confirm-vault = { $name } 的文件夹将移到库的回收站（.trash 文件夹），可在那里恢复，不会被删除。
ops-trash-confirm-system = { $name } 的文件夹将移到系统回收站，可在那里恢复，不会被删除。
ops-trash-do = 移到回收站
ops-var-credential-pick = 从凭据中选择
ops-var-credential-linked = 跟随一个凭据
ops-var-credential-unlink = 不再跟随该凭据
ops-credentials-use = 将此凭据用于变量
ops-credentials-used = 已设置变量：{ $names }。
ops-commands-show-secrets = 在命令中显示机密
ops-commands-hide-secrets = 在命令中隐藏机密
ops-substitute-hint = 替换命令中的变量
ops-context = 上下文
ops-copy-resolved = 复制解析后的内容
ops-copy-raw = 复制原文
ops-open-source-note = 打开来源笔记

ops-kind-external-pentest = 外部渗透测试
ops-kind-internal-pentest = 内部渗透测试
ops-kind-web-pentest = Web 渗透测试
ops-kind-red-team = 红队
ops-kind-lab = 实验环境
ops-kind-exam = 考试

ops-status-planning = 准备中
ops-status-in-progress = 进行中
ops-status-reporting = 报告
ops-status-done = 已完成

ops-host-status-new = 新发现
ops-host-status-enum = 已枚举
ops-host-status-foothold = 已立足
ops-host-status-user = 已获用户权限
ops-host-status-owned = 已拿下

ops-severity-info = 信息
ops-severity-low = 低
ops-severity-medium = 中
ops-severity-high = 高
ops-severity-critical = 严重

ops-finding-status-draft = 草稿
ops-finding-status-confirmed = 已确认
ops-finding-status-reported = 已报告
ops-finding-status-fixed = 已修复
ops-finding-status-accepted = 已接受

ops-section-overview = 概览
ops-section-journal = 日志
ops-section-hosts = 主机
ops-section-findings = 发现项
ops-section-credentials = 凭据
ops-section-checklists = 检查清单
ops-section-commands = 命令库
ops-section-exam = 考试
ops-section-report = 报告
ops-section-evidence = 证据
ops-section-scope = 范围

ops-journal-add = 添加条目
ops-journal-placeholder = 发生了什么，针对哪个目标
ops-journal-filter = 筛选日志
ops-journal-export-csv = 导出为 CSV
ops-journal-count =
    { $count ->
       *[other] { $count } 条记录
    }

ops-hosts-column-address = 地址
ops-hosts-column-name = 名称
ops-hosts-column-os = 系统
ops-hosts-column-status = 状态
ops-hosts-column-ports = 端口
ops-hosts-import-nmap = 导入 nmap 扫描结果
ops-hosts-count =
    { $count ->
       *[other] { $count } 台主机
    }

ops-finding-new = 新建发现项
ops-finding-severity = 严重程度
ops-finding-status = 状态
ops-finding-assets = 受影响资产
ops-finding-cwe = CWE
ops-finding-cwe-placeholder = 添加弱点：编号或名称
ops-finding-cwe-remove = 移除此弱点
ops-finding-cwe-filter = 按 CWE 筛选
ops-finding-cwe-invalid = { $value } 不是 CWE 编号，也不是目录中的名称。
ops-finding-cwe-none = 没有匹配的弱点
ops-finding-body = 正文
ops-finding-body-placeholder = 在此撰写发现（Markdown）
ops-finding-body-save = 保存正文
ops-finding-body-reload = 从笔记重新加载
ops-finding-preview = 预览
ops-finding-attack = ATT&CK 技术
ops-finding-score = CVSS 评分
ops-finding-vector = CVSS 向量
ops-finding-count =
    { $count ->
       *[other] { $count } 个发现项
    }

ops-cvss-calculator = CVSS 计算器
ops-cvss-version = 版本 { $version }
ops-cvss-invalid-vector = 此 CVSS 向量无效。

ops-credentials-column-user = 用户
ops-credentials-column-secret = 密钥
ops-credentials-column-source = 来源
ops-credentials-add = 添加凭据
ops-credentials-reveal = 显示
ops-credentials-hide = 隐藏

ops-checklist-progress = 已完成 { $done } / { $total }
ops-checklist-next-step = 下一步
ops-checklist-from-note = 用笔记作为模板

ops-exam-profile = 考试配置
ops-exam-time-left = 剩余 { $duration }
ops-exam-time-over = 时间已到
ops-exam-alert = 剩余不足 { $minutes } 分钟
ops-exam-points =
    { $points ->
       *[other] { $points } 分
    }
ops-exam-phase-practical = 实操阶段
ops-exam-phase-report = 报告阶段
ops-exam-rules = 规则

ops-report-build = 生成报告
ops-report-export-html = 导出为 HTML
ops-report-export-pdf = 导出为 PDF
ops-evidence-paste = 粘贴截图作为证据
ops-reminder-sent-data = 离开你设备的数据

ops-stat-hosts = 主机
ops-stat-owned = 已拿下
ops-stat-findings = 发现项
ops-stat-critical = 严重
ops-stat-checklist = 检查清单
ops-latest-events = 最新事件

ops-exam-oscp-rule-1 = 考试全程有人监考。
ops-exam-oscp-rule-2 = 证明文件 local.txt 和 proof.txt：在目标上的交互式 shell 中，用 type 或 cat 从原始位置读取；仅有 Web shell 不够。每份证明都要出现在报告的截图中。
ops-exam-oscp-rule-3 = Windows 上：SYSTEM、Administrator 或管理员级别的 shell；Linux 上：root shell。
ops-exam-oscp-rule-4 = Metasploit 和 Meterpreter：只能用于自选的一台目标机器，首次使用时即锁定。
ops-exam-oscp-rule-5 = 禁止：欺骗（IP、ARP、DNS、NBNS）、商业工具、自动化利用、批量漏洞扫描器、AI 助手和语言模型。
ops-exam-oscp-rule-6 = 报告：逐步记录每一次攻击，在考试结束后 24 小时内提交；提交后不可更改。
ops-exam-osep-rule-1 = 考试全程有人监考。
ops-exam-osep-rule-2 = 证明文件 local.txt 和 proof.txt：在目标上的交互式 shell 中读取；仅有 Web shell 或 RDP 会话不够。
ops-exam-osep-rule-3 = 报告为 PDF，并打包在 .7z 中；脚本和概念验证以文本形式复制进 PDF；提交后不可更改。
ops-exam-osep-rule-4 = 截图中缺少证明，该目标得零分。
ops-exam-oswe-rule-1 = 考试全程有人监考。
ops-exam-oswe-rule-2 = 报告为 PDF，并打包在 .7z 中；概念验证脚本以文本（或 base64）形式粘贴进 PDF；提交后不可更改。
ops-exam-oswe-rule-3 = 缺少证明或文档可能会减少某个目标的得分，甚至使其作废。
ops-exam-osed-rule-1 = 考试全程有人监考。
ops-exam-osed-rule-2 = 共三项任务；只有完整完成的任务才计分。
ops-exam-osed-rule-3 = 每个目标一份证明，在控制面板中提交，并显示在截图中。
ops-exam-osed-rule-4 = 报告需描述每项已解决任务的发现与利用过程；提交后不可更改。
ops-exam-crtp-rule-1 = 考试结束时：需交付挑战的详细解法和可行的缓解措施。
ops-exam-crtp-rule-2 = 含一次考试机会；不正当行为将被处以六个月的冷却期。
ops-exam-crte-rule-1 = 考试结束时：需交付挑战的详细解法和可行的缓解措施。
ops-exam-crte-rule-2 = 环境包含多个域和林。
ops-exam-crto-rule-1 = 48 小时的实验环境运行时间，可在 4 天的时间窗口内使用。
ops-exam-crto-rule-2 = 每台机器一个 flag，在计分板上提交：8 个中拿到 6 个即通过。按 flag 计分，而非按分数。
ops-exam-pnpt-rule-1 = 从开源情报收集到攻陷考试域控制器。
ops-exam-pnpt-rule-2 = 专业的书面报告，然后在评审面前进行 15 分钟的口头汇报。
ops-exam-cpts-rule-1 = 商业级报告，须在进入考试后 10 天内上传到考试页面。
ops-exam-cpts-rule-2 = 需在考试页面提交多个 flag；报告之外还须达到最低分数。
ops-exam-cpts-rule-3 = 报告逾期提交会使考试券失效，且没有第二次机会。
ops-exam-bscp-rule-1 = 两个应用，每个三个阶段：进入某个账户、进入管理界面、读取 /home/carlos/secret。
ops-exam-bscp-rule-2 = 整个考试期间必须使用 Burp 项目文件，并提交分析。
ops-exam-bscp-rule-3 = 不得寻求外部帮助；不得分享考试地址。
ops-exam-ejpt-rule-1 = 总分至少 70 %，且每个领域都须达到最低分。
ops-exam-ejpt-rule-2 = 选择题和实验；自动评分。
ops-exam-ecppt-rule-1 = 总分至少 70 %；自动评分。
ops-exam-ecppt-rule-2 = 考试券有效期 180 天，14 天内可免费重考一次。
ops-exam-oscp-machine-1 = 独立机器 1
ops-exam-oscp-machine-2 = 独立机器 2
ops-exam-oscp-machine-3 = 独立机器 3
ops-exam-oscp-machine-4 = Active Directory 组，机器 1
ops-exam-oscp-machine-5 = Active Directory 组，机器 2
ops-exam-oscp-machine-6 = Active Directory 组，机器 3
ops-exam-milestone-initial-access = 初始访问（local.txt）
ops-exam-milestone-privilege-escalation = 权限提升（proof.txt）

ops-cvss-metric-av = 攻击向量
ops-cvss-metric-ac = 攻击复杂度
ops-cvss-metric-at = 攻击要求
ops-cvss-metric-pr = 所需权限
ops-cvss-metric-ui = 用户交互
ops-cvss-metric-s = 范围
ops-cvss-metric-c = 机密性
ops-cvss-metric-i = 完整性
ops-cvss-metric-a = 可用性
ops-cvss-metric-vc = 易受攻击系统机密性
ops-cvss-metric-vi = 易受攻击系统完整性
ops-cvss-metric-va = 易受攻击系统可用性
ops-cvss-metric-sc = 后续系统机密性
ops-cvss-metric-si = 后续系统完整性
ops-cvss-metric-sa = 后续系统可用性
ops-cvss-value-av-n = 网络
ops-cvss-value-av-a = 相邻网络
ops-cvss-value-av-l = 本地
ops-cvss-value-av-p = 物理
ops-cvss-value-ac-l = 低
ops-cvss-value-ac-h = 高
ops-cvss-value-at-n = 无
ops-cvss-value-at-p = 存在
ops-cvss-value-pr-n = 无
ops-cvss-value-pr-l = 低
ops-cvss-value-pr-h = 高
ops-cvss-value-ui-n = 无
ops-cvss-value-ui-r = 需要
ops-cvss-value-ui-p = 被动
ops-cvss-value-ui-a = 主动
ops-cvss-value-s-u = 不变
ops-cvss-value-s-c = 改变
ops-cvss-value-c-h = 高
ops-cvss-value-c-l = 低
ops-cvss-value-c-n = 无
ops-cvss-value-i-h = 高
ops-cvss-value-i-l = 低
ops-cvss-value-i-n = 无
ops-cvss-value-a-h = 高
ops-cvss-value-a-l = 低
ops-cvss-value-a-n = 无
ops-cvss-value-vc-h = 高
ops-cvss-value-vc-l = 低
ops-cvss-value-vc-n = 无
ops-cvss-value-vi-h = 高
ops-cvss-value-vi-l = 低
ops-cvss-value-vi-n = 无
ops-cvss-value-va-h = 高
ops-cvss-value-va-l = 低
ops-cvss-value-va-n = 无
ops-cvss-value-sc-h = 高
ops-cvss-value-sc-l = 低
ops-cvss-value-sc-n = 无
ops-cvss-value-si-h = 高
ops-cvss-value-si-l = 低
ops-cvss-value-si-n = 无
ops-cvss-value-sa-h = 高
ops-cvss-value-sa-l = 低
ops-cvss-value-sa-n = 无

ops-var-rhost-desc = 目标地址
ops-var-lhost-desc = 本机地址
ops-var-lport-desc = 本地监听端口
ops-var-domain-desc = 域
ops-var-user-desc = 用户名
ops-var-pass-desc = 密码
ops-var-hash-desc = 密码哈希
ops-var-dc-desc = 域控制器
ops-var-dc_host-desc = 域控制器主机名
ops-var-domain_sid-desc = 域 SID
ops-var-url-desc = 目标网址
ops-var-wordlist-desc = 字典文件
ops-var-hostname-desc = 目标名称
ops-var-interface-desc = 网络接口

ops-checklist-mission-label = 项目流程
ops-checklist-exam-label = 考试流程

ops-cancel = 取消
ops-close = 关闭
ops-commands-count = 命令：{ $count }
ops-commands-empty = 没有找到命令。Focus 会读取你自己笔记中的 shell 代码块，范围是设置中指定的文件夹。
ops-commands-journaled = 命令已加入日志。
ops-commands-no-match = 没有命令匹配此搜索
ops-commands-search = 按工具、命令或标题搜索
ops-commands-to-journal = 加入日志
ops-copied = 已复制
ops-credentials-column-domain = 域
ops-credentials-column-kind = 类型
ops-credentials-copy = 复制密钥
ops-credentials-count =
    { $count ->
       *[other] { $count } 条凭据
    }
ops-credentials-duplicate = 此凭据已记录。
ops-credentials-empty = 还没有凭据。请在上方添加。
ops-cvss-apply = 应用到所选发现
ops-exam-points-label = 分数
ops-exam-points-of = 通过需 { $pass } 分，已得 { $points } 分（总分 { $total }）
ops-exam-points-pass = 通过需 { $pass } 分，已得 { $points } 分
ops-filter-all = 全部
ops-finding-create = 创建
ops-finding-title-placeholder = 发现的标题
ops-finding-vector-copy = 复制向量
ops-findings-empty = 没有可显示的发现。请用“新建发现”创建。
ops-hosts-empty = 还没有主机。导入 nmap 扫描结果来填充此表。
ops-hosts-import = 导入
ops-hosts-import-hint = 粘贴 nmap 扫描输出（XML、grepable 或普通格式）。
ops-hosts-import-report = 新建：{ $created }，更新：{ $updated }，读取的服务：{ $services }。
ops-hosts-paste-placeholder = 在此粘贴扫描结果
ops-journal-all-targets = 所有目标
ops-journal-empty = 日志为空。请从 Ops 栏添加条目。
ops-journal-no-match = 没有条目匹配此筛选
ops-loading = 正在读取项目…
ops-new-client = 客户
ops-new-create = 创建项目
ops-new-kind = 项目类型
ops-new-name = 名称
ops-new-name-required = 请为项目命名。
ops-new-scope-placeholder = 地址或域名
ops-new-template = 清单模板
ops-new-template-default = 该类型的默认模板
ops-no-target = 没有目标
ops-open-note = 打开笔记
ops-exam-rules-count = 规则：{ $count }

ops-credentials-column-valid = 适用于
ops-host-cited-none = 还没有内容引用此主机。
ops-host-cited-findings = 引用此主机的发现
ops-host-cited-credentials = 引用此主机的凭据
ops-journal-copy-link = 复制此条目的链接
ops-report-author = 作者
ops-report-empty = 生成报告后可在此预览。
ops-report-include-credentials = 凭据
ops-report-include-journal = 日志
ops-report-mask-secrets = 遮盖密钥
ops-report-preview = 预览
ops-report-secrets-warning = 报告将包含明文密钥。
ops-report-write = 写入 Report.md
ops-report-written = 已写入 { $file }。
ops-new-note-search = 搜索含复选框的笔记
ops-var-add = 添加变量
ops-var-invalid-name = 变量名只能使用字母、数字和下划线，且不能以数字开头。
ops-var-name-placeholder = 名称
ops-var-value-placeholder = 值

ops-finding-sheet = 发现
ops-finding-sheet-empty = 选择一项发现以阅读其笔记。

ops-exam-start = 开始考试计时
ops-exam-not-started = { $minutes } 分钟，尚未开始
ops-finding-edit = 编辑
ops-finding-edit-done = 完成
ops-finding-techniques = ATT&CK 技术
ops-finding-asset-placeholder = 添加资产（主机或地址）
ops-finding-technique-placeholder = 添加技术（T1059.001）
ops-finding-asset-remove = 移除此资产
ops-finding-technique-remove = 移除此技术
ops-finding-technique-invalid = “{ $value }”不是 ATT&CK 技术标识符。
ops-filter-severity = 严重程度：{ $value }
ops-filter-status = 状态：{ $value }
ops-scope = 范围
ops-scope-placeholder = 地址或域名，按 Enter 保存
ops-hosts-import-file = 选择扫描文件…
ops-sort-severity = 按严重程度排序
ops-sort-creation = 最新优先

ops-targets-address = 地址、范围或名称
ops-targets-hostname = 主机名
ops-targets-os = 系统
ops-targets-tags = 标签，用逗号分隔
ops-targets-paste-placeholder = 10.0.0.5
    10.0.1.0/24
    dc01.corp.local
ops-targets-add-button = 添加目标
ops-targets-manual = 添加一个目标
ops-targets-add = 添加
ops-targets-list = 粘贴列表或读取文件
ops-targets-hint = 每行一个目标：地址、范围（CIDR）或名称，后面可带主机名。# 开始注释。带表头的 CSV 按表头读取：ip 或 address、hostname 或 name、os、tags、notes（逗号、分号或制表符）。
ops-targets-preview-button = 预览
ops-targets-file = 选择文本或 CSV 文件…
ops-targets-known = 此目标已在任务中。
ops-targets-created = 已创建目标：{ $count }。
ops-targets-preview = 将创建：{ $create }。已存在：{ $known }。已拒绝：{ $refused }。
ops-targets-more = 另有 { $count } 个
ops-targets-known-title = 已存在，保持原样
ops-targets-refused-title = 已拒绝
ops-targets-refused-line = 第 { $line } 行：{ $text }（{ $reason }）
ops-targets-confirm = 创建目标（{ $count }）
ops-targets-reason-address = 不是地址、范围或名称
ops-targets-reason-name = 不是主机名
ops-targets-reason-extra = 地址和名称之后还有文字
ops-targets-reason-long = 过长
ops-targets-reason-repeated = 重复列出
ops-targets-reason-many = 一个列表中的目标过多
ops-targets-reason-none = 没有地址

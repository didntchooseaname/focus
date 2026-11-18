graph-title = 图谱
graph-empty = 还没有可绘制的笔记。
graph-stats =
    { $notes ->
       *[other] { $notes } 篇笔记
    }，{ $links ->
       *[other] { $links } 条链接
    }
graph-filter-query = 按查询筛选
graph-filter-folders = 文件夹
graph-filter-orphans = 孤立笔记
graph-show-tags = 显示标签节点
graph-show-unresolved = 显示未解析的链接
graph-show-attachments = 显示附件
graph-depth = 深度 { $depth }
graph-find-node = 查找节点
graph-forces = 力
graph-force-repulsion = 斥力
graph-force-distance = 链接距离
graph-force-gravity = 引力
graph-settling = 正在布局图谱…
graph-node-degree =
    { $incoming ->
       *[other] { $incoming } 条入链
    }，{ $outgoing ->
       *[other] { $outgoing } 条出链
    }
graph-controls = 图谱设置
graph-zoom-in = 放大
graph-zoom-out = 缩小
graph-zoom-fit = 适应视图
graph-zoom-level = { $percent }%
graph-find-none = 没有与“{ $query }”匹配的节点
graph-animate = 动画
graph-animate-preparing = 正在准备回放…
graph-animate-play = 播放
graph-animate-pause = 暂停
graph-animate-end = 完整图谱
graph-animate-speed-half = 0.5x
graph-animate-speed-one = 1x
graph-animate-speed-two = 2x
graph-animate-speed-four = 4x

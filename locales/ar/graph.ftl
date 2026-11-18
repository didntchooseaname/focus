graph-title = الرسم البياني
graph-empty = لا توجد ملاحظات للرسم بعد.
graph-stats =
    { $notes ->
        [zero] لا ملاحظات
        [one] ملاحظة واحدة
        [two] ملاحظتان
        [few] { $notes } ملاحظات
        [many] { $notes } ملاحظة
       *[other] { $notes } ملاحظة
    }، { $links ->
        [zero] لا روابط
        [one] رابط واحد
        [two] رابطان
        [few] { $links } روابط
        [many] { $links } رابطًا
       *[other] { $links } رابطًا
    }
graph-filter-query = تصفية بالاستعلام
graph-filter-folders = المجلدات
graph-filter-orphans = الملاحظات اليتيمة
graph-show-tags = إظهار عقد الوسوم
graph-show-unresolved = إظهار الروابط غير المحلولة
graph-show-attachments = إظهار المرفقات
graph-depth = العمق { $depth }
graph-find-node = البحث عن عقدة
graph-forces = القوى
graph-force-repulsion = التنافر
graph-force-distance = مسافة الرابط
graph-force-gravity = الجاذبية
graph-settling = جارٍ تخطيط الرسم البياني…
graph-node-degree =
    { $incoming ->
        [zero] لا روابط واردة
        [one] رابط وارد واحد
        [two] رابطان واردان
        [few] { $incoming } روابط واردة
        [many] { $incoming } رابطًا واردًا
       *[other] { $incoming } رابطًا واردًا
    }، { $outgoing ->
        [zero] لا روابط صادرة
        [one] رابط صادر واحد
        [two] رابطان صادران
        [few] { $outgoing } روابط صادرة
        [many] { $outgoing } رابطًا صادرًا
       *[other] { $outgoing } رابطًا صادرًا
    }
graph-controls = إعدادات الرسم البياني
graph-zoom-in = تكبير
graph-zoom-out = تصغير
graph-zoom-fit = عرض الكل
graph-zoom-level = { $percent }٪
graph-find-none = لا توجد عقدة تطابق «{ $query }»
graph-animate = تحريك
graph-animate-preparing = جارٍ تحضير الإعادة…
graph-animate-play = تشغيل
graph-animate-pause = إيقاف مؤقت
graph-animate-end = الرسم البياني كاملًا
graph-animate-speed-half = 0.5x
graph-animate-speed-one = 1x
graph-animate-speed-two = 2x
graph-animate-speed-four = 4x

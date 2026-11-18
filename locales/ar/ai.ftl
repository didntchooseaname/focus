ai-title = الذكاء الاصطناعي
ai-input-placeholder = اسأل عن ملاحظاتك
ai-send = إرسال
ai-stop = إيقاف
ai-retry = إعادة المحاولة
ai-copy = نسخ
ai-insert-at-cursor = إدراج عند المؤشر
ai-save-as-note = حفظ كملاحظة
ai-new-conversation = محادثة جديدة
ai-history = السجل
ai-history-empty = لا توجد محادثات بعد.
ai-reasoning = الاستدلال
ai-reasoning-show = إظهار الاستدلال
ai-reasoning-hide = إخفاء الاستدلال
ai-generating = جارٍ التوليد…
ai-tool-call = استدعاء أداة: { $name }
ai-tool-search-notes = البحث في الملاحظات
ai-tool-read-note = قراءة ملاحظة
ai-tool-list-folder = سرد محتويات مجلد
ai-tool-get-backlinks = قراءة الروابط الراجعة
ai-tool-get-outline = قراءة مخطط تفصيلي
ai-tool-list-tags = سرد الوسوم
ai-confirm-title = أكّد هذا التغيير
ai-confirm-apply = تطبيق
ai-confirm-reject = رفض
ai-confirm-always-allow = السماح دائمًا بهذه الأداة لـ { $server }
ai-diff-title = الفروقات
ai-context-note = الملاحظة الحالية
ai-context-selection = التحديد
ai-context-added = أُضيف بالرمز @
ai-context-remove = إزالة من السياق
ai-context-tokens =
    { $count ->
        [zero] لا رموز
        [one] نحو رمز واحد
        [two] نحو رمزين
        [few] نحو { $count } رموز
        [many] نحو { $count } رمزًا
       *[other] نحو { $count } رمز
    }
ai-usage-tokens =
    { $count ->
        [zero] لا رموز
        [one] رمز واحد
        [two] رمزان
        [few] { $count } رموز
        [many] { $count } رمزًا
       *[other] { $count } رمز
    }
ai-usage-cached = { $percent } % قُدّمت من ذاكرة التخزين المؤقت
ai-usage-cost = التكلفة التقديرية { $cost }
ai-balance = الرصيد: { $amount }
ai-redaction-notice =
    { $count ->
        [zero] لم يُخفَ أي سر قبل الإرسال
        [one] أُخفي سر واحد قبل الإرسال
        [two] أُخفي سران قبل الإرسال
        [few] أُخفيت { $count } أسرار قبل الإرسال
        [many] أُخفي { $count } سرًا قبل الإرسال
       *[other] أُخفي { $count } سر قبل الإرسال
    }
ai-connection-ok = الاتصال يعمل. الرصيد: { $amount }.
ai-connection-testing = جارٍ اختبار الاتصال…
ai-mcp-state-running = قيد التشغيل
ai-mcp-state-stopped = متوقف
ai-mcp-state-error = خطأ
ai-mcp-tools =
    { $count ->
        [zero] لا أدوات
        [one] أداة واحدة
        [two] أداتان
        [few] { $count } أدوات
        [many] { $count } أداة
       *[other] { $count } أداة
    }
ai-actions-edit = تحرير الإجراءات
ai-action-duplicate = تكرار
ai-action-hide = إخفاء
ai-action-restore = استعادة
ai-action-builtin = مضمّن
ai-action-ops-only = وضع Ops فقط

ai-action-summarize-label = لخّص
ai-action-summarize-desc = الفكرة المركزية والنقاط الرئيسية للتحديد.
ai-action-refine-label = حسّن
ai-action-refine-desc = يحسّن الوضوح والبنية دون تغيير المحتوى.
ai-action-rephrase-label = أعد الصياغة
ai-action-rephrase-desc = يقول الشيء نفسه بكلمات أخرى.
ai-action-correct-label = صحّح
ai-action-correct-desc = يصحح الإملاء والنحو وعلامات الترقيم.
ai-action-translate-fr-label = ترجم إلى الفرنسية
ai-action-translate-fr-desc = يُبقي Markdown والشيفرة والروابط كما هي.
ai-action-translate-en-label = ترجم إلى الإنجليزية
ai-action-translate-en-desc = يُبقي Markdown والشيفرة والروابط كما هي.
ai-action-continue-label = أكمل
ai-action-continue-desc = يكتب الفقرة التالية بالنبرة نفسها.
ai-action-explain-label = اشرح
ai-action-explain-desc = يشرح النص لقارئ يعرف المجال.
ai-action-tasks-label = استخرج المهام
ai-action-tasks-desc = يحوّل النص إلى قائمة مهام بصيغة Markdown.
ai-action-title-tags-label = اقترح عنوانًا ووسومًا
ai-action-title-tags-desc = عنوان قصير وثلاثة إلى ستة وسوم للملاحظة.
ai-action-mermaid-label = أنشئ مخطط Mermaid
ai-action-mermaid-desc = يرسم بنية النص في مخطط.
ai-action-note-from-outline-label = أنشئ ملاحظة من مخطط تفصيلي
ai-action-note-from-outline-desc = يكتب ملاحظة كاملة من المخطط التفصيلي المحدد.
ai-action-ops-finding-label = اكتب اكتشافًا
ai-action-ops-finding-desc = الوصف والأثر والأدلة والمعالجة والمراجع انطلاقًا من ملاحظاتك.
ai-action-ops-exec-summary-label = ملخص تنفيذي
ai-action-ops-exec-summary-desc = ملخص المهمة لقارئ غير تقني.
ai-action-ops-next-enum-label = خطوات الاستكشاف التالية
ai-action-ops-next-enum-desc = الخطوات التالية مرتبة من الأجدى إلى الأقل جدوى.

ai-output-chat = عرض في المحادثة
ai-output-replace = استبدال النص
ai-output-insert = إدراج في الأسفل
ai-output-note = إنشاء ملاحظة

ai-model = النموذج
ai-actions = الإجراءات
ai-add-context = إضافة ملاحظة إلى السياق
ai-empty-title = اسأل عن ملاحظاتك
ai-empty-hint = أضف ملاحظات إلى السياق بالرمز @، أو شغّل إجراءً على الملاحظة الحالية.
ai-no-key-title = لا يوجد مفتاح API
ai-no-key-body = أضف مفتاح { $provider } API في الإعدادات لطرح الأسئلة وتشغيل الإجراءات.
ai-no-key-open = فتح الإعدادات
ai-history-untitled = محادثة بلا عنوان
ai-history-delete = حذف المحادثة
ai-tool-refused = مرفوض
ai-tool-failed = فشل
ai-tool-no-output = لم تُرجع الأداة شيئًا.
ai-proposal-replace = الاستبدال المقترح
ai-confirm-applied = طُبّق
ai-confirm-rejected = مرفوض
ai-diff-more = أسطر غير معروضة: { $count }
ai-usage-input = { $count } مُدخلة
ai-usage-output = { $count } مُخرجة
ai-usage-cache = { $percent } % من الذاكرة المؤقتة
ai-stopped = أُوقف
ai-copied = تم النسخ
ai-scroll-bottom = الانتقال إلى الأسفل
ai-scroll-bottom-new = الانتقال إلى الأسفل: يوجد نص جديد
ai-action-needs-selection = حدّد نصًا أولًا
ai-action-needs-note = افتح ملاحظة أولًا
ai-needs-key = أضف مفتاح API في الإعدادات
ai-actions-running = انتظر انتهاء الإجابة
ai-masking-active = تُحجب الأسرار قبل الإرسال
ai-mention-empty = لا توجد ملاحظة مطابقة.
ai-actions-target-selection = يُطبَّق على التحديد
ai-actions-target-note = يُطبَّق على الملاحظة
ai-actions-no-target = افتح ملاحظة أو حدّد نصًا لاستخدام الإجراءات.
ai-error-missing-key = لم يُعيَّن مفتاح { $provider } API. أضفه من الإعدادات › الذكاء الاصطناعي.
ai-error-invalid-key = رفض { $provider } مفتاح API (401). تحقق منه في الإعدادات › الذكاء الاصطناعي.
ai-error-balance = رصيد { $provider } غير كافٍ (402). اشحن الحساب ثم أعد المحاولة.
ai-error-rate-limit = تلقى { $provider } طلبات كثيرة جدًا (429). أعد المحاولة بعد { $seconds } ث.
ai-error-bad-request = رفض { $provider } الطلب ({ $status }): { $message }
ai-error-server = خدمة { $provider } غير متاحة ({ $status }). أعد المحاولة بعد قليل.
ai-error-network = تعذّر على Focus الوصول إلى { $provider }: { $message }. تحقق من الاتصال ثم أعد المحاولة.
ai-error-protocol = أرسل { $provider } ردًا لا يستطيع Focus قراءته: { $message }. أعد المحاولة.
ai-error-refused = رفض النموذج هذا الطلب (الفئة: { $category }). أعد صياغته أو اختر نموذجًا آخر.
ai-error-no-model = لم يُحدَّد نموذج لـ { $provider } ولا يمكن اختيار أحدها نيابةً عنك. حدّد النموذج في الإعدادات › الذكاء الاصطناعي ثم أعد المحاولة.
ai-tools-disabled = هذا النموذج لا يقبل الأدوات: أعاد Focus إرسال الطلب بدونها.
ai-link-unresolved = لا توجد ملاحظة تطابق { $target }.
ai-mcp-failed = لم يبدأ خادم MCP ‏{ $server }: { $reason }
ai-usage-conversation = هذه المحادثة: { $tokens } رمزًا، التكلفة التقديرية { $cost }.
ai-ops-reminder = وضع Ops: السؤال والسياق ونتائج الأدوات تُرسل إلى { $provider }.
ai-ops-reminder-detail = وضع Ops: يُرسل سؤالك وشرائح السياق ونتائج الأدوات إلى { $provider }. تُخفى كلمات المرور والتجزئات والمفاتيح والرموز أولًا؛ وتُخفى عناوين IP فقط إذا كان الخيار مفعلًا.
ai-ops-reminder-local = وضع Ops: يذهب السؤال والسياق ونتائج الأدوات إلى { $provider } على هذا الجهاز ({ $url }) ولا تغادره.
ai-ops-reminder-local-detail = وضع Ops: يذهب سؤالك وشارات السياق ونتائج الأدوات إلى { $provider } على هذا الجهاز ({ $url }) ولا تغادره. لا يُرسل شيء عبر الشبكة.
ai-provider-local = يبقى الطلب على هذا الجهاز (العنوان { $url }).

errors-core-io = تعذر الوصول إلى { $path }: { $detail }. تحقق من وجود الملف ومن صلاحية الكتابة عليه.
errors-core-outside-vault = المسار { $path } خارج الخزنة. اختر موقعًا داخل الخزنة.
errors-core-already-exists = { $path } موجود مسبقًا. اختر اسمًا آخر.
errors-core-unknown-note = لم تعد هذه الملاحظة موجودة في الخزنة. حدّث الشجرة.
errors-core-conflict = تغير { $path } على القرص منذ آخر قراءة له. أعد تحميله أو احتفظ بنسختك.
errors-core-other = { $message }

errors-search-index-build = تعذر بناء فهرس البحث: { $detail }
errors-search-index-write = تعذر حفظ فهرس البحث: { $detail }

errors-ai-missing-key = لا يوجد مفتاح API. أضف مفتاح DeepSeek في الإعدادات، قسم الذكاء الاصطناعي.
errors-ai-invalid-key = رفض DeepSeek مفتاح API (401). تحقق منه في الإعدادات، قسم الذكاء الاصطناعي.
errors-ai-insufficient-balance = رصيد DeepSeek غير كافٍ (402). اشحن الحساب ثم حاول مرة أخرى.
errors-ai-rate-limited = بلغت حد الطلبات (429). حاول مرة أخرى بعد { $retry_after_secs } ث.
errors-ai-bad-request = رفض DeepSeek الطلب ({ $status }): { $message }
errors-ai-server = خادم DeepSeek غير متاح ({ $status }). حاول مرة أخرى بعد قليل.
errors-ai-network = خطأ في الشبكة: { $detail }. تحقق من اتصالك.
errors-ai-protocol = تعذرت قراءة الاستجابة: { $detail }
errors-ai-cancelled = أُلغي

errors-ai-mcp-invalid-config = خادم MCP "{ $server }": إعداد غير صالح: { $reason }
errors-ai-mcp-spawn = خادم MCP "{ $server }": تعذر تشغيل `{ $command }`: { $reason }. تحقق من الأمر في الإعدادات، قسم الذكاء الاصطناعي.
errors-ai-mcp-timeout = خادم MCP "{ $server }": لا إجابة على `{ $method }` بعد { $secs } ث. { $stderr }
errors-ai-mcp-exited = خادم MCP "{ $server }": توقفت العملية. { $detail }
errors-ai-mcp-protocol = خادم MCP "{ $server }": { $reason }
errors-ai-mcp-cancelled = أُلغي

errors-ops-io = تعذر الوصول إلى { $path }: { $detail }. تحقق من وجود الملف ومن صلاحية الكتابة عليه.
errors-ops-already-exists = المهمة { $path } موجودة مسبقًا. اختر اسمًا آخر.
errors-ops-parse = تعذرت قراءة { $file }: { $message }
errors-ops-invalid = { $message }
errors-ops-conflict = تغيّر { $path } على القرص منذ قراءته. أعد تحميله قبل الحفظ.

errors-md-unsupported = نوع هذا المخطط غير مدعوم: { $kind }. يُعرض المصدر بدلًا منه.
errors-md-syntax = صيغة Mermaid غير صالحة: { $detail }
errors-md-render = تعذر رسم المخطط: { $detail }

errors-mcp-not-running = تطبيق Focus غير مفتوح على هذه الخزنة. افتحه ثم حاول مرة أخرى.
errors-mcp-failed = { $message }

errors-vault-open = تعذر فتح الخزنة: { $detail }
errors-folder-picker = فشل منتقي المجلدات: { $detail }
errors-theme = تعذر تطبيق السمة: { $detail }
errors-settings-read = تعذرت قراءة ملف الإعدادات: { $detail }. تُستخدم القيم الافتراضية.
errors-settings-write = تعذر حفظ الإعدادات: { $detail }
errors-update-network = لم يُجب خادم الإصدارات. تحقق من الاتصال ثم أعد المحاولة.
errors-update-unsigned = أحدث إصدار غير موقَّع. لم يُثبَّت شيء.
errors-update-signature = الإصدار غير موقَّع بمفتاح نشر Focus. لم يُثبَّت شيء.
errors-update-damaged = الملف المنزَّل لا يطابق الإصدار الموقَّع. لم يُثبَّت شيء.
errors-update-missing = لا يحتوي هذا الإصدار على ملف لهذا التثبيت. احصل عليه من صفحة الإصدار.
errors-update-write = تعذّر على Focus الكتابة في { $folder } ({ $detail }). احصل على الإصدار الجديد من صفحة الإصدار.
errors-translation-file = تم تجاهل ملف الترجمة ({ $file }): { $detail }
errors-ai-keystore = تعذّر حفظ مفتاح API في سلسلة مفاتيح النظام: { $detail }
errors-copy-note = تعذّر نسخ الملاحظة: { $detail }
errors-export-report = تعذّر تصدير التقرير: { $detail }
clipboard-timeout = لم تستجب الحافظة خلال 3 ثوانٍ.
clipboard-too-large = تحتوي الحافظة على أكثر من { $size }: لم يُلصق شيء.
clipboard-failed = تعذّر على { $tool } قراءة الحافظة.
clipboard-start = تعذّر تشغيل { $tool } لقراءة الحافظة.

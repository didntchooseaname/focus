ops-title = Ops
ops-new-engagement = مهمة جديدة
ops-switch-engagement = تبديل المهمة
ops-no-engagement = لا توجد مهمة نشطة
ops-dashboard = لوحة المهمة
ops-set-target = تعيين كهدف
ops-target = الهدف
ops-variables = المتغيرات
ops-report-export-sysreptor = تصدير إلى SysReptor
ops-sysreptor-written = كُتب مشروع SysReptor في { $path }. استورده في SysReptor كمشروع. ما تُرك أو عُدّل:
ops-sysreptor-failed = فشل التصدير إلى SysReptor: { $detail }
ops-proofs-name = اسم الإثبات (local.txt, proof.txt...)
ops-proofs-value = القيمة
ops-proofs-machine = جهاز الامتحان
ops-proofs-title = الإثباتات ({ $count })
ops-proofs-add = إضافة الإثبات
ops-timer = المؤقّت
ops-timer-start = بدء المؤقّت
ops-timer-pause = إيقاف مؤقت
ops-timer-resume = استئناف
ops-timer-stop = إيقاف
ops-timer-stopped = توقّف بعد { $duration } من العمل.
ops-trash = نقل إلى سلة المهملات
ops-trash-title = نقل { $name } إلى سلة المهملات؟
ops-trash-confirm-vault = ينتقل مجلد { $name } إلى سلة مهملات الخزنة (المجلد .trash) حيث يمكنك استعادته. لا يُحذف.
ops-trash-confirm-system = ينتقل مجلد { $name } إلى سلة مهملات النظام حيث يمكنك استعادته. لا يُحذف.
ops-trash-do = نقل إلى سلة المهملات
ops-var-credential-pick = اختيار من بيانات الاعتماد
ops-var-credential-linked = يتبع بيانات اعتماد
ops-var-credential-unlink = التوقف عن تتبع بيانات الاعتماد
ops-credentials-use = استخدام بيانات الاعتماد هذه للمتغيرات
ops-credentials-used = المتغيرات المعيّنة: { $names }.
ops-commands-show-secrets = إظهار الأسرار في الأوامر
ops-commands-hide-secrets = إخفاء الأسرار في الأوامر
ops-substitute-hint = استبدال المتغيرات في الأوامر
ops-context = السياق
ops-copy-resolved = نسخ بعد الحل
ops-copy-raw = نسخ خام
ops-open-source-note = فتح الملاحظة المصدر

ops-kind-external-pentest = اختبار اختراق خارجي
ops-kind-internal-pentest = اختبار اختراق داخلي
ops-kind-web-pentest = اختبار اختراق ويب
ops-kind-red-team = الفريق الأحمر
ops-kind-lab = مختبر
ops-kind-exam = اختبار

ops-status-planning = التحضير
ops-status-in-progress = قيد التنفيذ
ops-status-reporting = التقرير
ops-status-done = منتهية

ops-host-status-new = جديد
ops-host-status-enum = تم استكشافه
ops-host-status-foothold = موطئ قدم
ops-host-status-user = مستخدم
ops-host-status-owned = تمت السيطرة

ops-severity-info = معلومة
ops-severity-low = منخفضة
ops-severity-medium = متوسطة
ops-severity-high = عالية
ops-severity-critical = حرجة

ops-finding-status-draft = مسودة
ops-finding-status-confirmed = مؤكد
ops-finding-status-reported = مُبلَّغ عنه
ops-finding-status-fixed = مُعالَج
ops-finding-status-accepted = مقبول

ops-section-overview = نظرة عامة
ops-section-journal = السجل
ops-section-hosts = الأجهزة
ops-section-findings = الاكتشافات
ops-section-credentials = بيانات الاعتماد
ops-section-checklists = قوائم التحقق
ops-section-commands = مكتبة الأوامر
ops-section-exam = الاختبار
ops-section-report = التقرير
ops-section-evidence = الأدلة
ops-section-scope = النطاق

ops-journal-add = إضافة إدخال
ops-journal-placeholder = ما حدث وعلى أي هدف
ops-journal-filter = تصفية السجل
ops-journal-export-csv = تصدير بصيغة CSV
ops-journal-count =
    { $count ->
        [zero] لا إدخالات
        [one] إدخال واحد
        [two] إدخالان
        [few] { $count } إدخالات
        [many] { $count } إدخالًا
       *[other] { $count } إدخال
    }

ops-hosts-column-address = العنوان
ops-hosts-column-name = الاسم
ops-hosts-column-os = النظام
ops-hosts-column-status = الحالة
ops-hosts-column-ports = المنافذ
ops-hosts-import-nmap = استيراد فحص nmap
ops-hosts-count =
    { $count ->
        [zero] لا أجهزة
        [one] جهاز واحد
        [two] جهازان
        [few] { $count } أجهزة
        [many] { $count } جهازًا
       *[other] { $count } جهاز
    }

ops-finding-new = اكتشاف جديد
ops-finding-severity = الخطورة
ops-finding-status = الحالة
ops-finding-assets = الأصول المتأثرة
ops-finding-cwe = CWE
ops-finding-cwe-placeholder = إضافة ضعف: رقم أو اسم
ops-finding-cwe-remove = إزالة هذا الضعف
ops-finding-cwe-filter = تصفية حسب CWE
ops-finding-cwe-invalid = { $value } ليس معرّف CWE ولا اسمًا في الفهرس.
ops-finding-cwe-none = لا يوجد ضعف مطابق
ops-finding-body = النص
ops-finding-body-placeholder = اكتب الملاحظة هنا (Markdown)
ops-finding-body-save = حفظ النص
ops-finding-body-reload = إعادة التحميل من الملاحظة
ops-finding-preview = معاينة
ops-finding-attack = تقنيات ATT&CK
ops-finding-score = درجة CVSS
ops-finding-vector = متجه CVSS
ops-finding-count =
    { $count ->
        [zero] لا اكتشافات
        [one] اكتشاف واحد
        [two] اكتشافان
        [few] { $count } اكتشافات
        [many] { $count } اكتشافًا
       *[other] { $count } اكتشاف
    }

ops-cvss-calculator = حاسبة CVSS
ops-cvss-version = الإصدار { $version }
ops-cvss-invalid-vector = متجه CVSS هذا غير صالح.

ops-credentials-column-user = المستخدم
ops-credentials-column-secret = السر
ops-credentials-column-source = المصدر
ops-credentials-add = إضافة بيانات اعتماد
ops-credentials-reveal = إظهار
ops-credentials-hide = إخفاء

ops-checklist-progress = اكتمل { $done } من { $total }
ops-checklist-next-step = الخطوة التالية
ops-checklist-from-note = استخدام ملاحظة كقالب

ops-exam-profile = ملف الاختبار
ops-exam-time-left = المتبقي { $duration }
ops-exam-time-over = انتهى الوقت
ops-exam-alert = المتبقي أقل من { $minutes } د
ops-exam-points =
    { $points ->
        [zero] لا نقاط
        [one] نقطة واحدة
        [two] نقطتان
        [few] { $points } نقاط
        [many] { $points } نقطة
       *[other] { $points } نقطة
    }
ops-exam-phase-practical = المرحلة العملية
ops-exam-phase-report = مرحلة التقرير
ops-exam-rules = القواعد

ops-report-build = إنشاء التقرير
ops-report-export-html = تصدير بصيغة HTML
ops-report-export-pdf = تصدير بصيغة PDF
ops-evidence-paste = لصق لقطة شاشة كدليل
ops-reminder-sent-data = ما يغادر جهازك

ops-stat-hosts = الأجهزة
ops-stat-owned = تمت السيطرة
ops-stat-findings = الاكتشافات
ops-stat-critical = حرجة
ops-stat-checklist = قائمة التحقق
ops-latest-events = آخر الأحداث

ops-exam-oscp-rule-1 = الاختبار تحت المراقبة.
ops-exam-oscp-rule-2 = ملفا الإثبات local.txt وproof.txt: يُقرآن في صدفة تفاعلية على الهدف بالأمر type أو cat من موقعهما الأصلي، ولا تكفي صدفة ويب. يظهر كل إثبات في لقطة شاشة داخل التقرير.
ops-exam-oscp-rule-3 = على Windows: صدفة SYSTEM أو Administrator أو بصلاحية مسؤول، وعلى Linux: صدفة root.
ops-exam-oscp-rule-4 = Metasploit وMeterpreter: جهاز هدف واحد من اختيارك، يُثبَّت عند أول استخدام.
ops-exam-oscp-rule-5 = ممنوع: انتحال الهوية (IP وARP وDNS وNBNS)، والأدوات التجارية، والاستغلال الآلي، وماسحات الثغرات الجماعية، ومساعدو الذكاء الاصطناعي ونماذج اللغة.
ops-exam-oscp-rule-6 = التقرير: كل هجوم موثّق خطوة بخطوة، ويُسلَّم خلال 24 ساعة بعد الاختبار، والتسليم نهائي.
ops-exam-osep-rule-1 = الاختبار تحت المراقبة.
ops-exam-osep-rule-2 = ملفا الإثبات local.txt وproof.txt: يُقرآن في صدفة تفاعلية على الهدف، ولا تكفي صدفة ويب أو جلسة RDP.
ops-exam-osep-rule-3 = التقرير بصيغة PDF داخل أرشيف .7z، والبرامج النصية وإثباتات المفهوم منسوخة كنص داخل PDF، والتسليم نهائي.
ops-exam-osep-rule-4 = غياب الإثبات عن لقطة الشاشة يعني صفر نقاط للهدف.
ops-exam-oswe-rule-1 = الاختبار تحت المراقبة.
ops-exam-oswe-rule-2 = التقرير بصيغة PDF داخل أرشيف .7z، وبرنامج إثبات المفهوم النصي ملصوق كنص (أو بصيغة base64) داخل PDF، والتسليم نهائي.
ops-exam-oswe-rule-3 = قد يخفض نقص الإثبات أو التوثيق نقاط هدف ما أو يلغيها.
ops-exam-osed-rule-1 = الاختبار تحت المراقبة.
ops-exam-osed-rule-2 = ثلاث مهام، ولا تُمنح النقاط إلا عن مهمة مكتملة بالكامل.
ops-exam-osed-rule-3 = إثبات واحد لكل هدف، يُرسل في لوحة التحكم ويظهر في لقطة شاشة.
ops-exam-osed-rule-4 = تقرير يصف اكتشاف كل مهمة محلولة واستغلالها، والتسليم نهائي.
ops-exam-crtp-rule-1 = في نهاية الاختبار: حلول مفصلة للتحديات وإجراءات تخفيف عملية يجب تسليمها.
ops-exam-crtp-rule-2 = محاولة واحدة مشمولة، وتُعاقب الممارسات غير النزيهة بفترة انتظار مدتها ستة أشهر.
ops-exam-crte-rule-1 = في نهاية الاختبار: حلول مفصلة للتحديات وإجراءات تخفيف عملية يجب تسليمها.
ops-exam-crte-rule-2 = بيئة تضم عدة نطاقات وغابات.
ops-exam-crto-rule-1 = 48 ساعة من تشغيل المختبر، تُستخدم ضمن نافذة مدتها 4 أيام.
ops-exam-crto-rule-2 = راية واحدة لكل جهاز، تُرسل إلى لوحة النتائج: 6 من 8 للنجاح. التقييم بالرايات وليس بالنقاط.
ops-exam-pnpt-rule-1 = من الاستخبارات مفتوحة المصدر إلى اختراق متحكم نطاق الاختبار.
ops-exam-pnpt-rule-2 = تقرير كتابي احترافي، ثم مناقشة شفهية مدتها 15 دقيقة أمام المقيّمين.
ops-exam-cpts-rule-1 = تقرير بمستوى تجاري يُرفع على صفحة الاختبار خلال 10 أيام من دخول الاختبار.
ops-exam-cpts-rule-2 = عدة رايات تُرسل على صفحة الاختبار، ويلزم حد أدنى من النقاط مع التقرير.
ops-exam-cpts-rule-3 = التقرير المسلَّم بعد الموعد النهائي يُسقط قسيمة الاختبار، دون محاولة ثانية.
ops-exam-bscp-rule-1 = تطبيقان من ثلاث مراحل لكل منهما: الدخول إلى حساب، والدخول إلى واجهة الإدارة، وقراءة /home/carlos/secret.
ops-exam-bscp-rule-2 = يجب استخدام ملف مشروع Burp طوال الاختبار وإرساله للتحليل.
ops-exam-bscp-rule-3 = لا مساعدة من الخارج، ولا تشارك عناوين الاختبار.
ops-exam-ejpt-rule-1 = حد أدنى للدرجة 70 % في المجموع وحد أدنى في كل مجال.
ops-exam-ejpt-rule-2 = أسئلة اختيار من متعدد ومختبرات، وتُصحح آليًا.
ops-exam-ecppt-rule-1 = حد أدنى للدرجة 70 % في المجموع، ويُصحح آليًا.
ops-exam-ecppt-rule-2 = قسيمة صالحة 180 يومًا، وإعادة مجانية واحدة خلال 14 يومًا.
ops-exam-oscp-machine-1 = جهاز مستقل 1
ops-exam-oscp-machine-2 = جهاز مستقل 2
ops-exam-oscp-machine-3 = جهاز مستقل 3
ops-exam-oscp-machine-4 = مجموعة Active Directory، الجهاز 1
ops-exam-oscp-machine-5 = مجموعة Active Directory، الجهاز 2
ops-exam-oscp-machine-6 = مجموعة Active Directory، الجهاز 3
ops-exam-milestone-initial-access = الوصول الأولي (local.txt)
ops-exam-milestone-privilege-escalation = رفع الامتيازات (proof.txt)

ops-cvss-metric-av = متجه الهجوم
ops-cvss-metric-ac = تعقيد الهجوم
ops-cvss-metric-at = متطلبات الهجوم
ops-cvss-metric-pr = الامتيازات المطلوبة
ops-cvss-metric-ui = تفاعل المستخدم
ops-cvss-metric-s = النطاق
ops-cvss-metric-c = السرية
ops-cvss-metric-i = السلامة
ops-cvss-metric-a = التوافر
ops-cvss-metric-vc = سرية النظام المعرّض للثغرة
ops-cvss-metric-vi = سلامة النظام المعرّض للثغرة
ops-cvss-metric-va = توافر النظام المعرّض للثغرة
ops-cvss-metric-sc = سرية النظام اللاحق
ops-cvss-metric-si = سلامة النظام اللاحق
ops-cvss-metric-sa = توافر النظام اللاحق
ops-cvss-value-av-n = الشبكة
ops-cvss-value-av-a = المجاور
ops-cvss-value-av-l = المحلي
ops-cvss-value-av-p = المادي
ops-cvss-value-ac-l = منخفض
ops-cvss-value-ac-h = عالٍ
ops-cvss-value-at-n = بلا
ops-cvss-value-at-p = موجودة
ops-cvss-value-pr-n = بلا
ops-cvss-value-pr-l = منخفضة
ops-cvss-value-pr-h = عالية
ops-cvss-value-ui-n = بلا
ops-cvss-value-ui-r = مطلوب
ops-cvss-value-ui-p = سلبي
ops-cvss-value-ui-a = نشط
ops-cvss-value-s-u = دون تغيير
ops-cvss-value-s-c = متغير
ops-cvss-value-c-h = عالية
ops-cvss-value-c-l = منخفضة
ops-cvss-value-c-n = بلا
ops-cvss-value-i-h = عالية
ops-cvss-value-i-l = منخفضة
ops-cvss-value-i-n = بلا
ops-cvss-value-a-h = عالٍ
ops-cvss-value-a-l = منخفض
ops-cvss-value-a-n = بلا
ops-cvss-value-vc-h = عالية
ops-cvss-value-vc-l = منخفضة
ops-cvss-value-vc-n = بلا
ops-cvss-value-vi-h = عالية
ops-cvss-value-vi-l = منخفضة
ops-cvss-value-vi-n = بلا
ops-cvss-value-va-h = عالٍ
ops-cvss-value-va-l = منخفض
ops-cvss-value-va-n = بلا
ops-cvss-value-sc-h = عالية
ops-cvss-value-sc-l = منخفضة
ops-cvss-value-sc-n = بلا
ops-cvss-value-si-h = عالية
ops-cvss-value-si-l = منخفضة
ops-cvss-value-si-n = بلا
ops-cvss-value-sa-h = عالٍ
ops-cvss-value-sa-l = منخفض
ops-cvss-value-sa-n = بلا

ops-var-rhost-desc = عنوان الهدف
ops-var-lhost-desc = عنوان الجهاز المحلي
ops-var-lport-desc = منفذ الاستماع المحلي
ops-var-domain-desc = النطاق
ops-var-user-desc = اسم المستخدم
ops-var-pass-desc = كلمة المرور
ops-var-hash-desc = تجزئة كلمة المرور
ops-var-dc-desc = متحكم النطاق
ops-var-dc_host-desc = اسم مضيف وحدة تحكم النطاق
ops-var-domain_sid-desc = معرّف SID للنطاق
ops-var-url-desc = عنوان ويب الهدف
ops-var-wordlist-desc = ملف قائمة الكلمات
ops-var-hostname-desc = اسم الهدف
ops-var-interface-desc = واجهة الشبكة

ops-checklist-mission-label = سير عمل المهمة
ops-checklist-exam-label = سير عمل الاختبار

ops-cancel = إلغاء
ops-close = إغلاق
ops-commands-count = الأوامر: { $count }
ops-commands-empty = لم يُعثر على أي أمر. يقرأ Focus كتل الشيفرة الخاصة بسطر الأوامر في ملاحظاتك، داخل المجلدات المحددة في الإعدادات.
ops-commands-journaled = أُضيف الأمر إلى اليومية.
ops-commands-no-match = لا يوجد أمر يطابق هذا البحث
ops-commands-search = ابحث بالأداة أو الأمر أو العنوان
ops-commands-to-journal = إضافة إلى اليومية
ops-copied = تم النسخ
ops-credentials-column-domain = النطاق
ops-credentials-column-kind = النوع
ops-credentials-copy = نسخ السر
ops-credentials-count = بيانات الاعتماد: { $count }
ops-credentials-duplicate = بيانات الاعتماد هذه مسجّلة مسبقًا.
ops-credentials-empty = لا توجد بيانات اعتماد بعد. أضف واحدة أعلاه.
ops-cvss-apply = تطبيق على الملاحظة المحددة
ops-exam-points-label = النقاط
ops-exam-points-of = { $points } من { $pass } للنجاح (المجموع { $total })
ops-exam-points-pass = { $points } من { $pass } للنجاح
ops-filter-all = الكل
ops-finding-create = إنشاء
ops-finding-title-placeholder = عنوان الملاحظة
ops-finding-vector-copy = نسخ المتجه
ops-findings-empty = لا توجد نتائج لعرضها. أنشئ واحدة عبر نتيجة جديدة.
ops-hosts-empty = لا توجد مضيفات بعد. استورد فحص nmap لملء هذا الجدول.
ops-hosts-import = استيراد
ops-hosts-import-hint = الصق ناتج فحص nmap (XML أو grepable أو عادي).
ops-hosts-import-report = أُنشئ: { $created }، حُدّث: { $updated }، الخدمات المقروءة: { $services }.
ops-hosts-paste-placeholder = الصق الفحص هنا
ops-journal-all-targets = كل الأهداف
ops-journal-empty = اليومية فارغة. أضف إدخالًا من شريط Ops.
ops-journal-no-match = لا يوجد إدخال يطابق هذا المرشّح
ops-loading = جارٍ قراءة المهمة…
ops-new-client = العميل
ops-new-create = إنشاء المهمة
ops-new-kind = نوع المهمة
ops-new-name = الاسم
ops-new-name-required = أعطِ المهمة اسمًا.
ops-new-scope-placeholder = عناوين أو نطاقات
ops-new-template = قالب قائمة المراجعة
ops-new-template-default = الافتراضي لهذا النوع
ops-no-target = لا يوجد هدف
ops-open-note = فتح الملاحظة
ops-exam-rules-count = القواعد: { $count }

ops-credentials-column-valid = صالحة على
ops-host-cited-none = لا شيء يشير إلى هذا المضيف بعد.
ops-host-cited-findings = النتائج التي تشير إلى هذا المضيف
ops-host-cited-credentials = بيانات الاعتماد التي تشير إلى هذا المضيف
ops-journal-copy-link = نسخ رابط هذا الإدخال
ops-report-author = المؤلف
ops-report-empty = أنشئ التقرير لمعاينته هنا.
ops-report-include-credentials = بيانات الاعتماد
ops-report-include-journal = اليومية
ops-report-mask-secrets = حجب الأسرار
ops-report-preview = معاينة
ops-report-secrets-warning = سيحتوي التقرير على أسرار بنص صريح.
ops-report-write = كتابة Report.md
ops-report-written = تمت كتابة { $file }.
ops-new-note-search = ابحث عن ملاحظة تحتوي على مربعات اختيار
ops-var-add = إضافة المتغير
ops-var-invalid-name = يتكوّن اسم المتغير من أحرف وأرقام وشرطات سفلية ولا يبدأ برقم.
ops-var-name-placeholder = الاسم
ops-var-value-placeholder = القيمة

ops-finding-sheet = النتيجة
ops-finding-sheet-empty = حدّد نتيجة لقراءة ملاحظتها.

ops-exam-start = بدء مؤقت الامتحان
ops-exam-not-started = { $minutes } دقيقة، لم يبدأ
ops-finding-edit = تعديل
ops-finding-edit-done = تم
ops-finding-techniques = تقنيات ATT&CK
ops-finding-asset-placeholder = أضف أصلًا (مضيف أو عنوان)
ops-finding-technique-placeholder = أضف تقنية (T1059.001)
ops-finding-asset-remove = إزالة هذا الأصل
ops-finding-technique-remove = إزالة هذه التقنية
ops-finding-technique-invalid = «{ $value }» ليس معرّف تقنية في ATT&CK.
ops-filter-severity = الخطورة: { $value }
ops-filter-status = الحالة: { $value }
ops-scope = النطاق
ops-scope-placeholder = عناوين أو نطاقات، Enter للحفظ
ops-hosts-import-file = اختيار ملف فحص…
ops-sort-severity = مرتبة حسب الخطورة
ops-sort-creation = الأحدث أولًا

ops-targets-address = عنوان أو نطاق أو اسم
ops-targets-hostname = اسم المضيف
ops-targets-os = النظام
ops-targets-tags = وسوم، مفصولة بفواصل
ops-targets-paste-placeholder = 10.0.0.5
    10.0.1.0/24
    dc01.corp.local
ops-targets-add-button = إضافة أهداف
ops-targets-manual = إضافة هدف
ops-targets-add = إضافة
ops-targets-list = لصق قائمة أو قراءة ملف
ops-targets-hint = هدف واحد في كل سطر: عنوان أو نطاق (CIDR) أو اسم، ثم اسم مضيف اختياريًا. يبدأ # تعليقًا. يُقرأ ملف CSV ذو سطر عناوين حسب عناوينه: ip أو address وhostname أو name وos وtags وnotes (فاصلة أو فاصلة منقوطة أو علامة جدولة).
ops-targets-preview-button = معاينة
ops-targets-file = اختيار ملف نصي أو CSV…
ops-targets-known = هذا الهدف موجود في المهمة.
ops-targets-created = الأهداف المنشأة: { $count }.
ops-targets-preview = للإنشاء: { $create }. موجودة: { $known }. مرفوضة: { $refused }.
ops-targets-more = و{ $count } أخرى
ops-targets-known-title = موجودة، تُركت كما هي
ops-targets-refused-title = مرفوضة
ops-targets-refused-line = السطر { $line }: { $text } ({ $reason })
ops-targets-confirm = إنشاء الأهداف ({ $count })
ops-targets-reason-address = ليس عنوانًا ولا نطاقًا ولا اسمًا
ops-targets-reason-name = ليس اسم مضيف
ops-targets-reason-extra = نص بعد العنوان والاسم
ops-targets-reason-long = طويل جدًا
ops-targets-reason-repeated = مذكور مرتين
ops-targets-reason-many = أهداف كثيرة في قائمة واحدة
ops-targets-reason-none = لا عنوان

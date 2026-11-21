search-placeholder = البحث في الخزنة
search-searching = جارٍ البحث…
search-no-results = لا توجد نتائج
search-results-count =
    { $count ->
        [zero] لا نتائج في { $files ->
            [zero] لا ملاحظات
            [one] ملاحظة واحدة
            [two] ملاحظتان
            [few] { $files } ملاحظات
            [many] { $files } ملاحظة
           *[other] { $files } ملاحظة
        }
        [one] نتيجة واحدة في { $files ->
            [zero] لا ملاحظات
            [one] ملاحظة واحدة
            [two] ملاحظتان
            [few] { $files } ملاحظات
            [many] { $files } ملاحظة
           *[other] { $files } ملاحظة
        }
        [two] نتيجتان في { $files ->
            [zero] لا ملاحظات
            [one] ملاحظة واحدة
            [two] ملاحظتان
            [few] { $files } ملاحظات
            [many] { $files } ملاحظة
           *[other] { $files } ملاحظة
        }
        [few] { $count } نتائج في { $files ->
            [zero] لا ملاحظات
            [one] ملاحظة واحدة
            [two] ملاحظتان
            [few] { $files } ملاحظات
            [many] { $files } ملاحظة
           *[other] { $files } ملاحظة
        }
        [many] { $count } نتيجة في { $files ->
            [zero] لا ملاحظات
            [one] ملاحظة واحدة
            [two] ملاحظتان
            [few] { $files } ملاحظات
            [many] { $files } ملاحظة
           *[other] { $files } ملاحظة
        }
   *[other] { $count } نتيجة في { $files ->
            [zero] لا ملاحظات
            [one] ملاحظة واحدة
            [two] ملاحظتان
            [few] { $files } ملاحظات
            [many] { $files } ملاحظة
           *[other] { $files } ملاحظة
        }
    }
search-results-truncated = قُطعت النتائج عند { $limit } ملاحظة. ضيّق الاستعلام.
search-elapsed = { $duration }
search-option-case = مطابقة حالة الأحرف
search-option-word = كلمة كاملة
search-option-regex = تعبير نمطي
search-sort-relevance = الصلة
search-sort-path = المسار
search-sort-newest = الأحدث أولًا
search-sort-oldest = الأقدم أولًا
search-ai-toggle = الذكاء الاصطناعي
search-ai-queries = استعلامات أنشأها النموذج
search-ai-answer = الإجابة
search-ai-citation = المصدر { $number }
search-op-tag = الملاحظات ذات هذا الوسم
search-op-path = الملاحظات التي يتضمن مسارها النص
search-op-file = الملاحظات التي يتضمن اسم ملفها النص
search-op-title = الملاحظات التي يتضمن عنوانها النص
search-op-heading = الملاحظات التي تحتوي على عنوان يتضمن النص
search-op-task = الملاحظات التي تحتوي على مهمة تتضمن النص
search-op-prop = الملاحظات التي تطابق إحدى خصائص frontmatter فيها
search-op-exclude = يستبعد المصطلح
search-op-phrase = عبارة مطابقة تمامًا
search-op-or = أحد المصطلحين
search-op-regex = تعبير نمطي بين شرطتين مائلتين
search-tags-empty = لا توجد وسوم في هذه الخزنة.
search-tag-count =
    { $count ->
        [zero] لا ملاحظات
        [one] ملاحظة واحدة
        [two] ملاحظتان
        [few] { $count } ملاحظات
        [many] { $count } ملاحظة
       *[other] { $count } ملاحظة
    }
search-index-building = جارٍ بناء الفهرس: { $done } من { $total } ملاحظة
search-index-ready = الفهرس جاهز: { $count } ملاحظة
search-empty-title = ابحث في ملاحظاتك
search-empty-hint = اكتب كلمات أو عبارة بين علامتي اقتباس أو عاملًا مثل tag: أو path:.
search-none-hint = جرّب كلمات أقل، أو عطّل مطابقة الحالة والكلمة الكاملة.
search-clear = مسح البحث
search-help-toggle = صياغة البحث
search-help-insert = إدراج هذا المثال
search-sort = ترتيب النتائج
search-collapse = طيّ الملاحظة
search-expand = توسيع الملاحظة
search-hint-open = فتح
search-hint-new-tab = تبويب جديد
search-hint-clear = مسح
search-hint-ask = اسأل المساعد
search-diag-unclosed-quote = علامة اقتباس غير مغلقة
search-diag-invalid-regex = تعبير نمطي غير صالح: { $detail }
search-diag-unknown-operator = معامل غير معروف { $name }: يُبحث عنه كنص
search-diag-missing-value = { $operator } يحتاج إلى قيمة
search-diag-unclosed-parenthesis = قوس غير مغلق
search-diag-unmatched-parenthesis = قوس إغلاق بلا قوس فتح
search-ai-toggle-tip = البحث المساعَد
search-ai-intent = فُهم على أنه
search-ai-stop = إيقاف
search-ai-stage-expanding = جارٍ كتابة الاستعلامات…
search-ai-stage-searching = جارٍ البحث في الخزنة…
search-ai-stage-ranking = جارٍ ترتيب الملاحظات…
search-ai-stage-answering = جارٍ كتابة الإجابة…
search-ai-stopped = تم الإيقاف
search-ai-nothing = لا توجد ملاحظة تطابق الاستعلامات المولَّدة.
search-ai-cost = حوالي { $amount } دولار أمريكي
search-ai-query-add = إضافة استعلام
search-ai-query-edit = تعديل هذا الاستعلام
search-ai-query-remove = إزالة هذا الاستعلام
search-ai-query-placeholder = استعلام جديد
search-ai-failed-missing-key = لا يوجد مفتاح API. أضف واحدًا في الإعدادات › الذكاء الاصطناعي.
search-ai-failed-invalid-key = رفض DeepSeek مفتاح API (401). تحقق منه في الإعدادات › الذكاء الاصطناعي.
search-ai-failed-balance = رصيد DeepSeek غير كافٍ (402). اشحن الحساب.
search-ai-failed-rate-limit = بلغت حد معدل DeepSeek. أعد المحاولة بعد { $seconds } ثانية.
search-ai-failed-network = تعذّر الوصول إلى DeepSeek. تم الاحتفاظ بالنتائج المحلية.
search-ai-failed-other = فشل المساعد. تم الاحتفاظ بالنتائج المحلية.
search-replace-toggle = استبدال في الخزنة كلها
search-replace-placeholder = استبدال بـ
search-replace-reading = جارٍ قراءة الملاحظات…
search-replace-summary =
    { $lines ->
        [zero] لا أسطر للتعديل، عدد الملاحظات: { $files }
        [one] سطر واحد للتعديل، عدد الملاحظات: { $files }
        [two] سطران للتعديل، عدد الملاحظات: { $files }
        [few] { $lines } أسطر للتعديل، عدد الملاحظات: { $files }
        [many] { $lines } سطرًا للتعديل، عدد الملاحظات: { $files }
       *[other] { $lines } سطر للتعديل، عدد الملاحظات: { $files }
    }
search-replace-kept = أسطر محتفظ بها: { $lines }، في ملاحظات: { $files }
search-replace-apply = تطبيق
search-replace-no-terms = لا يحتوي الاستعلام على نص للاستبدال.
search-replace-done =
    { $count ->
        [zero] لم يتم أي استبدال
        [one] تم استبدال تكرار واحد، عدد الملاحظات: { $files }
        [two] تم استبدال تكرارين، عدد الملاحظات: { $files }
        [few] تم استبدال { $count } تكرارات، عدد الملاحظات: { $files }
        [many] تم استبدال { $count } تكرارًا، عدد الملاحظات: { $files }
       *[other] تم استبدال { $count } تكرار، عدد الملاحظات: { $files }
    }
search-replace-failed =
    { $count ->
        [zero] تمت كتابة كل الملاحظات
        [one] لم تُكتب ملاحظة واحدة
        [two] لم تُكتب ملاحظتان
        [few] لم تُكتب { $count } ملاحظات
        [many] لم تُكتب { $count } ملاحظة
       *[other] لم تُكتب { $count } ملاحظة
    }
search-replace-changed = تغيّرت على القرص منذ المعاينة
search-replace-write-error = تعذّرت الكتابة ({ $detail })
search-replace-failed-note = { $title }: { $reason }
search-replace-unreadable =
    { $count ->
        [zero] لا ملاحظات مستبعدة
        [one] استُبعدت ملاحظة واحدة: الترميز ليس UTF-8 صالحًا
        [two] استُبعدت ملاحظتان: الترميز ليس UTF-8 صالحًا
        [few] استُبعدت { $count } ملاحظات: الترميز ليس UTF-8 صالحًا
        [many] استُبعدت { $count } ملاحظة: الترميز ليس UTF-8 صالحًا
       *[other] استُبعدت { $count } ملاحظة: الترميز ليس UTF-8 صالحًا
    }

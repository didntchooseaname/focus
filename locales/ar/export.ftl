export-title = تصدير
export-format-html = HTML مستقل
export-format-markdown = Markdown قياسي
export-format-pdf = PDF
export-pdf-unavailable = يتطلب تصدير PDF متصفح Chromium ولم يُعثر عليه. صدّر بصيغة HTML بدلًا من ذلك.
export-theme = السمة
export-embed-images = تضمين الصور
export-embed-fonts = تضمين الخطوط
export-done = تم التصدير إلى { $path }
export-failed = فشل التصدير: { $detail }
export-copy-html = نسخ بصيغة HTML
export-copy-markdown = نسخ بصيغة Markdown
export-vault-zip = تصدير الخزنة كأرشيف zip
export-folder-zip = تصدير هذا المجلد كأرشيف zip

export-html-contents = المحتويات
export-html-properties = الخصائص
export-html-footer = صُدّر من Focus في { $date }
export-html-copy = نسخ
export-html-copied = تم النسخ
export-html-fold-all = طي الكل
export-html-unfold-all = فتح الكل
export-html-note-missing = لم يُعثر على الملاحظة
export-html-image-missing = لم يُعثر على الصورة
export-html-diagram = مخطط
export-html-back-to-text = العودة إلى النص
export-html-diagram-failed = لم يُرسم المخطط
export-html-detected = مكتشفة
export-html-footnotes = الحواشي
export-html-backlinks = الروابط الراجعة

import-title = استيراد
import-source-files = ملفات .md و.txt و.html
import-source-folder = مجلد
import-source-zip = أرشيف zip
import-source-notion = تصدير Notion
import-done =
    { $count ->
        [zero] لم تُستورد أي ملاحظة
        [one] استُوردت ملاحظة واحدة
        [two] استُوردت ملاحظتان
        [few] استُوردت { $count } ملاحظات
        [many] استُوردت { $count } ملاحظة
       *[other] استُوردت { $count } ملاحظة
    }
import-failed = فشل الاستيراد: { $detail }
settings-export-done = صُدّرت الإعدادات إلى { $path }
settings-import-invalid = هذا الملف ليس ملف إعدادات صالحًا: { $detail }

transfer-close = إغلاق
transfer-cancel = إلغاء

export-scope = ما سيتم تصديره
export-scope-note = ملاحظة
export-scope-folder = مجلد
export-scope-vault = الخزنة
export-format = الصيغة
export-format-zip = أرشيف zip
export-pdf-one-note = يقبل تصدير PDF ملاحظة واحدة في كل مرة.
export-use-html = التصدير بصيغة HTML بدلًا من ذلك
export-options = الخيارات
export-theme-dark = داكن
export-theme-light = فاتح
export-code-colors = ألوان الشيفرة
export-code-soft = ملوّنة
export-code-mono = أحادية اللون
export-mermaid = رسم مخططات Mermaid
export-destination = مجلد الوجهة
export-choose = اختيار…
export-start = تصدير
export-back = العودة إلى الخيارات
export-cancelled = أُلغي التصدير.
export-running-notes = جارٍ تصدير الملاحظات: { $done } من { $total }
export-running-packing = جارٍ إنشاء الأرشيف…
export-count-notes = الملاحظات
export-count-files = الملفات
export-count-size = الحجم
export-skipped = ملاحظات مستبعدة:
export-skip-outside = خارج الخزنة، تم استبعاده
export-open-folder = فتح المجلد

import-source = المصدر
import-choose = اختيار…
import-detected = تم التعرف على: { $kind }
import-unknown = لم يُعثر في هذا المسار على ما يستطيع Focus استيراده.
import-reading = جارٍ قراءة المصدر…
import-preview = ما سيتم إنشاؤه
import-count-notes = الملاحظات
import-count-attachments = المرفقات
import-count-skipped = المتجاوَزة
import-destination = مجلد الوجهة داخل الخزنة
import-destination-hint = لا يُستبدل أي شيء: الاسم المستخدم يحصل على رقم.
import-start = استيراد
import-running = جارٍ الاستيراد…
import-skipped-title = ملفات مستبعدة:
import-skip-hidden = ملف مخفي
import-skip-symlink = رابط رمزي
import-skip-unsafe = مسار خارج المجلد
import-skip-unsupported = نوع ملف غير مدعوم
import-skip-special = ليس ملفًا عاديًا
import-skip-large = ملف كبير جدًا
import-skip-deep = متداخل بعمق مفرط

attachments-tidy-title = ترتيب المرفقات
attachments-orphans-title = المرفقات اليتيمة
attachments-loading = جارٍ قراءة الخزنة…
attachments-count-renamed = أُعيدت تسميتها
attachments-count-moved = نُقلت
attachments-count-merged = مكرّرات مدمجة
attachments-count-links = روابط أُعيدت كتابتها
attachments-kind-rename = إعادة تسمية
attachments-kind-move = نقل
attachments-kind-merge = دمج
attachments-merge-keeps = يُبقي على { $path }
attachments-apply = تطبيق
attachments-tidy-empty = المرفقات مرتّبة بالفعل.
attachments-tidy-done = تم ترتيب المرفقات.
attachments-skipped = ملفات تُركت في مكانها لأنها تغيّرت منذ المعاينة: { $count }
attachments-orphans-hint = ملفات لا تشير إليها أي ملاحظة. تحتفظ بها سلة المحذوفات في مجلد .trash داخل الخزنة.
attachments-orphans-empty = كل مرفق تستخدمه ملاحظة.
attachments-select-all = تحديد الكل
attachments-selected = المحدَّد
attachments-trash = النقل إلى سلة المحذوفات
attachments-trashed = نُقلت إلى سلة المحذوفات: { $count }
attachments-preview-none = لا توجد معاينة لهذا النوع من الملفات

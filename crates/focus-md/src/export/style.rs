use base64::Engine;

use super::CodeColors;

struct Tokens {
    bg: &'static str,
    bg_2: &'static str,
    card: &'static str,
    card_2: &'static str,
    card_3: &'static str,
    line: &'static str,
    line_2: &'static str,
    hatch: &'static str,
    ink: &'static str,
    ink_body: &'static str,
    ink_2: &'static str,
    ink_3: &'static str,
    ink_4: &'static str,
    btn: &'static str,
    btn_ink: &'static str,
    ok: &'static str,
    warn: &'static str,
    danger: &'static str,
    soft: [&'static str; 4],
}

const DARK: Tokens = Tokens {
    bg: "#070707",
    bg_2: "#0c0c0d",
    card: "#121213",
    card_2: "#18181a",
    card_3: "#202022",
    line: "#232325",
    line_2: "#303033",
    hatch: "#1c1c1e",
    ink: "#f5f5f6",
    ink_body: "#d7d7db",
    ink_2: "#b4b4ba",
    ink_3: "#7d7d85",
    ink_4: "#55555c",
    btn: "#f5f5f6",
    btn_ink: "#0a0a0a",
    ok: "#4ade80",
    warn: "#fbbf24",
    danger: "#f87171",
    soft: ["#9ecbff", "#c9b3ff", "#a5d6a7", "#e6c384"],
};

const LIGHT: Tokens = Tokens {
    bg: "#ffffff",
    bg_2: "#f6f6f7",
    card: "#ffffff",
    card_2: "#f4f4f5",
    card_3: "#ebebed",
    line: "#e6e6e8",
    line_2: "#d6d6da",
    hatch: "#e4e4e7",
    ink: "#0b0b0c",
    ink_body: "#1f1f23",
    ink_2: "#4b4b52",
    ink_3: "#6b6b73",
    ink_4: "#a1a1aa",
    btn: "#0b0b0c",
    btn_ink: "#ffffff",
    ok: "#16a34a",
    warn: "#b45309",
    danger: "#dc2626",
    soft: ["#0550ae", "#6639ba", "#1a7f37", "#953800"],
};

fn variables(t: &Tokens, colors: CodeColors) -> String {
    let (func, ty, string, number) = match colors {
        CodeColors::Soft => (t.soft[0], t.soft[1], t.soft[2], t.soft[3]),
        CodeColors::Mono => (t.ink, t.ink, t.ink_2, t.ink),
    };
    let ty_weight = if colors == CodeColors::Mono { 500 } else { 400 };
    format!(
        "--bg:{};--bg-2:{};--card:{};--card-2:{};--card-3:{};--line:{};--line-2:{};--hatch:{};--ink:{};--ink-body:{};--ink-2:{};--ink-3:{};--ink-4:{};--btn:{};--btn-ink:{};--ok:{};--warn:{};--danger:{};--c-fn:{func};--c-ty:{ty};--c-str:{string};--c-num:{number};--w-ty:{ty_weight};",
        t.bg,
        t.bg_2,
        t.card,
        t.card_2,
        t.card_3,
        t.line,
        t.line_2,
        t.hatch,
        t.ink,
        t.ink_body,
        t.ink_2,
        t.ink_3,
        t.ink_4,
        t.btn,
        t.btn_ink,
        t.ok,
        t.warn,
        t.danger
    )
}

const FONT_FILES: [(&str, &str, &[u8], u16); 4] = [
    (
        "Inter",
        "normal",
        include_bytes!("../../../../assets/fonts/Inter-Regular.ttf"),
        400,
    ),
    (
        "Inter",
        "normal",
        include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf"),
        600,
    ),
    (
        "Sora",
        "normal",
        include_bytes!("../../../../assets/fonts/Sora-SemiBold.ttf"),
        600,
    ),
    (
        "JetBrains Mono",
        "normal",
        include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf"),
        400,
    ),
];

fn font_faces() -> String {
    let engine = base64::engine::general_purpose::STANDARD;
    FONT_FILES
        .iter()
        .map(|(family, style, bytes, weight)| {
            format!(
                "@font-face{{font-family:'{family}';font-style:{style};font-weight:{weight};src:url(data:font/ttf;base64,{}) format('truetype');}}\n",
                engine.encode(bytes)
            )
        })
        .collect()
}

pub(super) fn stylesheet(dark: bool, colors: CodeColors, embed_fonts: bool) -> String {
    let (theme, other) = if dark {
        (&DARK, &LIGHT)
    } else {
        (&LIGHT, &DARK)
    };
    let mut css = String::new();
    if embed_fonts {
        css.push_str(&font_faces());
    }
    css.push_str(&format!(":root{{{}}}\n", variables(theme, colors)));
    css.push_str(&format!(
        "@media print{{:root{{{}}}}}\n",
        variables(&LIGHT, colors)
    ));
    let _ = other;
    css.push_str(BASE);
    css
}

pub(super) fn tabs_css(count: usize) -> String {
    let mut css = String::from(
        r#".tabs{margin:16px 0;border:1px solid var(--line);background:var(--bg);position:relative}
.tabs>input{position:absolute;opacity:0;width:1px;height:1px;margin:0}
.tab-bar{display:flex;height:32px;overflow-x:auto;overflow-y:hidden;border-bottom:1px solid var(--line);scrollbar-width:none}
.tab-bar>label{flex:none;display:flex;align-items:center;padding:0 12px;white-space:nowrap;cursor:pointer;font:500 13px/20px Inter,system-ui,sans-serif;color:var(--ink-3);box-shadow:inset 0 -1px 0 transparent}
.tab-bar>label:hover{color:var(--ink-2)}
.tab-panel{display:none;padding:16px}
.tab-panel>:first-child{margin-top:0}
.tab-panel>:last-child{margin-bottom:0}
.tab-title{display:none}
"#,
    );
    for n in 1..=count {
        css.push_str(&format!(
            ".tabs>input:nth-of-type({n}):checked~.tab-bar>label:nth-child({n}){{color:var(--ink);box-shadow:inset 0 -1px 0 var(--ink)}}\n.tabs>input:nth-of-type({n}):checked~.tab-panels>.tab-panel:nth-child({n}){{display:block}}\n.tabs>input:nth-of-type({n}):focus-visible~.tab-bar>label:nth-child({n}){{outline:1px solid var(--ink);outline-offset:-1px}}\n"
        ));
    }
    css.push_str("@media print{.tab-bar{display:none}.tab-panel{display:block!important;padding:12px 16px}.tab-title{display:block;font:600 13px/20px Inter,system-ui,sans-serif;margin:0 0 8px;color:var(--ink)}.tabs{break-inside:auto}}\n");
    css
}

pub(super) const BASE: &str = r#"
*{box-sizing:border-box}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--ink-body);font:400 15px/25px Inter,system-ui,-apple-system,"Segoe UI",sans-serif;font-feature-settings:"cv11","ss03";-webkit-font-smoothing:antialiased}
@media screen{body{background:repeating-linear-gradient(135deg,var(--bg) 0 11px,var(--hatch) 11px 12px);min-height:100vh}}
.sheet{max-width:856px;margin:0 auto;padding:48px;background:var(--bg);min-height:100vh}
@media screen{.sheet{border-left:1px solid var(--line);border-right:1px solid var(--line)}}
@media (max-width:640px){.sheet{padding:24px 16px}}
.note-title{font:600 34px/40px Sora,Inter,system-ui,sans-serif;letter-spacing:-.03em;color:var(--ink);margin:0 0 16px}
h1,h2,h3,h4,h5,h6{color:var(--ink);margin:0;font-weight:600}
h1{font:600 26px/32px Sora,Inter,system-ui,sans-serif;letter-spacing:-.03em;margin:40px 0 12px}
h2{font:600 20px/28px Sora,Inter,system-ui,sans-serif;letter-spacing:-.02em;margin:32px 0 8px;padding-top:12px;border-top:1px solid var(--line)}
h3{font:600 16.5px/24px Inter,system-ui,sans-serif;margin:24px 0 6px}
h4,h5,h6{font:600 14.5px/22px Inter,system-ui,sans-serif;margin:24px 0 6px}
h5,h6{color:var(--ink-2)}
.sheet>h1:first-child,.sheet>.note-title+h1{margin-top:16px}
p{margin:0 0 12px}
a{color:var(--ink);text-decoration:underline;text-decoration-color:var(--line-2);text-underline-offset:3px;text-decoration-thickness:1px}
a:hover{text-decoration-color:var(--ink)}
a.unresolved,span.unresolved{color:var(--ink-3);text-decoration:underline dotted var(--line-2);text-underline-offset:3px}
a.external::after{content:"";display:inline-block;width:12px;height:12px;margin-inline-start:3px;vertical-align:-1px;background:var(--ink-3);-webkit-mask:url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M15 3h6v6M10 14L21 3M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6'/%3E%3C/svg%3E") center/contain no-repeat;mask:url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M15 3h6v6M10 14L21 3M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6'/%3E%3C/svg%3E") center/contain no-repeat}
code,kbd,samp,pre{font-family:"JetBrains Mono",ui-monospace,"SFMono-Regular",Menlo,Consolas,monospace;font-variant-ligatures:none}
:not(pre)>code{font-size:.88em;background:var(--card-2);border-radius:2px;padding:1px 4px;unicode-bidi:isolate}
mark.hl{background:var(--ink-2);color:var(--btn-ink);padding:0 2px}
.tag{font-size:13px;padding:1px 6px;background:var(--card-2);border:1px solid var(--line);border-radius:2px;color:var(--ink-2);text-decoration:none}
.props .tag{display:inline-block;line-height:20px;padding:0 8px}
.math{font-family:"JetBrains Mono",ui-monospace,monospace;font-size:.88em;background:var(--card-2);border-radius:2px;padding:1px 4px}
.math-block{direction:ltr;text-align:left;margin:16px 0;border:1px solid var(--line);background:var(--bg-2);padding:12px 16px;overflow-x:auto}
.math-block pre{margin:0;font:400 13px/21px "JetBrains Mono",ui-monospace,monospace;color:var(--ink-body)}
hr{border:0;border-top:1px solid var(--line);margin:24px 0}
blockquote{margin:16px 0;padding:0;padding-inline-start:16px;border-inline-start:2px solid var(--line-2);color:var(--ink-2)}
blockquote>:last-child{margin-bottom:0}
ul,ol{margin:0 0 12px;padding-inline-start:24px}
li{margin:2px 0}
li>p{margin:0 0 6px}
li>ul,li>ol{margin:2px 0 4px}
li.task{list-style:none;margin-inline-start:-24px;padding-inline-start:0;display:flex;gap:8px;align-items:flex-start}
li.task>.body{flex:1;min-width:0}
.box{flex:none;width:14px;height:14px;margin-top:5px;border:1px solid var(--line-2);border-radius:2px;display:inline-flex;align-items:center;justify-content:center;font:500 10px/1 "JetBrains Mono",ui-monospace,monospace;color:var(--ink-3)}
li.task[data-state="x"]>.box{background:var(--btn);border-color:var(--btn);color:var(--btn-ink)}
li.task[data-state="x"]>.body{color:var(--ink-3);text-decoration:line-through}
.sheet img{max-width:100%;height:auto}
figure.image{margin:16px 0}
figure.image img{display:block;border:1px solid var(--line)}
figure.image figcaption{margin-top:6px;font-size:12.5px;line-height:18px;color:var(--ink-3)}
img.inline{vertical-align:middle;border:1px solid var(--line)}
.table-wrap{margin:16px 0;overflow-x:auto}
table{border-collapse:collapse;width:100%}
th{font:500 11.5px/16px Inter,system-ui,sans-serif;color:var(--ink-3);text-align:start;padding:8px 12px;border-bottom:1px solid var(--line-2)}
td{padding:8px 12px;border-bottom:1px solid var(--line);vertical-align:top}
tbody tr:hover{background:var(--bg-2)}
figure.code{direction:ltr;text-align:left;margin:16px 0;background:var(--bg-2);border:1px solid var(--line)}
figure.code>header{display:flex;align-items:center;justify-content:space-between;height:28px;padding:0 12px;border-bottom:1px solid var(--line);font:400 11.5px/16px "JetBrains Mono",ui-monospace,monospace;color:var(--ink-3)}
figure.code>header .detected{color:var(--ink-4);margin-left:6px}
figure.code>header button{font:500 11.5px/16px Inter,system-ui,sans-serif;color:var(--ink-3);background:none;border:0;padding:0;cursor:pointer;opacity:0}
figure.code:hover>header button,figure.code>header button:focus{opacity:1}
figure.code pre{margin:0;padding:12px 16px;overflow-x:auto;font:400 13px/21px "JetBrains Mono",ui-monospace,monospace;color:var(--ink-body);tab-size:4}
figure.code pre code{white-space:pre}
.t-kw{color:var(--ink);font-weight:600}
.t-fn{color:var(--c-fn)}
.t-ty,.t-tag{color:var(--c-ty);font-weight:var(--w-ty)}
.t-str{color:var(--c-str)}
.t-num,.t-const{color:var(--c-num)}
.t-var,.t-prop,.t-flag,.t-attr{color:var(--ink-body)}
.t-op,.t-punct{color:var(--ink-3)}
.t-cmt{color:var(--ink-3);font-style:italic}
.t-prompt{color:var(--ink-4)}
.t-ins{color:var(--ok)}
.t-del{color:var(--danger)}
.t-head{color:var(--ink);font-weight:600}
.callout,details.fold{margin:16px 0;background:var(--bg-2);border:1px solid var(--line);border-inline-start:2px solid var(--c,var(--ink-2));--c:var(--ink-2)}
.callout[data-family="danger"],.callout[data-family="failure"],.callout[data-family="bug"]{--c:var(--danger)}
.callout[data-family="warning"]{--c:var(--warn)}
.callout[data-family="success"],.callout[data-family="tip"]{--c:var(--ok)}
.callout .callout{margin:12px 0 0;margin-inline-start:16px}
.callout>.title,.callout>summary,details.fold>summary{display:flex;align-items:center;gap:8px;min-height:32px;padding:4px 16px;font:600 13px/20px Inter,system-ui,sans-serif;color:var(--ink);list-style:none}
.callout>summary,details.fold>summary{cursor:pointer}
.callout>summary:hover,details.fold>summary:hover{background:var(--card-2)}
summary::-webkit-details-marker{display:none}
.callout .icon{flex:none;width:14px;height:14px;color:var(--c);fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}
.callout .label{flex:1;min-width:0}
.chev{flex:none;width:14px;height:14px;color:var(--ink-3);fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round;transition:transform 120ms}
html[dir=rtl] .chev{transform:scaleX(-1)}
details[open]>summary>.chev{transform:rotate(90deg)}
.callout>.content{padding:4px 16px 12px}
.callout>.content>:last-child,details.fold>.content>:last-child{margin-bottom:0}
details.fold>summary{padding:4px 12px}
details.fold>.content{padding:4px 16px 12px}
details.fold>summary .chev{order:-1}
figure.mermaid{direction:ltr;margin:24px 0;background:var(--bg);border:1px solid var(--line)}
figure.mermaid>header{display:flex;align-items:center;justify-content:space-between;height:28px;padding:0 12px;border-bottom:1px solid var(--line);font:500 11.5px/16px Inter,system-ui,sans-serif;color:var(--ink-3)}
figure.mermaid>header code{font-size:11.5px;color:var(--ink-3)}
figure.mermaid .diagram{padding:16px;overflow-x:auto;text-align:center}
figure.mermaid svg{display:inline-block}
.mermaid-error{margin:0;padding:8px 12px;border-top:1px solid var(--line);color:var(--danger);font-size:12.5px;line-height:18px}
.embed{margin:16px 0;padding:0;padding-inline-start:16px;border-inline-start:2px solid var(--line-2)}
.embed>.embed-title{font:600 13px/20px Inter,system-ui,sans-serif;margin-bottom:8px}
.embed.missing{color:var(--ink-3)}
.raw-html{margin:16px 0;padding:12px 16px;background:var(--bg-2);border:1px solid var(--line);overflow-x:auto;font:400 13px/21px "JetBrains Mono",ui-monospace,monospace;color:var(--ink-3)}
details.props{margin:0 0 24px}
details.props>summary{font:500 11.5px/16px Inter,system-ui,sans-serif;color:var(--ink-3);cursor:pointer;list-style:none;display:flex;gap:6px;align-items:center;margin-bottom:8px}
.props .row{display:flex;flex-wrap:wrap;gap:8px}
.pill{display:inline-flex;gap:6px;align-items:baseline;font-size:12.5px;line-height:20px;padding:0 8px;background:var(--card-2);border:1px solid var(--line);border-radius:2px;color:var(--ink-2)}
.pill .k{color:var(--ink-3)}
.pill .v.mono{font-family:"JetBrains Mono",ui-monospace,monospace;font-size:11.5px}
.footnotes{margin-top:40px;padding-top:12px;border-top:1px solid var(--line);font-size:13px;line-height:20px;color:var(--ink-2)}
.footnotes ol{padding-inline-start:20px}
sup.fn a{text-decoration:none;color:var(--ink-3);font-size:11px}
@page{size:A4;margin:18mm 16mm}
@media print{
html,body{background:#fff !important;min-height:0}
body{font-size:11pt;line-height:1.55}
.sheet{max-width:none;padding:0;border:0;background:#fff;min-height:0}
figure.code button{display:none}
pre,figure.code,figure.mermaid,.callout,details.fold,table,tr,figure.image,.math-block,blockquote{break-inside:avoid}
h1,h2,h3,h4,h5,h6{break-after:avoid}
figure.code pre,.raw-html{white-space:pre-wrap;overflow-wrap:anywhere}
figure.code pre code{white-space:pre-wrap}
.table-wrap,figure.mermaid .diagram{overflow:visible}
figure.mermaid svg{width:auto!important;min-width:0!important;max-width:100%!important;height:auto!important}
a{text-decoration-color:var(--ink-3)}
}
"#;

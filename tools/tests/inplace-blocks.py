import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(sys.argv[1])
BIN = sys.argv[2]
WANT = set(sys.argv[3:])
WORK = ROOT / "target/test-inplace-blocks"

BLOCKS = """---
status: draft words
aliases:
  - first alias
---

# Blocks

> [!note]+ Folded title
> body line

> plain quote line

```python
def f():
    return 1
```

$$
x^2
$$

<div>html here</div>

Last paragraph.
"""
ARABIC = "# كتلة\n\n> [!tip] عنوان عربي\n> نص التنبيه\n"


def at(text, needle, n=0):
    start = text.index(needle)
    return len(text[: start + n].encode())


def insert(text, byte, what):
    raw = text.encode()
    return (raw[:byte] + what.encode() + raw[byte:]).decode()


def end_of(text, needle):
    return at(text, needle, len(needle))


OPEN = ["view live", "wait 600", "caret 1:0", "wait 300"]

CASES = {
    "callout-title": ("Blocks.md", OPEN + ["click Folded title|6", "type @"],
                      lambda t: insert(t, at(t, "Folded title", 6), "@"), True),
    "callout-enter": ("Blocks.md", OPEN + ["click body line|9", "key enter", "type @"],
                      lambda t: insert(t, end_of(t, "body line"), "\n> @"), True),
    "callout-leave": ("Blocks.md", OPEN + ["click body line|9", "key enter", "key enter", "type @"],
                      lambda t: insert(t, end_of(t, "body line"), "\n\n@"), True),
    "quote-enter": ("Blocks.md", OPEN + ["click plain quote line|16", "key enter", "type @"],
                    lambda t: insert(t, end_of(t, "plain quote line"), "\n> @"), True),
    "callout-type": ("Blocks.md", OPEN + ["click #callout 0", "wait 300", "type warn", "key enter"],
                     lambda t: t.replace("> [!note]+ Folded", "> [!warning]+ Folded"), False),
    "callout-undo": ("Blocks.md", OPEN + ["click body line|4", "type abc", "key ctrl-z", "type @"],
                     lambda t: insert(t, at(t, "body line", 4), "@"), True),
    "code-enter": ("Blocks.md", OPEN + ["click return 1|8", "key enter", "type @"],
                   lambda t: insert(t, end_of(t, "return 1"), "\n    @"), True),
    "code-leave": ("Blocks.md", OPEN + ["click return 1|8", "key enter", "key enter", "type @"],
                   lambda t: t.replace("    return 1\n```\n", "    return 1\n```\n@\n"), True),
    "code-tab": ("Blocks.md", OPEN + ["click def f():|8", "key tab", "type @"],
                 lambda t: insert(t, end_of(t, "def f():"), "    @"), True),
    "code-type": ("Blocks.md", OPEN + ["click return 1|7", "type 0"],
                  lambda t: insert(t, at(t, "return 1", 7), "0"), True),
    "prop-type": ("Blocks.md", OPEN + ["click draft words|5", "type @"],
                  lambda t: insert(t, at(t, "draft words", 5), "@"), True),
    "prop-quote": ("Blocks.md", OPEN + ["click draft words|0", "type ["],
                   lambda t: t.replace("status: draft words", 'status: "[draft words"'), True),
    "prop-list-enter": ("Blocks.md", OPEN + ["click first alias|11", "key enter", "type x"],
                        lambda t: insert(t, end_of(t, "first alias"), "\n  - x"), True),
    "arabic-callout": ("Arabic.md", OPEN + ["click نص التنبيه|2", "key end", "key enter", "type @"],
                       lambda t: insert(t, end_of(t, "نص التنبيه"), "\n> @"), True),
    "math-above": ("Blocks.md", OPEN + ["click x^2|0", "wait 300", "type @"],
                   lambda t: insert(t, at(t, "x^2", 0), "@"), False),
    "html-above": ("Blocks.md", OPEN + ["click html here|4", "wait 300", "type @"],
                   lambda t: insert(t, at(t, "html here", 4), "@"), False),
}


def vault():
    base = WORK / "base"
    shutil.rmtree(WORK, ignore_errors=True)
    (base / ".obsidian").mkdir(parents=True)
    (base / ".obsidian/app.json").write_text("{}")
    (base / "Blocks.md").write_text(BLOCKS)
    (base / "Arabic.md").write_text(ARABIC)
    return base


def run(name, note, steps):
    case = WORK / name
    shutil.rmtree(case, ignore_errors=True)
    shutil.copytree(WORK / "base", case / "vault")
    script = case / "script.focus"
    script.write_text("\n".join([f"open {note}", "wait 800", *steps, "wait 1500", "quit", ""]))
    env = dict(os.environ, FOCUS_BIN=BIN, FOCUS_SHOT_DIR=str(case))
    with open(case / "log.txt", "w") as log:
        code = subprocess.run([str(ROOT / "tools/shot.sh"), str(script), str(case / "vault")],
                              env=env, stdout=log, stderr=subprocess.STDOUT, timeout=600).returncode
    return case, code


def main():
    base = vault()
    failed = 0
    for name, (note, steps, expect, caret) in CASES.items():
        if WANT and name not in WANT:
            continue
        original = (base / note).read_text()
        dump = WORK / name / "end.json"
        case, code = run(name, note, steps + ["wait 300", f"dump {dump}"])
        got = (case / "vault" / note).read_bytes()
        wanted = expect(original).encode()
        try:
            in_place = json.loads(dump.read_text())["caret_shown"]
        except (OSError, ValueError):
            in_place = None
        if code == 0 and got == wanted and (in_place or not caret):
            print(f"ok   {name}")
            continue
        failed += 1
        print(f"FAIL {name}: status {code}, caret in place at the end: {in_place}, see {case}/log.txt")
        for i, (a, b) in enumerate(zip(got, wanted)):
            if a != b:
                print(f"     first difference at byte {i}: got {got[max(0, i - 20):i + 20]!r}, wanted {wanted[max(0, i - 20):i + 20]!r}")
                break
        else:
            print(f"     lengths {len(got)} and {len(wanted)}")
    sys.exit(1 if failed else 0)


main()

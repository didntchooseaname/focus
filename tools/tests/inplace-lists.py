import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(sys.argv[1])
BIN = sys.argv[2]
WANT = set(sys.argv[3:])
WORK = ROOT / "target/test-inplace-lists"

LISTS = """# Lists

- first item
- second item
  - nested item
- [ ] open task
- [x] done task

1. alpha
2. beta

> [!note] Callout
> - in callout
> - again

Last paragraph.
"""
ARABIC = "# قائمة\n\n- عنصر أول\n- عنصر ثان\n"
LONG = "# Long list\n\n" + "".join(f"- [ ] item {i} with words\n" for i in range(300))


def at(text, needle, n=0):
    start = text.index(needle)
    return len(text[: start + n].encode())


def insert(text, byte, what):
    raw = text.encode()
    return (raw[:byte] + what.encode() + raw[byte:]).decode()


def end_of(needle):
    return lambda t: at(t, needle, len(needle))


OPEN = ["view live", "wait 600", "caret 1:0", "wait 300"]

CASES = {
    "click-item": ("Lists.md", OPEN + ["click second item|3", "type @"],
                   lambda t: insert(t, at(t, "second item", 3), "@")),
    "enter-bullet": ("Lists.md", OPEN + ["click first item|10", "key enter", "type @"],
                     lambda t: insert(t, end_of("first item")(t), "\n- @")),
    "enter-task": ("Lists.md", OPEN + ["click open task|9", "key enter", "type @"],
                   lambda t: insert(t, end_of("open task")(t), "\n- [ ] @")),
    "enter-done-task": ("Lists.md", OPEN + ["click done task|9", "key enter", "type @"],
                        lambda t: insert(t, end_of("done task")(t), "\n- [ ] @")),
    "enter-number": ("Lists.md", OPEN + ["click alpha|5", "key enter", "type @"],
                     lambda t: insert(t, end_of("1. alpha")(t), "\n2. @")),
    "enter-split": ("Lists.md", OPEN + ["click first item|6", "key enter", "type @"],
                    lambda t: insert(t, at(t, "first item", 6), "\n- @")),
    "enter-empty": ("Lists.md", OPEN + ["click done task|9", "key enter", "key enter", "type @"],
                    lambda t: insert(t, end_of("done task")(t), "\n\n@")),
    "enter-empty-nested": ("Lists.md", OPEN + ["click nested item|11", "key enter", "key enter", "type @"],
                           lambda t: insert(t, end_of("nested item")(t), "\n- @")),
    "tab": ("Lists.md", OPEN + ["click second item|0", "key tab", "type @"],
            lambda t: t.replace("- second item\n  - nested item", "  - @second item\n    - nested item")),
    "shift-tab": ("Lists.md", OPEN + ["click nested item|0", "key shift-tab", "type @"],
                  lambda t: t.replace("  - nested item", "- @nested item")),
    "tab-first": ("Lists.md", OPEN + ["click first item|0", "key tab", "type @"],
                  lambda t: insert(t, at(t, "first item"), "@")),
    "backspace-lowers": ("Lists.md", OPEN + ["click nested item|0", "key backspace", "type @"],
                         lambda t: t.replace("  - nested item", "- @nested item")),
    "backspace-joins": ("Lists.md", OPEN + ["click second item|0", "key backspace", "type @"],
                        lambda t: t.replace("- first item\n- second item", "- first item@second item")),
    "backspace-task": ("Lists.md", OPEN + ["click done task|0", "key backspace", "type @"],
                       lambda t: t.replace("- [ ] open task\n- [x] done task", "- [ ] open task@done task")),
    "checkbox": ("Lists.md", OPEN + ["click #task 0"],
                 lambda t: t.replace("- [ ] open task", "- [x] open task")),
    "undo": ("Lists.md", OPEN + ["click first item|10", "key enter", "key ctrl-z", "type @"],
             lambda t: insert(t, end_of("first item")(t), "@")),
    "callout-enter": ("Lists.md", OPEN + ["click in callout|10", "key enter", "type @"],
                      lambda t: insert(t, end_of("in callout")(t), "\n> - @")),
    "callout-tab": ("Lists.md", OPEN + ["click again|0", "key tab", "type @"],
                    lambda t: t.replace("> - again", ">   - @again")),
    "arabic-click": ("Arabic.md", OPEN + ["click عنصر ثان|5", "type @"],
                     lambda t: insert(t, at(t, "عنصر ثان", 5), "@")),
    "arabic-enter": ("Arabic.md", OPEN + ["click عنصر أول|8", "key enter", "type @"],
                     lambda t: insert(t, end_of("عنصر أول")(t), "\n- @")),
    "long-enter": ("Long.md", ["view live", "wait 1500", "caret 153:25", "wait 500", "key enter", "type @"],
                   lambda t: insert(t, end_of("item 150 with words")(t), "\n- [ ] @")),
    "long-tab": ("Long.md", ["view live", "wait 1500", "caret 153:6", "wait 500", "key tab", "type @"],
                 lambda t: t.replace("- [ ] item 150 with", "  - [ ] @item 150 with")),
}

DUMP_CASES = {
    "markers-drawn": ("Lists.md", OPEN + ["click second item|2"], ["second item"], ["- second", "- [", "1. alpha"]),
    "task-drawn": ("Lists.md", OPEN + ["click open task|2"], ["open task"], ["[ ]", "[x]"]),
}


def vault():
    base = WORK / "base"
    shutil.rmtree(WORK, ignore_errors=True)
    (base / ".obsidian").mkdir(parents=True)
    (base / ".obsidian/app.json").write_text("{}")
    (base / "Lists.md").write_text(LISTS)
    (base / "Arabic.md").write_text(ARABIC)
    (base / "Long.md").write_text(LONG)
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
    for name, (note, steps, expect) in CASES.items():
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
        if code == 0 and got == wanted and (in_place or name == "checkbox"):
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
    for name, (note, steps, shown, hidden) in DUMP_CASES.items():
        if WANT and name not in WANT:
            continue
        dump = WORK / name / "dump.json"
        case, code = run(name, note, steps + [f"dump {dump}"])
        try:
            data = json.loads(dump.read_text())
        except (OSError, ValueError) as error:
            failed += 1
            print(f"FAIL {name}: no dump ({error}), status {code}, see {case}/log.txt")
            continue
        texts = ["".join(c for c in t["text"] if c not in "⁦⁧⁨⁩") for t in data["texts"]]
        whole = "\n".join(texts)
        missing = [s for s in shown if s not in whole]
        extra = [s for s in hidden if s in whole]
        if code == 0 and data["caret_shown"] and not missing and not extra:
            print(f"ok   {name}")
        else:
            failed += 1
            print(f"FAIL {name}: caret {data['caret_shown']}, missing {missing}, markers shown as text {extra}")
    sys.exit(1 if failed else 0)


main()

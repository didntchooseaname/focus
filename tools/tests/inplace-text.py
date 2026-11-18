import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(sys.argv[1])
BIN = sys.argv[2]
WANT = set(sys.argv[3:])
WORK = ROOT / "target/test-inplace-text"

TEXT = """# Title of the note

First paragraph with **bold words** and a [[Other|link]] here.
Second line of the paragraph.

## Heading two

Last paragraph.
"""
ARABIC = "# عنوان\n\nفقرة أولى بالعربية للتجربة.\n"
ESCAPES = "# Escapes\n\nPrice 5\\*3 is \\|odd\\| and **bold** end.\n"
LONG_HEAD = "# Long note\n\nTyped paragraph here.\n\n"
LEAVE = "# Leave\n\n- one\n\n| a | b |\n|---|---|\n| c | d |\n\n```\ncode\n```\n\n> [!note] T\n> body\n\nEnd.\n"


def at(text, needle, n=0):
    start = text.index(needle)
    return len(text[: start + n].encode())


def insert(text, byte, what):
    raw = text.encode()
    return (raw[:byte] + what.encode() + raw[byte:]).decode()


def delete(text, start, end):
    raw = text.encode()
    return (raw[:start] + raw[end:]).decode()


OPEN = ["view live", "wait 600", "caret 1:0", "wait 300"]

CASES = {
    "caret-type": ("Text.md", OPEN + ["caret 3:6", "type @"],
                   lambda t: insert(t, at(t, "First paragraph", 6), "@")),
    "caret-enter": ("Text.md", OPEN + ["caret 3:6", "key enter", "type @"],
                    lambda t: insert(t, at(t, "First paragraph", 6), "\n@")),
    "caret-undo": ("Text.md", OPEN + ["caret 3:6", "type abc", "key ctrl-z", "type @"],
                   lambda t: insert(t, at(t, "First paragraph", 6), "@")),
    "caret-arabic": ("Arabic.md", OPEN + ["caret 3:5", "type @"],
                     lambda t: insert(t, at(t, "أولى", 0), "@")),
    "click-paragraph": ("Text.md", OPEN + ["click First paragraph|6", "type @"],
                        lambda t: insert(t, at(t, "First paragraph", 6), "@")),
    "click-second-line": ("Text.md", OPEN + ["click Second line|7", "type @"],
                          lambda t: insert(t, at(t, "Second line", 7), "@")),
    "click-heading": ("Text.md", OPEN + ["click Heading two|3", "type @"],
                      lambda t: insert(t, at(t, "Heading two", 3), "@")),
    "click-bold": ("Text.md", OPEN + ["click bold words|2", "type X"],
                   lambda t: insert(t, at(t, "bold words", 2), "X")),
    "click-link-text": ("Text.md", OPEN + ["click link here|5"] + ["key left"] * 5 + ["type @"],
                        lambda t: insert(t, at(t, "link]]", 2), "@")),
    "click-title": ("Text.md", OPEN + ["click Title of the note|5", "type @"],
                    lambda t: insert(t, at(t, "Title of the note", 5), "@")),
    "right-into-bold": ("Text.md", OPEN + ["click with bold|4", "key right", "key right", "type @"],
                        lambda t: insert(t, at(t, "**bold", 1), "@")),
    "down-to-next-line": ("Text.md", OPEN + ["caret 3:0", "key down", "type @"],
                          lambda t: insert(t, at(t, "Second line"), "@")),
    "end-of-line": ("Text.md", OPEN + ["caret 4:0", "key end", "type @"],
                    lambda t: insert(t, at(t, "paragraph.\n\n##", len("paragraph.")), "@")),
    "enter-new-line": ("Text.md", OPEN + ["click Last paragraph|4", "key enter", "type @"],
                       lambda t: insert(t, at(t, "Last paragraph", 4), "\n@")),
    "backspace-join": ("Text.md", OPEN + ["click Last paragraph|0", "key backspace", "type @"],
                       lambda t: delete(t, at(t, "two\n\nLast", 3), at(t, "Last paragraph")).replace("twoLast", "two@Last")),
    "delete-char": ("Text.md", OPEN + ["click Last paragraph|0", "key delete"],
                    lambda t: delete(t, at(t, "Last paragraph"), at(t, "Last paragraph", 1))),
    "backspace-escape": ("Escapes.md", OPEN + ["click 5*3|2", "key backspace"],
                         lambda t: t.replace("5\\*3", "53")),
    "delete-escape": ("Escapes.md", OPEN + ["click 5*3|1", "key delete"],
                      lambda t: t.replace("5\\*3", "53")),
    "type-before-escape": ("Escapes.md", OPEN + ["click 5*3|1", "type X"],
                           lambda t: t.replace("5\\*3", "5X\\*3")),
    "backspace-escaped-pipe": ("Escapes.md", OPEN + ["click odd| and|4", "key backspace"],
                               lambda t: t.replace("odd\\|", "odd")),
    "backspace-after-bold": ("Escapes.md", OPEN + ["click bold end|4", "key backspace"],
                             lambda t: t.replace("**bold**", "**bol**")),
    "delete-at-bold-edge": ("Escapes.md", OPEN + ["click and bold|4", "key left", "key left", "key delete"],
                            lambda t: t.replace("**bold**", "**old**")),
    "backspace-in-markup": ("Escapes.md", OPEN + ["click and bold|4", "key left", "key backspace"],
                            lambda t: t.replace("**bold**", "*bold**")),
    "space-end-backspace": ("Text.md", OPEN + ["click Last paragraph|4", "key end", "key space", "key space", "key backspace", "type @"],
                            lambda t: t.replace("Last paragraph.", "Last paragraph. @")),
    "space-end-list-enter": ("Leave.md", OPEN + ["click one|3", "key space", "key enter", "type @"],
                             lambda t: t.replace("- one\n", "- one \n- @\n")),
    "select-and-type": ("Text.md", OPEN + ["click Last paragraph|0", "key shift-right", "key shift-right", "key shift-right", "key shift-right", "type Next"],
                        lambda t: t.replace("Last paragraph", "Next paragraph")),
    "undo-caret": ("Text.md", OPEN + ["click Last paragraph|4", "type abc", "click First paragraph|0", "key ctrl-z", "type @"],
                   lambda t: insert(t, at(t, "Last paragraph", 4), "@")),
    "escape": ("Text.md", OPEN + ["click Last paragraph|4", "key escape", "type zz"],
               lambda t: t),
    "arabic": ("Arabic.md", OPEN + ["click أولى|2", "type @"],
               lambda t: insert(t, at(t, "أولى", 2), "@")),
    "long-note": ("Long.md", ["view live", "wait 1500", "caret 3:0", "wait 300", "click Typed paragraph|6", "type @"],
                  lambda t: insert(t, at(t, "Typed paragraph", 6), "@")),
}

DUMP_CASES = {
    "markup-bold": ("Text.md", OPEN + ["click bold words|2"], ["**bold words**"], ["[[Other|link]]", "## Heading"]),
    "markup-link": ("Text.md", OPEN + ["click link here|5", "key left"], ["[[Other|link]]"], ["**bold"]),
    "markup-heading": ("Text.md", OPEN + ["click Heading two|2"], ["## Heading two"], ["**bold", "[[Other"]),
    "markup-none": ("Text.md", OPEN + ["click Second line|2"], [], ["**bold", "[[Other", "## Heading"]),
}

DRAWN_CASES = {
    "drawn-after-enter": ("Leave.md", OPEN + ["click End.|4", "key enter"]),
    "drawn-list-left": ("Leave.md", OPEN + ["click one|3", "key enter", "key enter"]),
    "drawn-table-left": ("Leave.md", OPEN + ["click d|1", "key enter", "key enter"]),
    "drawn-code-left": ("Leave.md", OPEN + ["click code|4", "key enter", "key enter"]),
    "drawn-callout-line": ("Leave.md", OPEN + ["click body|4", "key enter"]),
    "drawn-callout-left": ("Leave.md", OPEN + ["click body|4", "key enter", "key enter"]),
    "drawn-space-end": ("Text.md", OPEN + ["click Last paragraph|4", "key end", "key space"]),
    "drawn-space-heading": ("Text.md", OPEN + ["click Heading two|3", "key end", "key space"]),
    "drawn-space-cell": ("Leave.md", OPEN + ["click d|1", "key space"]),
}


def vault():
    base = WORK / "base"
    shutil.rmtree(WORK, ignore_errors=True)
    (base / ".obsidian").mkdir(parents=True)
    (base / ".obsidian/app.json").write_text("{}")
    (base / "Text.md").write_text(TEXT)
    (base / "Other.md").write_text("# Other\n\nText.\n")
    (base / "Arabic.md").write_text(ARABIC)
    (base / "Escapes.md").write_text(ESCAPES)
    (base / "Leave.md").write_text(LEAVE)
    sys.path.insert(0, str(ROOT / "tools/bench"))
    import interaction

    body = interaction.note_text(10000).split("\n", 6)[6]
    (base / "Long.md").write_text(LONG_HEAD + body)
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
                              env=env, stdout=log, stderr=subprocess.STDOUT, timeout=300).returncode
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
        if code == 0 and got == wanted and in_place == (name != "escape"):
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
    for name, (note, steps) in DRAWN_CASES.items():
        if WANT and name not in WANT:
            continue
        dump = WORK / name / "dump.json"
        case, code = run(name, note, steps + ["wait 200", f"dump {dump}"])
        try:
            data = json.loads(dump.read_text())
        except (OSError, ValueError) as error:
            failed += 1
            print(f"FAIL {name}: no dump ({error}), status {code}, see {case}/log.txt")
            continue
        holding = [t["key"] for t in data["texts"] if t["caret"] is not None]
        if code == 0 and data["caret_shown"] and holding:
            print(f"ok   {name}")
        else:
            failed += 1
            print(f"FAIL {name}: caret {data['caret_shown']}, no drawn text holds it, see {case}/log.txt")
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
            print(f"FAIL {name}: caret {data['caret_shown']}, markup missing {missing}, shown though away from the caret {extra}")
    sys.exit(1 if failed else 0)


main()

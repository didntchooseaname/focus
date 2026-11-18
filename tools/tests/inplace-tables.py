import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(sys.argv[1])
BIN = sys.argv[2]
WANT = set(sys.argv[3:])
WORK = ROOT / "target/test-inplace-tables"

TABLE = """# Table

| Nom | Valeur |
|---|---|
| alpha | 1 |
| beta | 2 |
| gamma\\|delta | 3 |

Fin.
"""
ARABIC = """# جدول

| الاسم | القيمة |
|---|---|
| ألف | واحد |
"""
QUOTE = "> [!note] Titre\r\n> | a | b |\r\n> |---|---|\r\n> | c | d |\r\n"
LONG = "# Long\n\n| Nom | Valeur | Note |\n|---|---|---|\n" + "".join(
    f"| ligne {i} | {i * 7} | texte de la cellule {i} |\n" for i in range(200)
) + "\nFin.\n"
PASTED = "un\ndeux|trois"


def at(text, needle, n=0):
    start = text.index(needle)
    return len(text[: start + n].encode())


def insert(text, byte, what):
    raw = text.encode()
    return (raw[:byte] + what.encode() + raw[byte:]).decode()


def padded(rows):
    widths = [max(3, *(len(row[c]) for row in rows)) for c in range(len(rows[0]))]
    line = lambda cells: "| " + " | ".join(c.ljust(w) for c, w in zip(cells, widths)) + " |"
    return "\n".join([line(rows[0]), line(["-" * w for w in widths])] + [line(r) for r in rows[1:]])


OPEN = ["view live", "wait 600", "caret 1:0", "wait 300"]
T = "Table.md"

CASES = {
    "click-cell": (T, OPEN + ["click alpha|2", "type @"], lambda t: insert(t, at(t, "alpha", 2), "@")),
    "type-pipe": (T, OPEN + ["click alpha|5", "type |"], lambda t: t.replace("| alpha |", "| alpha\\| |")),
    "paste": (T, OPEN + ["click alpha|5", "key ctrl-v", "wait 500"],
              lambda t: t.replace("| alpha |", "| alphaun deux\\|trois |")),
    "tab": (T, OPEN + ["click alpha|0", "key tab", "type @"], lambda t: t.replace("| alpha | 1 |", "| alpha | 1@ |")),
    "shift-tab": (T, OPEN + ["click beta|0", "key shift-tab", "type @"], lambda t: t.replace("| alpha | 1 |", "| alpha | 1@ |")),
    "tab-adds-row": (T, OPEN + ["click 3|1", "key tab", "type @"],
                     lambda t: t.replace("| gamma\\|delta | 3 |\n", "| gamma\\|delta | 3 |\n| @ |  |\n")),
    "enter": (T, OPEN + ["click alpha|0", "key enter", "type @"], lambda t: t.replace("| beta |", "| beta@ |")),
    "enter-adds-row": (T, OPEN + ["click 3|1", "key enter", "type @"],
                       lambda t: t.replace("| gamma\\|delta | 3 |\n", "| gamma\\|delta | 3 |\n|  | @ |\n")),
    "enter-leaves": (T, OPEN + ["click 3|1", "key enter", "key enter", "type @"],
                     lambda t: t.replace("| gamma\\|delta | 3 |\n", "| gamma\\|delta | 3 |\n\n@\n")),
    "backspace-pipe": (T, OPEN + ["click delta|0", "key backspace"], lambda t: t.replace("gamma\\|delta", "gammadelta")),
    "menu-row-above": (T, OPEN + ["caret 6:2", "table row-above", "type @"],
                       lambda t: t.replace("| beta | 2 |", "| @ |  |\n| beta | 2 |")),
    "menu-row-below": (T, OPEN + ["caret 3:2", "table row-below", "type @"],
                       lambda t: t.replace("|---|---|\n", "|---|---|\n| @ |  |\n")),
    "menu-column-right": (T, OPEN + ["caret 5:2", "table column-right", "type @"],
                          lambda t: t.replace("| Nom | Valeur |", "| Nom |  | Valeur |")
                          .replace("|---|---|", "|--- | ---|---|")
                          .replace("| alpha | 1 |", "| alpha | @ | 1 |")
                          .replace("| beta | 2 |", "| beta |  | 2 |")
                          .replace("| gamma\\|delta | 3 |", "| gamma\\|delta |  | 3 |")),
    "menu-column-left": (T, OPEN + ["caret 5:2", "table column-left", "type @"],
                         lambda t: t.replace("| Nom | Valeur |", "|  | Nom | Valeur |")
                         .replace("|---|---|", "|--- | ---|---|")
                         .replace("| alpha | 1 |", "| @ | alpha | 1 |")
                         .replace("| beta | 2 |", "|  | beta | 2 |")
                         .replace("| gamma\\|delta | 3 |", "|  | gamma\\|delta | 3 |")),
    "menu-delete-row": (T, OPEN + ["caret 5:2", "table delete-row"], lambda t: t.replace("| alpha | 1 |\n", "")),
    "menu-delete-column": (T, OPEN + ["caret 5:2", "table delete-column"],
                           lambda t: t.replace("| Nom | Valeur |", "| Valeur |").replace("|---|---|", "|---|")
                           .replace("| alpha | 1 |", "| 1 |").replace("| beta | 2 |", "| 2 |")
                           .replace("| gamma\\|delta | 3 |", "| 3 |")),
    "menu-align-center": (T, OPEN + ["caret 5:10", "table align-center"], lambda t: t.replace("|---|---|", "|---|:-:|")),
    "menu-align-table": (T, OPEN + ["caret 5:2", "table align-table"],
                         lambda t: t.replace("| Nom | Valeur |\n|---|---|\n| alpha | 1 |\n| beta | 2 |\n| gamma\\|delta | 3 |",
                                             padded([["Nom", "Valeur"], ["alpha", "1"], ["beta", "2"], ["gamma\\|delta", "3"]]))),
    "menu-undo": (T, OPEN + ["caret 5:2", "table delete-column", "wait 300", "key ctrl-z"], lambda t: t),
    "menu-disabled": (T, OPEN + ["caret 3:2", "table delete-row"], lambda t: t),
    "menu-shot": (T, OPEN + ["caret 5:2", "wait 300", "table menu", "wait 300", "shot table-menu"], lambda t: t),
    "menu-shot-ar": ("Arabic.md", ["lang ar"] + OPEN + ["caret 5:3", "wait 300", "table menu", "wait 300", "shot table-menu-ar"],
                     lambda t: t),
    "typing-undo": (T, OPEN + ["click alpha|2", "type xyz", "key ctrl-z"], lambda t: t),
    "quote-enter": ("Quote.md", OPEN + ["click d|1", "key enter", "type @"],
                    lambda t: t.replace("> | c | d |\r\n", "> | c | d |\r\n> |  | @ |\r\n")),
    "arabic": ("Arabic.md", ["lang ar"] + OPEN + ["click ألف|1", "type @", "shot arabic"],
               lambda t: insert(t, at(t, "ألف", 1), "@")),
    "arabic-column": ("Arabic.md", OPEN + ["caret 5:2", "table column-left", "type @"],
                      lambda t: t.replace("| الاسم | القيمة |", "|  | الاسم | القيمة |")
                      .replace("|---|---|", "|--- | ---|---|").replace("| ألف |", "| @ | ألف |")),
    "long-table": ("Long.md", ["view live", "wait 1500", "caret 105:35", "wait 500", "type @"],
                   lambda t: insert(t, at(t, "cellule 100", 3), "@")),
}

STAND_IN = """#!/bin/sh
case " $* " in
  *" --list-types "*|*" -l "*) printf 'text/plain;charset=utf-8\\nUTF8_STRING\\n' ;;
  *) cat "$FOCUS_FAKE_PASTE" ;;
esac
"""


def vault():
    base = WORK / "base"
    shutil.rmtree(WORK, ignore_errors=True)
    (base / ".obsidian").mkdir(parents=True)
    (base / ".obsidian/app.json").write_text("{}")
    (base / "Table.md").write_text(TABLE)
    (base / "Arabic.md").write_text(ARABIC)
    (base / "Quote.md").write_bytes(QUOTE.encode())
    (base / "Long.md").write_text(LONG)
    tools = WORK / "bin"
    tools.mkdir()
    for name in ("wl-paste", "wl-copy"):
        (tools / name).write_text(STAND_IN if name == "wl-paste" else "#!/bin/sh\ncat > /dev/null\n")
        (tools / name).chmod(0o755)
    (WORK / "pasted").write_text(PASTED)
    return base


def run(name, note, steps):
    case = WORK / name
    shutil.rmtree(case, ignore_errors=True)
    shutil.copytree(WORK / "base", case / "vault")
    script = case / "script.focus"
    script.write_text("\n".join(["set editor.edit_in_place on", f"open {note}", "wait 800", *steps,
                                 "wait 1500", "quit", ""]))
    env = dict(os.environ, FOCUS_BIN=BIN, FOCUS_SHOT_DIR=str(case), FOCUS_FAKE_PASTE=str(WORK / "pasted"),
               PATH=f"{WORK / 'bin'}:{os.environ['PATH']}")
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
        original = (base / note).read_bytes().decode()
        dump = WORK / name / "end.json"
        case, code = run(name, note, steps + ["wait 300", f"dump {dump}"])
        got = (case / "vault" / note).read_bytes()
        wanted = expect(original).encode()
        errors = [l for l in (case / "log.txt").read_text().splitlines() if l.startswith(("focus:", "Error:"))
                  and not (name == "menu-disabled" and "delete-row: disabled in this cell" in l)]
        if name == "menu-disabled" and not any("disabled in this cell" in l for l in (case / "log.txt").read_text().splitlines()):
            errors.append("the item ran in the head row")
        try:
            in_place = json.loads(dump.read_text())["caret_shown"]
        except (OSError, ValueError):
            in_place = None
        if code == 0 and got == wanted and (in_place or name.startswith("menu-shot")) and not errors:
            print(f"ok   {name}")
            continue
        failed += 1
        print(f"FAIL {name}: status {code}, caret in place at the end: {in_place}, see {case}/log.txt")
        for line in errors[:2]:
            print(f"     {line}")
        for i, (a, b) in enumerate(zip(got, wanted)):
            if a != b:
                print(f"     first difference at byte {i}: got {got[max(0, i - 20):i + 20]!r}, wanted {wanted[max(0, i - 20):i + 20]!r}")
                break
        else:
            if len(got) != len(wanted):
                print(f"     lengths {len(got)} and {len(wanted)}")
    sys.exit(1 if failed else 0)


main()

import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(sys.argv[1])
BIN = sys.argv[2]
WANT = set(sys.argv[3:])
WORK = ROOT / "target/test-selection-bar"

TEXT = """# Bar

First paragraph with plain words here.

Second paragraph.

- one
- two
  - two a
- three

Last one.
"""
REFUSE = "# Refuse\n\nIntro.\n\n## Head\nBody.\n"
PROPS = "---\nsummary: One value\n---\n# Props\n\nBody.\n"

LIVE = ["view live", "wait 600"]
EDIT = ["view source", "wait 400", "focus editor"]
POINTER = ["pointer down plain words|0", "pointer move plain words|5", "pointer up plain words|5", "wait 300"]
KEYS = ["caret 3:21", "wait 200"] + ["key shift-right"] * 5 + ["wait 300"]
EDIT_KEYS = ["key ctrl-home", "key down", "key down", "key home"] + ["key right"] * 21 + ["key shift-right"] * 5 + ["wait 300"]
HIDDEN = "the selection bar does not show"


def swap(text, a, b):
    return text.replace(a, "\0").replace(b, a).replace("\0", b)


CASES = {
    "live-bold": ("Bar.md", LIVE + POINTER + ["bar bold"], lambda t: t.replace("plain", "**plain**"), None),
    "live-bold-removed": ("Bar.md", LIVE + POINTER + ["bar bold", "wait 300", "bar bold"], lambda t: t, None),
    "live-italic-keys": ("Bar.md", LIVE + KEYS + ["bar italic"], lambda t: t.replace("plain", "*plain*"), None),
    "live-strike": ("Bar.md", LIVE + POINTER + ["bar strike"], lambda t: t.replace("plain", "~~plain~~"), None),
    "live-highlight": ("Bar.md", LIVE + POINTER + ["bar highlight"], lambda t: t.replace("plain", "==plain=="), None),
    "live-code": ("Bar.md", LIVE + POINTER + ["bar code"], lambda t: t.replace("plain", "`plain`"), None),
    "live-link": ("Bar.md", LIVE + POINTER + ["bar link"], lambda t: t.replace("plain", "[plain]()"), None),
    "live-type-two-blocks": ("Bar.md", LIVE + ["pointer down First paragraph|6", "pointer move Second paragraph|3",
                                               "pointer up Second paragraph|3", "wait 300", "bar type:h2"],
                             lambda t: t.replace("First paragraph", "## First paragraph").replace("Second paragraph", "## Second paragraph"), None),
    "live-escape": ("Bar.md", LIVE + POINTER + ["key escape", "wait 200", "bar bold"], lambda t: t, HIDDEN),
    "live-typing-hides": ("Bar.md", LIVE + POINTER + ["type X", "wait 200", "bar bold"], lambda t: t.replace("plain", "X"), HIDDEN),
    "live-setting-off": ("Bar.md", ["set editor.selection_bar off"] + LIVE + POINTER + ["bar bold"], lambda t: t, HIDDEN),
    "live-extract": ("Bar.md", LIVE + ["pointer down plain words|0", "pointer move plain words|11",
                                      "pointer up plain words|11", "wait 300", "bar extract", "wait 500",
                                      "type Moved", "wait 300", "key ctrl-enter"],
                     lambda t: t.replace("plain words", "[[Moved]]"), None),
    "live-ai": ("Bar.md", ["ai fake"] + LIVE + POINTER + ["bar ai", "wait 500"], lambda t: t, None),
    "live-ai-off": ("Bar.md", ["set ai.enabled off"] + LIVE + POINTER + ["bar ai"], lambda t: t,
                    "the AI functions are off"),
    "live-ctrl-b": ("Bar.md", LIVE + KEYS + ["key ctrl-b"], lambda t: t.replace("plain", "**plain**"), None),
    "property-no-bar": ("Props.md", LIVE + ["caret 2:9"] + ["key shift-right"] * 3 + ["wait 300", "bar bold"],
                        lambda t: t, HIDDEN),
    "property-ctrl-b": ("Props.md", LIVE + ["caret 2:9"] + ["key shift-right"] * 3 + ["key ctrl-b"], lambda t: t, None),
    "in-place-off-bar": ("Bar.md", ["set editor.edit_in_place off"] + LIVE + POINTER + ["bar bold"], lambda t: t, HIDDEN),
    "in-place-off-handles": ("Bar.md", ["set editor.edit_in_place off"] + LIVE + ["handle Second paragraph"], lambda t: t,
                             "no block handle shows there"),
    "read-no-bar": ("Bar.md", ["view reading", "wait 600"] + POINTER + ["bar bold"], lambda t: t, HIDDEN),
    "edit-bold-keys": ("Bar.md", EDIT + EDIT_KEYS + ["bar bold"], lambda t: t.replace("plain", "**plain**"), None),
    "edit-bold-removed": ("Bar.md", EDIT + EDIT_KEYS + ["bar bold", "wait 300", "bar bold"], lambda t: t, None),
    "edit-type-two-blocks": ("Bar.md", EDIT + EDIT_KEYS + ["key shift-down", "key shift-down", "wait 300", "bar type:bullets"],
                             lambda t: t.replace("First paragraph", "- First paragraph").replace("Second paragraph", "- Second paragraph"), None),
    "edit-setting-off": ("Bar.md", ["set editor.selection_bar off"] + EDIT + EDIT_KEYS + ["bar bold"], lambda t: t, HIDDEN),
    "handle-up": ("Bar.md", LIVE + ["handle Second paragraph", "handle move above First paragraph", "handle drop"],
                  lambda t: swap(t, "First paragraph with plain words here.", "Second paragraph."), None),
    "handle-first": ("Bar.md", LIVE + ["handle Last one", "handle move above First paragraph", "handle drop"],
                     lambda t: t.replace("\n\nLast one.", "").replace("# Bar\n\n", "# Bar\n\nLast one.\n\n"), None),
    "handle-last": ("Bar.md", LIVE + ["handle First paragraph", "handle move below Last one", "handle drop"],
                    lambda t: t.replace("First paragraph with plain words here.\n\n", "") + "\nFirst paragraph with plain words here.\n", None),
    "handle-escape": ("Bar.md", LIVE + ["handle Second paragraph", "handle move below Last one", "key escape", "handle drop"],
                      lambda t: t, "no block handle is held"),
    "handle-item": ("Bar.md", LIVE + ["handle two", "handle move below three", "handle drop"],
                    lambda t: t.replace("- two\n  - two a\n- three\n", "- three\n- two\n  - two a\n"), None),
    "handle-refused": ("Refuse.md", LIVE + ["handle Intro", "handle move above Body", "handle drop"], lambda t: t, None),
    "handle-undo": ("Bar.md", LIVE + ["handle Second paragraph", "handle move above First paragraph", "handle drop",
                                     "wait 300", "key ctrl-z"], lambda t: t, None),
    "handles-off": ("Bar.md", ["set editor.block_handles off"] + LIVE + ["handle Second paragraph"], lambda t: t,
                    "no block handle shows there"),
    "keys-live-up": ("Bar.md", LIVE + ["caret 5:0", "key alt-up"],
                     lambda t: swap(t, "First paragraph with plain words here.", "Second paragraph."), None),
    "keys-handles-off": ("Bar.md", ["set editor.block_handles off"] + LIVE + ["caret 5:0", "key alt-up"],
                         lambda t: swap(t, "First paragraph with plain words here.", "Second paragraph."), None),
    "keys-edit-item": ("Bar.md", EDIT + ["key ctrl-home"] + ["key down"] * 7 + ["key alt-down"],
                       lambda t: t.replace("- two\n  - two a\n- three\n", "- three\n- two\n  - two a\n"), None),
}


EXTRA = {
    "live-extract": lambda case: "plain words" in (case / "vault/Moved.md").read_text(),
}


def vault():
    base = WORK / "base"
    shutil.rmtree(WORK, ignore_errors=True)
    (base / ".obsidian").mkdir(parents=True)
    (base / ".obsidian/app.json").write_text("{}")
    (base / "Bar.md").write_text(TEXT)
    (base / "Refuse.md").write_text(REFUSE)
    (base / "Props.md").write_text(PROPS)
    return base


def run(name, note, steps):
    case = WORK / name
    shutil.rmtree(case, ignore_errors=True)
    shutil.copytree(WORK / "base", case / "vault")
    config = case / "config"
    config.mkdir()
    (config / "settings.json").write_text('{"language": "en"}')
    script = case / "script.focus"
    script.write_text("\n".join([f"open {note}", "wait 800", *steps, "wait 1500", "quit", ""]))
    env = dict(os.environ, FOCUS_BIN=BIN, FOCUS_SHOT_DIR=str(case), FOCUS_CONFIG_DIR=str(config),
               FOCUS_CACHE_DIR=str(case / "cache"), FOCUS_DATA_DIR=str(case / "data"))
    with open(case / "log.txt", "w") as log:
        code = subprocess.run([str(ROOT / "tools/shot.sh"), str(script), str(case / "vault")],
                              env=env, stdout=log, stderr=subprocess.STDOUT, timeout=300).returncode
    return case, code


def main():
    base = vault()
    failed = 0
    for name, (note, steps, expect, error) in CASES.items():
        if WANT and name not in WANT:
            continue
        original = (base / note).read_text()
        case, code = run(name, note, steps)
        got = (case / "vault" / note).read_bytes()
        wanted = expect(original).encode()
        log = (case / "log.txt").read_text(errors="replace")
        said = ("focus: script, line" not in log) if error is None else (error in log)
        try:
            extra = EXTRA.get(name, lambda _: True)(case)
        except OSError:
            extra = False
        if code == 0 and got == wanted and said and extra:
            print(f"ok   {name}")
            continue
        failed += 1
        reason = "" if said else f", the log does not say {error!r}" if error else ", an instruction failed"
        reason += "" if extra else ", a file it should leave is missing"
        print(f"FAIL {name}: status {code}{reason}, see {case}/log.txt")
        if got != wanted:
            print(f"     got    {got!r}")
            print(f"     wanted {wanted!r}")
    sys.exit(1 if failed else 0)


main()

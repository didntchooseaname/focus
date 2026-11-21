import os
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(sys.argv[1])
BIN = sys.argv[2]
WANT = set(sys.argv[3:])
WORK = ROOT / "target/test-find-paint"

NOTE = "# Find\n\nalphabet one, alphabet two, alphabet three.\n"
FIND = ["key ctrl-f", "type alphabet", "wait 600", "key enter", "wait 500", "shot paint"]
EDIT = ["view source", "wait 400"]
LIVE = ["view live", "wait 500", "click alphabet one|2", "wait 400"]

THEMES = {
    "dark": {"btn": 0xF5F5F6, "btn_ink": 0x0A0A0A, "card_3": 0x202022, "ink_3": 0x7D7D85},
    "light": {"btn": 0x0B0B0C, "btn_ink": 0xFFFFFF, "card_3": 0xEBEBED, "ink_3": 0x6B6B73},
}

CASES = {f"{view}-{theme}": (steps, theme) for view, steps in (("edit", EDIT), ("live", LIVE))
         for theme in THEMES}


def rgb(value):
    return (value >> 16 & 255, value >> 8 & 255, value & 255)


def distance(a, b):
    return max(abs(x - y) for x, y in zip(a, b))


def boxes(image, color, top):
    width, height = image.size
    pixels = image.load()
    runs = {}
    for y in range(top, height - 30):
        x = 0
        while x < width:
            if distance(pixels[x, y][:3], color) <= 2:
                start = x
                while x < width and distance(pixels[x, y][:3], color) <= 2:
                    x += 1
                if 40 <= x - start <= 160 and start >= 300:
                    runs.setdefault((start, x), []).append(y)
            else:
                x += 1
    found = []
    for (x0, x1), rows in runs.items():
        rows.sort()
        band = [rows[0]]
        for y in rows[1:]:
            if y == band[-1] + 1:
                band.append(y)
                continue
            if len(band) >= 3:
                found.append((x0, x1, band[0], band[-1]))
            band = [y]
        if len(band) >= 3:
            found.append((x0, x1, band[0], band[-1]))
    merged = []
    for box in sorted(found, key=lambda b: (b[0], b[2])):
        if merged and box[0] == merged[-1][0] and box[1] == merged[-1][1] and box[2] - merged[-1][3] <= 22:
            merged[-1] = (box[0], box[1], merged[-1][2], box[3])
        else:
            merged.append(box)
    return [box for box in merged if box[3] - box[2] >= 15]


def check(path, theme):
    colors = {k: rgb(v) for k, v in THEMES[theme].items()}
    image = Image.open(path).convert("RGB")
    pixels = image.load()
    problems = []
    current = boxes(image, colors["btn"], 125)
    others = boxes(image, colors["card_3"], 125)
    if len(current) != 1:
        problems.append(f"{len(current)} boxes in btn, one wanted: {current}")
    for x0, x1, y0, y1 in current:
        ink = sum(1 for x in range(x0, x1) for y in range(y0, y1 + 1)
                  if distance(pixels[x, y], colors["btn_ink"]) <= 24)
        if ink < 30:
            problems.append(f"the current match at {x0},{y0} has {ink} pixels of btn_ink text")
    if len(others) < 2:
        problems.append(f"{len(others)} boxes in card_3, two wanted: {others}")
    for x0, x1, y0, y1 in others:
        def underlined(y):
            near = sum(1 for x in range(x0 + 2, x1 - 2)
                       if distance(pixels[x, y], colors["ink_3"]) < distance(pixels[x, y], colors["card_3"]))
            return near >= 0.8 * (x1 - x0 - 4)
        if not any(underlined(y) for y in range(y0, y1 + 1)):
            problems.append(f"the match at {x0},{y0} has no ink_3 underline")
    return problems


def run(name, steps, theme):
    case = WORK / name
    shutil.rmtree(case, ignore_errors=True)
    shutil.copytree(ROOT / "fixtures/demo-vault", case / "vault")
    (case / "vault/Find.md").write_text(NOTE)
    config = case / "config"
    config.mkdir()
    (config / "settings.json").write_text('{"language": "en"}')
    script = case / "script.focus"
    lines = ["wait 800", f"theme {theme}", "open Find.md", *steps, *FIND, "key escape", "quit", ""]
    script.write_text("\n".join(lines))
    env = dict(os.environ, FOCUS_BIN=BIN, FOCUS_SHOT_DIR=str(case), FOCUS_CONFIG_DIR=str(config),
               FOCUS_CACHE_DIR=str(case / "cache"), FOCUS_DATA_DIR=str(case / "data"))
    with open(case / "log.txt", "w") as log:
        code = subprocess.run([str(ROOT / "tools/shot.sh"), str(script), str(case / "vault")],
                              env=env, stdout=log, stderr=subprocess.STDOUT, timeout=300).returncode
    return case, code


def main():
    failed = 0
    for name, (steps, theme) in CASES.items():
        if WANT and name not in WANT:
            continue
        case, code = run(name, steps, theme)
        shot = case / "paint.png"
        problems = [f"status {code}"] if code else []
        problems += check(shot, theme) if shot.exists() else ["no capture"]
        if problems:
            failed += 1
            print(f"FAIL {name}: {'; '.join(problems)}, see {case}")
        else:
            print(f"ok   {name}")
    sys.exit(1 if failed else 0)


main()

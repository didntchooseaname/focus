import sys
from pathlib import Path

from PIL import Image

BG = (7, 7, 7)
HATCH = (0x1C, 0x1C, 0x1E)
LINE = (0x23, 0x23, 0x25)


def rules(im):
    top, bottom = 90, im.height - 40
    found = []
    for x in range(im.width):
        hits = sum(1 for y in range(top, bottom, 4) if im.getpixel((x, y)) == LINE)
        if hits * 4 >= (bottom - top) * 0.9:
            found.append(x)
    return [0] + found + [im.width]


def ink(im, x0, x1, y0, y1):
    return sum(
        1
        for x in range(x0, x1, 2)
        for y in range(y0, y1, 3)
        if im.getpixel((x, y)) not in (BG, HATCH, LINE)
    )


def column(im, split):
    edges = rules(im)
    pairs = [(a + 1, b) for a, b in zip(edges, edges[1:]) if b - a > 120]
    notes = [p for p in pairs if heading_rule(im, p[0], p[1]) is not None]
    if not notes:
        return None
    if split:
        return max(notes, key=lambda p: p[0])
    return max(notes, key=lambda p: ink(im, p[0], p[1] - 8, 100, im.height - 40))


def first_x(im, x0, x1):
    best = None
    for y in range(100, im.height - 40):
        for x in range(x0, x1 - 10):
            if im.getpixel((x, y)) not in (BG, HATCH, LINE):
                if best is None or x < best:
                    best = x
                break
    return best


def heading_rule(im, x0, x1):
    width = x1 - x0
    for y in range(100, im.height - 40):
        run = sum(1 for x in range(x0, x1) if im.getpixel((x, y)) == LINE)
        if run >= width * 0.5:
            return y
    return None


failed = 0
base = Path(sys.argv[1])
for root, size in [(base / scale, size) for scale in ("100", "120") for size in ("1", "sm")]:
    for layout in ("both", "noleft", "none", "noright", "split"):
        shots = {v: Image.open(root / size / f"{layout}-{v}.png").convert("RGB") for v in ("reading", "live", "source")}
        found = column(shots["reading"], layout == "split")
        if found is None:
            print(f"FAIL {root.name}% {size} {layout}: no text column with a heading rule in Read")
            failed += 1
            continue
        x0, x1 = found
        xs = {v: first_x(im, x0, x1) for v, im in shots.items()}
        ys = {v: heading_rule(shots[v], x0, x1) for v in ("reading", "live")}
        same_x = len(set(xs.values())) == 1
        same_y = ys["reading"] is not None and ys["reading"] == ys["live"]
        label = f"{root.name}% {size} {layout}: first glyph x {xs}, heading rule y {ys}"
        print(("ok   " if same_x and same_y else "FAIL ") + label)
        failed += not (same_x and same_y)
sys.exit(1 if failed else 0)

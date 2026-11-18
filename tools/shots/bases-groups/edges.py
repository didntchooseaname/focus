import sys

from PIL import Image

LINE, LINE_2 = (0x23, 0x23, 0x25), (0x30, 0x30, 0x33)
image = Image.open(sys.argv[1]).convert("RGB")
width, height = image.size
pixels = image.load()
left, right, top, bottom = 300, 1300, 80, height - 30


def runs(y):
    found, start, kind = [], None, None
    for x in range(left, right + 1):
        colour = pixels[x, y] if x < right else None
        on = colour in (LINE, LINE_2)
        if start is not None and (not on or colour != kind):
            if x - start >= 60:
                found.append((y, start, x - 1, kind))
            start = None
        if on and start is None:
            start, kind = x, colour
    return found


rules = [rule for y in range(top, bottom) for rule in runs(y)]
if not rules:
    sys.exit("no rule found")
_, c0, c1, _ = max(rules, key=lambda rule: rule[2] - rule[1])
tables = []
for y, a, b, kind in rules:
    if kind == LINE_2:
        tables.append({"last": y, "edges": (a, b), "rows": 0})
    elif tables and y - tables[-1]["last"] < 100 and tables[-1]["edges"] == (a, b):
        tables[-1]["last"], tables[-1]["rows"] = y, tables[-1]["rows"] + 1
for table in (table for table in tables if table["rows"]):
    t0, t1 = table["edges"]
    print(f"  column {c0}..{c1}  table {t0}..{t1}  left gap {t0 - c0}  right gap {c1 - t1}")

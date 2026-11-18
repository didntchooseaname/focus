import sys

from PIL import Image

TEXT = 0xA0
INK = 0xD0
LEFT, RIGHT, TOP, BOTTOM = 280, 1200, 80, 970
RULE_MIN = 16


def load(path):
    img = Image.open(path).convert("RGB")
    return img, img.load()


def bright(px, x, y):
    return max(px[x, y]) >= TEXT


def rule(img, px):
    def on(x, y):
        return min(px[x, y]) >= 0xE8 and not bright(px, x - 1, y) and not bright(px, x + 1, y)
    for x in range(LEFT, RIGHT):
        y = TOP
        while y < BOTTOM:
            if on(x, y):
                start = y
                while y < BOTTOM and on(x, y):
                    y += 1
                if y - start >= RULE_MIN:
                    return (x, start, y)
            y += 1
    return None


def text_pixels(px, x0, x1, y0, y1):
    count, peak = 0, 0
    for y in range(max(y0, TOP), min(y1, BOTTOM)):
        for x in range(x0, x1):
            m = max(px[x, y])
            if m >= TEXT:
                count += 1
                peak = max(peak, m)
    return count, peak


def regions(px, found):
    x, top, bottom = found
    x0, x1 = x + 8, min(x + 680, RIGHT)
    return {
        "block": text_pixels(px, x0, x1, top, bottom),
        "above": text_pixels(px, x0, x1, TOP, top - 2),
        "below": text_pixels(px, x0, x1, bottom + 2, BOTTOM),
    }


def check_live(ref_path, snaps):
    img, px = load(ref_path)
    found = rule(img, px)
    if found is None:
        print(f"FAIL {ref_path}: no open block in the reference")
        return False
    ref = regions(px, found)
    ok = True
    for path in snaps:
        img, px = load(path)
        got = rule(img, px)
        if got is None:
            print(f"FAIL {path}: the open block's rule is gone")
            ok = False
            continue
        r = regions(px, got)
        why = []
        if r["block"][0] < ref["block"][0] * 0.5:
            why.append(f"block text {r['block'][0]} px against {ref['block'][0]}")
        elif r["block"][1] < INK:
            why.append(f"block text at 0x{r['block'][1]:02x}, dimmed")
        if r["above"][0] < ref["above"][0] * 0.9:
            why.append(f"text above {r['above'][0]} px against {ref['above'][0]}")
        if r["below"][0] < ref["below"][0] * 0.75:
            why.append(f"text below {r['below'][0]} px against {ref['below'][0]}")
        name = path.rsplit("/", 1)[-1]
        if why:
            ok = False
            print(f"FAIL {name}: " + "; ".join(why))
        else:
            print(f"ok   {name}: block {r['block'][0]} px (ref {ref['block'][0]}), above {r['above'][0]}, below {r['below'][0]}")
    return ok


def check_edit(ref_path, snaps):
    _, px = load(ref_path)
    ref = text_pixels(px, LEFT, RIGHT, TOP, BOTTOM)
    ok = True
    for path in snaps:
        _, px = load(path)
        got = text_pixels(px, LEFT, RIGHT, TOP, BOTTOM)
        name = path.rsplit("/", 1)[-1]
        if got[0] < ref[0] * 0.8 or got[1] < INK:
            ok = False
            print(f"FAIL {name}: source text {got[0]} px at 0x{got[1]:02x} against {ref[0]}")
        else:
            print(f"ok   {name}: source text {got[0]} px (ref {ref[0]})")
    return ok


if __name__ == "__main__":
    mode, ref, *snaps = sys.argv[1:]
    passed = (check_live if mode == "live" else check_edit)(ref, snaps)
    sys.exit(0 if passed else 1)

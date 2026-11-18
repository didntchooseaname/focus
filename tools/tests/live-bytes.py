#!/usr/bin/env python3
import os, subprocess, sys

root = os.path.dirname(os.path.abspath(__file__))
BYTEDIFF = os.path.join(root, '..', 'shots', 'qa3-bytediff.py')


def files(top):
    out = {}
    for d, _, names in os.walk(top):
        for n in names:
            p = os.path.join(d, n)
            out[os.path.relpath(p, top)] = p
    return out


def unescape(text):
    return text.replace('\\r', '\r').replace('\\n', '\n').encode('utf-8')


def line_end(data, n):
    start = 0
    for _ in range(n - 1):
        start = data.index(b'\n', start) + 1
    end = data.find(b'\n', start)
    end = len(data) if end < 0 else end
    return end - 1 if end > start and data[end - 1:end] == b'\r' else end, start


def expected(base, spec):
    kind, _, rest = spec.partition(':')
    if kind == 'unchanged':
        return base
    if kind in ('append', 'insert'):
        n, _, text = rest.partition(':')
        at, _ = line_end(base, int(n))
        return base[:at] + unescape(text) + base[at:]
    if kind == 'eof':
        return base + unescape(rest)
    if kind == 'both':
        n, a, b = rest.split(':')
        at, _ = line_end(base, int(n))
        return base[:at] + unescape(a + b) + base[at:]
    if kind == 'external':
        at, _ = line_end(base, 9)
        data = base[:at] + b'AABB' + base[at:]
        return data.replace(b'Dernier paragraphe.', b'Dernier paragraphe MODIFIE EXTERNE.')
    raise SystemExit(f'unknown spec {spec}')


def main():
    base_dir, new_dir, rel, spec = sys.argv[1:5]
    kind = spec.split(':')[0]
    a, b = files(base_dir), files(new_dir)
    others = [p for p in sorted(set(a) | set(b)) if p != rel and not p.startswith('.trash') and (p not in a or p not in b or open(a[p], 'rb').read() != open(b[p], 'rb').read())]
    new = open(b[rel], 'rb').read() if rel in b else None
    base = open(a[rel], 'rb').read()
    problems = []
    if others:
        problems.append('other files changed: ' + ', '.join(others))
    if new is None:
        problems.append(f'{rel} is gone')
    elif kind == 'inside':
        n, _, text = spec.split(':', 1)[1].partition(':')
        at, start = line_end(base, int(n))
        want = unescape(text)
        ok = any(new == base[:i] + want + base[i:] for i in range(start, at + 1))
        if not ok:
            problems.append(f'{want!r} is not inserted inside line {n}')
    elif kind == 'kept':
        _, x, y = spec.split(':')
        text = new.decode('utf-8', 'replace')
        for needle in (x, y, 'MODIFIEE EXTERNE'):
            if needle not in text:
                problems.append(f'{needle!r} is missing from the file')
    elif new != expected(base, spec):
        problems.append('the file is not what the typing should give')
    if problems:
        print('; '.join(problems), flush=True)
        subprocess.run([sys.executable, BYTEDIFF, base_dir, new_dir])
        sys.exit(1)


main()

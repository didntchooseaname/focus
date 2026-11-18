#!/usr/bin/env python3
import os, sys

def files(root):
    out = {}
    for d, _, names in os.walk(root):
        for n in names:
            p = os.path.join(d, n)
            out[os.path.relpath(p, root)] = p
    return out

def show(b, limit=160):
    s = b.decode('utf-8', 'replace').replace('\r', '\\r').replace('\n', '\\n\n' if False else '\\n')
    return s if len(s) <= limit else s[:limit // 2] + ' ... ' + s[-limit // 2:]

a, b = files(sys.argv[1]), files(sys.argv[2])
changed = 0
for rel in sorted(set(a) | set(b)):
    if rel.startswith('.trash'):
        continue
    if rel not in b:
        print(f'REMOVED {rel}'); changed += 1; continue
    if rel not in a:
        print(f'ADDED   {rel} ({os.path.getsize(b[rel])} bytes)'); changed += 1; continue
    x, y = open(a[rel], 'rb').read(), open(b[rel], 'rb').read()
    if x == y:
        continue
    changed += 1
    p = 0
    while p < min(len(x), len(y)) and x[p] == y[p]:
        p += 1
    s = 0
    while s < min(len(x), len(y)) - p and x[-1 - s] == y[-1 - s]:
        s += 1
    print(f'CHANGED {rel}: {len(x)} -> {len(y)} bytes ({len(y)-len(x):+d}), first difference at byte {p}, {s} identical bytes at the end')
    print(f'    before: {show(x[p:len(x)-s])}')
    print(f'    after : {show(y[p:len(y)-s])}')
if not changed:
    print('(no file changed)')

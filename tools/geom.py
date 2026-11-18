#!/usr/bin/env python3
import json
import subprocess
import sys

cls = sys.argv[1] if len(sys.argv) > 1 else "focus-capture"
clients = json.loads(subprocess.check_output(["hyprctl", "clients", "-j"]))
wins = [c for c in clients if c["class"] == cls]
if not wins:
    sys.exit(f"no window of class {cls}")
c = wins[-1]
print(f'{c["at"][0]},{c["at"][1]} {c["size"][0]}x{c["size"][1]}')

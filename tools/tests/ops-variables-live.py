import json
import pathlib
import re
import sys

out = pathlib.Path(sys.argv[1])
engagements = pathlib.Path(sys.argv[2])
log = pathlib.Path(sys.argv[3])
failures = 0
SECRETS = ("Winter2026!", "Spring2026!", "Autumn2026!")


def report(ok, label, detail=""):
    global failures
    if any(secret in detail for secret in SECRETS):
        detail = "(text withheld: it holds a secret)"
    print(("ok   " if ok else "FAIL ") + label + ("" if ok or not detail else ": " + detail))
    if not ok:
        failures += 1


def views(name):
    path = out / f"{name}.json"
    if not path.exists():
        report(False, f"{name}: no dump")
        return []
    return json.loads(path.read_text())["views"]


def nxc(view):
    return next((b for b in view["blocks"] if "nxc smb" in b["shown"]), None)


def check(name, label, shown=(), absent=(), copied=(), where=None):
    found = [v for v in views(name) if where is None or where in v["view"] or where in v.get("note", "")]
    blocks = [(v["view"], nxc(v)) for v in found]
    blocks = [(view, block) for view, block in blocks if block]
    if not blocks:
        report(False, f"{label}: no code block drawn" + (f" in {where}" if where else ""))
        return
    for view, block in blocks:
        ok = all(s in block["shown"] for s in shown) and not any(a in block["shown"] for a in absent)
        ok = ok and all(c in block["copied"] for c in copied)
        report(ok, f"{label} ({view})", block["shown"])


check("bar-read", "Ops bar, Read", shown=["nxc smb 10.0.0.1 "])
check("bar-live", "Ops bar, Live", shown=["nxc smb 10.0.0.2 "])
check("bar-inplace", "Ops bar, Live editing in place", shown=["nxc smb 10.0.0.3 "])
check("bar-added", "a variable added by hand", shown=["--port 8080"], copied=["--port 8080"])
check("bar-cleared", "a value cleared", shown=["nxc smb $TARGET "], copied=["nxc smb $TARGET "])
check("used", "Use this credential", shown=["-u alice", "-p '•"], absent=["Winter2026!"],
      copied=["-u alice", "-p 'Winter2026!'"])
check("linked", "a variable linked from the Ops bar", shown=["-u bob"], copied=["-u bob"])
check("followed", "a linked credential that changes", absent=["Autumn2026!"],
      copied=["-p 'Autumn2026!'"])
check("menu-set", "value set in the Variables menu", shown=["--port 9090"], copied=["--port 9090"])
check("menu-escaped", "Escape gives the value back", shown=["--port 9090"], absent=["1111"])
check("menu-cleared", "value cleared in the Variables menu", shown=["--port $PROXY_PORT"])
check("box-off", "box of the Ops bar unchecked", shown=["nxc smb $TARGET -u $USER"])
check("box-on", "box of the Ops bar checked", shown=["nxc smb 10.0.0.4 "])
check("setting-off", "ops.code_variables off", shown=["nxc smb $TARGET -u $USER"])
check("setting-on", "ops.code_variables on", shown=["nxc smb 10.0.0.4 "])
check("behind", "a tab that was behind", shown=["nxc smb 10.0.0.9 "])
check("groups", "first group", shown=["nxc smb 10.0.0.10 "], where="group 1")
check("groups", "second group", shown=["nxc smb 10.0.0.10 "], where="group 2")
check("embed", "an embedded note", shown=["nxc smb 10.0.0.11 "], where="Hosts list")
check("preview", "a link preview", shown=["nxc smb 10.0.0.12 "], where="preview")
check("commands", "the command library", shown=["nxc smb 10.0.0.13 "], copied=["nxc smb 10.0.0.13 "],
      where="commands")
check("engagement", "another engagement, link preview", shown=["nxc smb $TARGET "], where="preview")
check("engagement", "another engagement, command library", shown=["nxc smb $TARGET "], where="commands")

readme = engagements / "Lab" / "README.md"
text = readme.read_text() if readme.exists() else ""
vars_block = re.search(r"^vars:\n((?:[ \t]+.*\n)*)", text, re.M)
stored = vars_block.group(1) if vars_block else ""
report("WEB" in stored and "http://10.0.0.5/" in stored, "the menu adds WEB with its value on disk", stored)
report(re.search(r"LPORT: ['\"]?4444", stored) is not None, "the menu sets LPORT on disk", stored)
report("PROXY_PORT" not in stored, "the menu clears PROXY_PORT on disk", stored)
report("1111" not in text, "a value given back by Escape is not written")
logged = log.read_text() if log.exists() else ""
report(not any(secret in logged for secret in SECRETS), "no secret on the output or the log")
sys.exit(1 if failures else 0)

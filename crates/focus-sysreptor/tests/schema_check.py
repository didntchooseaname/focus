#!/usr/bin/env python3
import json, sys, tarfile, urllib.request
from pathlib import Path

import jsonschema

BASE = "https://raw.githubusercontent.com/Syslifters/sysreptor/96317561b3/api/src/sysreptor"
FILES = {
    "fielddefinition.json": "utils/fielddefinition/fielddefinition.schema.json",
    "section.json": "pentests/fielddefinition/sectiondefinition.schema.json",
    "ordering.json": "pentests/fielddefinition/findingordering.schema.json",
    "cwe.json": "utils/fielddefinition/cwe.json",
}

archive, cache = Path(sys.argv[1]), Path(sys.argv[2])
cache.mkdir(parents=True, exist_ok=True)
for local, remote in FILES.items():
    if not (cache / local).exists():
        urllib.request.urlretrieve(f"{BASE}/{remote}", cache / local)

tar = tarfile.open(archive)
name = next(n for n in tar.getnames() if n.endswith(".json") and "/" not in n)
project = json.load(tar.extractfile(name))
design = project["project_type"]


def check(schema, instance, label):
    jsonschema.Draft202012Validator(json.load(open(cache / schema))).validate(instance)
    print("ok", label)


check("fielddefinition.json", design["finding_fields"], "finding_fields")
check("section.json", design["report_sections"], "report_sections")
check("fielddefinition.json", [f for s in design["report_sections"] for f in s["fields"]], "report fields")
check("ordering.json", design["finding_ordering"], "finding_ordering")
listed = {f"CWE-{c['id']}" for c in json.load(open(cache / "cwe.json"))}
for finding in project["findings"]:
    value = finding["data"].get("cwe")
    assert value is None or value in listed, value
print("ok cwe values")

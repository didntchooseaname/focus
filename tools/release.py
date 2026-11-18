#!/usr/bin/env python3
"""Version and notes of a release, read from the commit history (Conventional Commits 1.0.0).

  tools/release.py next               what the release workflow should do, as key=value lines
  tools/release.py stamp <version>    writes the version into the workspace manifest
  tools/release.py notes <version> [--assets <dir>] [--repository <owner/name>]
  tools/release.py lint <range>       checks the commit messages of a git range

A breaking change raises the major number (the minor one before 1.0.0), `feat` the minor
number, `fix` and `perf` the patch number. Other types release nothing. The version of
the workspace manifest is a floor: the next release never goes below it.
"""

import argparse
import hashlib
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "Cargo.toml"
PRODUCT = "Focus"
TYPES = ("feat", "fix", "perf", "refactor", "docs", "test", "build", "ci", "style", "chore", "revert")
SUBJECT = re.compile(r"^(?P<type>[a-z]+)(?:\((?P<scope>[a-z0-9-]+)\))?(?P<breaking>!)?: (?P<description>.+)$")
BREAKING_FOOTER = re.compile(r"^BREAKING[ -]CHANGE: ", re.MULTILINE)
VERSION_FIELD = re.compile(r'(\[workspace\.package\]\n(?:[^\[]*\n)*?version = ")([^"]+)(")')
SECTIONS = (("breaking", "Breaking changes"), ("feat", "Features"), ("fix", "Fixes"), ("perf", "Performance"))
SYSTEMS = (
    (".AppImage", "Linux x86_64, AppImage"),
    ("-linux-x86_64.tar.gz", "Linux x86_64, archive"),
    ("-windows-x64.zip", "Windows x64"),
    ("-windows-arm64.zip", "Windows 11 on ARM64"),
)


def git(*args):
    return subprocess.run(("git", *args), cwd=ROOT, check=True, capture_output=True, text=True).stdout


def parse(version):
    return tuple(int(part) for part in version.split("."))


def manifest_version():
    return VERSION_FIELD.search(MANIFEST.read_text()).group(2)


def last_tag(commit="HEAD", besides=None):
    tags = git("tag", "--merged", commit, "--list", "v[0-9]*", "--sort=-v:refname").split()
    tags = [tag for tag in tags if tag != besides]
    return tags[0] if tags else None


def commits(since, until="HEAD", merges=False):
    spec = f"{since}..{until}" if since else until
    flags = [] if merges else ["--no-merges"]
    out = git("log", "--reverse", *flags, "--format=%H%x1f%s%x1f%b%x1e", spec)
    return [tuple(field.strip("\n") for field in entry.split("\x1f")) for entry in out.split("\x1e") if entry.strip()]


def kind(subject, body):
    match = SUBJECT.match(subject)
    if not match:
        return None
    if match.group("breaking") or BREAKING_FOOTER.search(body):
        return "breaking"
    return match.group("type")


def bump(version, kinds):
    major, minor, patch = parse(version)
    if "breaking" in kinds:
        return (major + 1, 0, 0) if major else (0, minor + 1, 0)
    if "feat" in kinds:
        return (major, minor + 1, 0)
    if kinds & {"fix", "perf"}:
        return (major, minor, patch + 1)
    return None


def plan(commit="HEAD"):
    floor = parse(manifest_version())
    previous = last_tag(commit)
    if git("tag", "--contains", commit, "--list", "v[0-9]*").split():
        return {"release": "false", "reason": "this commit is already part of a release"}
    if previous is None:
        version = floor
    else:
        kinds = {kind(subject, body) for _, subject, body in commits(previous, commit)}
        version = bump(previous[1:], kinds)
        if version is None:
            return {"release": "false", "reason": f"no feat, fix, perf or breaking change since {previous}"}
        version = max(version, floor)
    text = ".".join(str(part) for part in version)
    if git("tag", "--list", f"v{text}").strip():
        return {"release": "false", "reason": f"v{text} exists on another line of history"}
    return {
        "release": "true",
        "version": text,
        "tag": f"v{text}",
        "previous": previous or "",
        "commit": git("rev-parse", commit).strip(),
    }


def stamp(version):
    parse(version)
    text, count = VERSION_FIELD.subn(rf"\g<1>{version}\g<3>", MANIFEST.read_text())
    if count != 1:
        sys.exit("no version field under [workspace.package] in Cargo.toml")
    MANIFEST.write_text(text)


def size(path):
    return f"{path.stat().st_size / 1_000_000:.1f} MB"


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def notes(version, assets, repository):
    tag = f"v{version}"
    until = tag if git("tag", "--list", tag).strip() else "HEAD"
    previous = last_tag(until, besides=tag)
    lines = []
    if previous is None:
        lines += [f"First release of {PRODUCT}.", ""]
    else:
        groups = {key: [] for key, _ in SECTIONS}
        for commit, subject, body in commits(previous, until):
            key = kind(subject, body)
            if key in groups:
                match = SUBJECT.match(subject)
                description = match.group("description")
                where = f"{match.group('scope')}, " if match.group("scope") else ""
                groups[key].append(f"- {description[0].upper()}{description[1:]} ({where}{commit[:7]})")
        lines += [f"## Changes since {previous[1:]}", ""]
        for key, title in SECTIONS:
            if groups[key]:
                lines += [f"### {title}", "", *groups[key], ""]
    if assets:
        files = sorted(path for path in pathlib.Path(assets).iterdir() if path.is_file())
        rows = [(label, path) for suffix, label in SYSTEMS for path in files if path.name.endswith(suffix)]
        if rows:
            lines += ["## Downloads", "", "| System | File | Size | SHA-256 |", "|---|---|---|---|"]
            lines += [f"| {label} | `{path.name}` | {size(path)} | `{sha256(path)}` |" for label, path in rows]
            lines += [
                "",
                "`SHA256SUMS.txt` holds the same sums: `sha256sum --check --ignore-missing SHA256SUMS.txt`.",
                "",
                "The Windows files are compiled on Linux and are not run before publication. They are "
                "not signed: Windows asks for a confirmation at the first launch.",
                "",
            ]
    if previous and repository:
        lines += [f"Every commit: https://github.com/{repository}/compare/{previous}...{tag}", ""]
    return "\n".join(lines)


def lint(spec):
    problems = []
    for commit, subject, body in commits(None, spec, merges=True):
        short = commit[:7]
        match = SUBJECT.match(subject)
        if not match or match.group("type") not in TYPES:
            problems.append(f"{short}: not a Conventional Commits subject: {subject}")
            continue
        description = match.group("description")
        if len(subject) > 72:
            problems.append(f"{short}: first line of {len(subject)} characters, 72 at most")
        if description[0].isupper() or description.endswith("."):
            problems.append(f"{short}: description starts in lower case and ends without a period")
        message = f"{subject}\n{body}"
        if re.search("[–—]", message):
            problems.append(f"{short}: em dash or en dash in the message")
        if re.search(r"(?im)^co-authored-by:|generated with", message):
            problems.append(f"{short}: co-author or tool mention in the message")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("next")
    commands.add_parser("stamp").add_argument("version")
    notes_command = commands.add_parser("notes")
    notes_command.add_argument("version")
    notes_command.add_argument("--assets")
    notes_command.add_argument("--repository")
    commands.add_parser("lint").add_argument("range")
    args = parser.parse_args()

    if args.command == "next":
        for key, value in plan().items():
            print(f"{key}={value}")
    elif args.command == "stamp":
        stamp(args.version)
    elif args.command == "notes":
        print(notes(args.version, args.assets, args.repository))
    elif args.command == "lint":
        problems = lint(args.range)
        print("\n".join(problems) if problems else "commit messages: ok")
        sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()

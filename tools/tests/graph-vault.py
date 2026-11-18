import random, sys, time
from pathlib import Path

NOTES = 2500
CREATED = 92
BATCHES = (400, 1900, NOTES)
AREAS = ["Recon", "Web", "Active Directory", "Linux", "Windows", "Crypto", "Network", "Cloud", "Journal", "Writeups"]
MARKER = ".graph-vault-v1"


def main(root):
    if (root / MARKER).exists():
        return
    rng = random.Random(7)
    names, folders = [], []
    for i in range(NOTES):
        area = AREAS[i % len(AREAS)] if rng.random() < 0.7 else rng.choice(AREAS)
        folders.append(f"{area}/Topic {rng.randrange(8)}")
        names.append(f"{area} {i:04d}")
    hubs = rng.sample(range(NOTES), 25)
    cited = []
    links = [[] for _ in range(NOTES)]
    for i in range(NOTES):
        count = 0 if rng.random() < 0.12 else min(12, 1 + int(rng.expovariate(0.5)))
        for _ in range(count):
            r = rng.random()
            target = rng.choice(hubs) if r < 0.3 else rng.choice(cited) if cited and r < 0.65 else rng.randrange(NOTES)
            if target != i:
                links[i].append(target)
                cited.append(target)
    created = {i: 1_672_531_200 + rng.randrange(1000) * 86_400 for i in rng.sample(range(NOTES), CREATED)}
    root.mkdir(parents=True, exist_ok=True)
    start = 0
    for end in BATCHES:
        for i in range(start, end):
            front = ""
            if i in created:
                front = "---\ncreated: " + time.strftime("%Y-%m-%d", time.gmtime(created[i])) + "\n---\n"
            body = " ".join(f"[[{names[t]}]]" for t in links[i])
            path = root / folders[i] / f"{names[i]}.md"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"{front}# {names[i]}\n\n{body}\n")
        start = end
        if end != NOTES:
            time.sleep(1.5)
    (root / MARKER).write_text("")


if __name__ == "__main__":
    main(Path(sys.argv[1]))

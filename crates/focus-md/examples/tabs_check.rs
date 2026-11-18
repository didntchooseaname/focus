use focus_md::{Block, BlockKind, ParseOptions, TabsForm, parse};
fn count(blocks: &[Block], f: &mut usize, m: &mut usize) {
    for b in blocks {
        match &b.kind {
            BlockKind::Tabs { form, tabs } => {
                if *form == TabsForm::Fenced {
                    *f += 1
                } else {
                    *m += 1
                }
                for t in tabs {
                    count(&t.blocks, f, m)
                }
            }
            BlockKind::Quote(x)
            | BlockKind::Callout { blocks: x, .. }
            | BlockKind::Details { blocks: x, .. } => count(x, f, m),
            BlockKind::List { items, .. } => items.iter().for_each(|i| count(&i.blocks, f, m)),
            _ => {}
        }
    }
}
fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            if !p.file_name().unwrap().to_string_lossy().starts_with('.') {
                walk(&p, out)
            }
        } else if p.extension().is_some_and(|x| x == "md") {
            out.push(p)
        }
    }
}
fn main() {
    let mut files = Vec::new();
    walk(
        std::path::Path::new(&std::env::args().nth(1).unwrap()),
        &mut files,
    );
    let (mut tf, mut tm) = (0, 0);
    for p in files {
        let s = std::fs::read_to_string(&p).unwrap_or_default();
        let d = parse(&s, &ParseOptions::default());
        let (mut f, mut m) = (0, 0);
        count(&d.blocks, &mut f, &mut m);
        let fo = s
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                (t.starts_with("```") || t.starts_with("~~~"))
                    && t.trim_start_matches(['`', '~']).trim() == "tabs"
            })
            .count();
        let mo = s
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("{{#tabs") && t.ends_with("}}")
            })
            .count();
        if f != fo || m != mo {
            println!("{} fenced {f}/{fo} mdbook {m}/{mo}", p.display());
        }
        tf += f;
        tm += m;
    }
    println!("fenced {tf}, mdbook {tm}");
}

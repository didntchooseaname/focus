use crate::fold::{FoldMode, fold_for_index};
use regex_syntax::hir::{Class, Hir, HirKind};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Req {
    All,
    Tri(u32),
    And(Vec<Req>),
    Or(Vec<Req>),
}

impl Req {
    fn and(a: Req, b: Req) -> Req {
        match (a, b) {
            (Req::All, x) | (x, Req::All) => x,
            (Req::And(mut v), Req::And(w)) => {
                v.extend(w);
                Req::And(v)
            }
            (Req::And(mut v), x) | (x, Req::And(mut v)) => {
                v.push(x);
                Req::And(v)
            }
            (a, b) => Req::And(vec![a, b]),
        }
    }

    fn or(items: Vec<Req>) -> Req {
        if items.is_empty() || items.contains(&Req::All) {
            return Req::All;
        }
        let mut items = items;
        if items.len() == 1 {
            items.pop().unwrap_or(Req::All)
        } else {
            Req::Or(items)
        }
    }
}

pub(crate) fn trigram_key(t: &[u8]) -> u32 {
    (t[0] as u32) << 16 | (t[1] as u32) << 8 | t[2] as u32
}

pub(crate) fn literal(text: &str) -> Req {
    let mut folded = Vec::new();
    fold_for_index(text, &mut folded);
    if folded.len() < 3 {
        return Req::All;
    }
    let mut keys: Vec<u32> = folded.windows(3).map(trigram_key).collect();
    keys.sort_unstable();
    keys.dedup();
    Req::And(keys.into_iter().map(Req::Tri).collect())
}

const MAX_EXACT: usize = 32;

struct Info {
    exact: Option<BTreeSet<String>>,
    req: Req,
}

fn exact_req(set: &BTreeSet<String>) -> Req {
    Req::or(set.iter().map(|s| literal(s)).collect())
}

impl Info {
    fn into_req(self) -> Req {
        match self.exact {
            Some(set) => Req::and(exact_req(&set), self.req),
            None => self.req,
        }
    }
    fn any() -> Info {
        Info {
            exact: None,
            req: Req::All,
        }
    }
    fn empty_string() -> Info {
        Info {
            exact: Some(BTreeSet::from([String::new()])),
            req: Req::All,
        }
    }
}

fn fold(s: &str) -> String {
    FoldMode::FULL.str(s)
}

fn info(hir: &Hir) -> Info {
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => Info::empty_string(),
        HirKind::Literal(lit) => {
            let s = String::from_utf8_lossy(&lit.0);
            Info {
                exact: Some(BTreeSet::from([fold(&s)])),
                req: Req::All,
            }
        }
        HirKind::Class(Class::Unicode(cls)) => {
            let mut set = BTreeSet::new();
            let mut count = 0u32;
            for r in cls.ranges() {
                count += r.end() as u32 - r.start() as u32 + 1;
                if count > 16 {
                    return Info::any();
                }
                for c in r.start()..=r.end() {
                    set.insert(fold(&c.to_string()));
                }
            }
            Info {
                exact: Some(set),
                req: Req::All,
            }
        }
        HirKind::Class(Class::Bytes(_)) => Info::any(),
        HirKind::Capture(c) => info(&c.sub),
        HirKind::Repetition(rep) => {
            if rep.min == 0 {
                return Info::any();
            }
            let sub = info(&rep.sub);
            if rep.min == 1 && rep.max == Some(1) {
                return sub;
            }
            Info {
                exact: None,
                req: sub.into_req(),
            }
        }
        HirKind::Concat(items) => {
            let mut req = Req::All;
            let mut acc: Option<BTreeSet<String>> = Some(BTreeSet::from([String::new()]));
            for item in items {
                let i = info(item);
                req = Req::and(req, i.req);
                match (acc.take(), i.exact) {
                    (Some(a), Some(b)) if a.len() * b.len() <= MAX_EXACT => {
                        let mut next = BTreeSet::new();
                        for x in &a {
                            for y in &b {
                                next.insert(format!("{x}{y}"));
                            }
                        }
                        acc = Some(next);
                    }
                    (Some(a), Some(b)) => {
                        req = Req::and(req, exact_req(&a));
                        acc = Some(b);
                    }
                    (Some(a), None) => {
                        req = Req::and(req, exact_req(&a));
                        acc = Some(BTreeSet::from([String::new()]));
                    }
                    (None, _) => unreachable!("acc is set back to Some on every iteration"),
                }
            }
            Info { exact: acc, req }
        }
        HirKind::Alternation(items) => {
            let infos: Vec<Info> = items.iter().map(info).collect();
            if infos.iter().all(|i| i.exact.is_some() && i.req == Req::All) {
                let mut set = BTreeSet::new();
                for i in &infos {
                    set.extend(i.exact.iter().flatten().cloned());
                }
                if set.len() <= MAX_EXACT {
                    return Info {
                        exact: Some(set),
                        req: Req::All,
                    };
                }
            }
            Info {
                exact: None,
                req: Req::or(infos.into_iter().map(Info::into_req).collect()),
            }
        }
    }
}

pub(crate) fn regex(pattern: &str, case_insensitive: bool) -> Req {
    let hir = regex_syntax::ParserBuilder::new()
        .case_insensitive(case_insensitive)
        .multi_line(true)
        .build()
        .parse(pattern);
    match hir {
        Ok(h) => info(&h).into_req(),
        Err(_) => Req::All,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tris(s: &str) -> Vec<String> {
        fn walk(r: &Req, out: &mut Vec<String>) {
            match r {
                Req::Tri(k) => out.push(
                    String::from_utf8_lossy(&[(k >> 16) as u8, (k >> 8) as u8, *k as u8])
                        .into_owned(),
                ),
                Req::And(v) | Req::Or(v) => v.iter().for_each(|x| walk(x, out)),
                Req::All => {}
            }
        }
        let mut out = Vec::new();
        walk(&regex(s, true), &mut out);
        out.sort();
        out.dedup();
        out
    }

    #[test]
    fn literals_from_regexes() {
        let k = tris("kerberos");
        for x in ["ber", "erb", "ero", "ker", "ros", "rbe"] {
            assert!(k.contains(&x.to_string()), "{x} absent de {k:?}");
        }
        assert!(tris("Kerb.*ros").contains(&"erb".to_string()));
        assert!(!tris("Kerb.*ros").contains(&"b.r".to_string()));
        let t = tris("(golden|silver) ticket");
        for x in ["gol", "sil", "tic", "ket", "en ", "er "] {
            assert!(t.contains(&x.to_string()), "{x} absent de {t:?}");
        }
        assert!(matches!(
            regex("(golden|silver) ticket", true),
            Req::And(_) | Req::Or(_)
        ));
        assert_eq!(regex("a.b", true), Req::All);
        assert_eq!(regex("(abc)?", true), Req::All);
        assert!(matches!(regex("x(abc)+y", true), Req::And(_)));
        assert_eq!(tris("Élévation"), tris("elevation"));
    }
}

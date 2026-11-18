#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FoldMode {
    pub case: bool,
    pub accents: bool,
}

impl FoldMode {
    pub const FULL: FoldMode = FoldMode {
        case: true,
        accents: true,
    };

    pub fn for_term(term: &str, case_sensitive: bool, accent_sensitive: bool) -> FoldMode {
        let has_upper = term.chars().any(char::is_uppercase);
        let has_accent = term.chars().any(|c| focus_core::strip_accent(c) != c);
        FoldMode {
            case: !(case_sensitive || has_upper),
            accents: !(accent_sensitive || has_accent),
        }
    }

    pub fn is_identity(self) -> bool {
        !self.case && !self.accents
    }

    #[inline]
    pub fn char(self, c: char) -> char {
        if c.is_ascii() {
            return if self.case { c.to_ascii_lowercase() } else { c };
        }
        let c = if self.case {
            c.to_lowercase().next().unwrap_or(c)
        } else {
            c
        };
        if self.accents {
            focus_core::strip_accent(c)
        } else {
            c
        }
    }

    pub fn str(self, s: &str) -> String {
        s.chars().map(|c| self.char(c)).collect()
    }
}

pub(crate) struct Folded {
    pub text: String,
    map: Vec<(u32, u32)>,
    orig_len: usize,
}

impl Folded {
    pub fn new(src: &str, mode: FoldMode) -> Folded {
        if src.is_ascii() || mode.is_identity() {
            let text = if mode.case && !mode.is_identity() {
                src.to_ascii_lowercase()
            } else {
                src.to_string()
            };
            return Folded {
                text,
                map: Vec::new(),
                orig_len: src.len(),
            };
        }
        let mut text = String::with_capacity(src.len());
        let mut map = Vec::new();
        let mut shifted = false;
        for (i, c) in src.char_indices() {
            let f = mode.char(c);
            if f.len_utf8() != c.len_utf8() {
                shifted = true;
            }
            if shifted {
                map.push((text.len() as u32, i as u32));
            }
            text.push(f);
        }
        if !shifted {
            map.clear();
        }
        Folded {
            text,
            map,
            orig_len: src.len(),
        }
    }

    pub fn orig(&self, folded: usize) -> usize {
        if self.map.is_empty() {
            return folded;
        }
        if folded >= self.text.len() {
            return self.orig_len;
        }
        match self.map.binary_search_by_key(&(folded as u32), |&(f, _)| f) {
            Ok(i) => self.map[i].1 as usize,
            Err(0) => folded,
            Err(i) => {
                let (f, o) = self.map[i - 1];
                o as usize + (folded - f as usize)
            }
        }
    }
}

pub(crate) fn fold_for_index(src: &str, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(src.len());
    if src.is_ascii() {
        out.extend(src.bytes().map(|b| b.to_ascii_lowercase()));
        return;
    }
    let mut buf = [0u8; 4];
    for c in src.chars() {
        if c.is_ascii() {
            out.push((c as u8).to_ascii_lowercase());
        } else {
            out.extend_from_slice(FoldMode::FULL.char(c).encode_utf8(&mut buf).as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_follow_the_term() {
        assert_eq!(
            FoldMode::for_term("elevation", false, false),
            FoldMode {
                case: true,
                accents: true
            }
        );
        assert_eq!(
            FoldMode::for_term("élévation", false, false),
            FoldMode {
                case: true,
                accents: false
            }
        );
        assert_eq!(
            FoldMode::for_term("Kerberos", false, false),
            FoldMode {
                case: false,
                accents: true
            }
        );
        assert_eq!(
            FoldMode::for_term("kerberos", true, false),
            FoldMode {
                case: false,
                accents: true
            }
        );
        assert_eq!(
            FoldMode::for_term("elevation", false, true),
            FoldMode {
                case: true,
                accents: false
            }
        );
    }

    #[test]
    fn positions_map_back() {
        let src = "Élévation ÇA ok";
        let f = Folded::new(src, FoldMode::FULL);
        assert_eq!(f.text, "elevation ca ok");
        let at = f.text.find("ca").unwrap();
        assert_eq!(&src[f.orig(at)..f.orig(at + 2)], "ÇA");
        let at = f.text.find("ok").unwrap();
        assert_eq!(&src[f.orig(at)..f.orig(at + 2)], "ok");
        assert_eq!(f.orig(f.text.len()), src.len());
        let same = Folded::new(
            "Abc",
            FoldMode {
                case: true,
                accents: true,
            },
        );
        assert_eq!(same.text, "abc");
    }

    #[test]
    fn index_fold() {
        let mut out = Vec::new();
        fold_for_index("Élévation de PRIVILÈGES", &mut out);
        assert_eq!(out, b"elevation de privileges");
    }
}

use std::path::{Path, PathBuf};

pub const URI_LIST: &str = "text/uri-list";
pub const GNOME_FILES: &str = "x-special/gnome-copied-files";
pub const KDE_CUT: &str = "application/x-kde-cutselection";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
    pub refused: usize,
}

pub struct Offer {
    pub types: Vec<(&'static str, Vec<u8>)>,
}

impl Offer {
    pub fn new(paths: &[PathBuf], cut: bool) -> Option<Offer> {
        if paths.is_empty()
            || paths
                .iter()
                .any(|p| !p.is_absolute() || p.as_os_str().as_encoded_bytes().contains(&0))
        {
            return None;
        }
        let uris: Vec<String> = paths.iter().map(|p| uri_of(p)).collect();
        let list: String = uris.iter().map(|u| format!("{u}\r\n")).collect();
        let gnome = format!("{}\n{}", if cut { "cut" } else { "copy" }, uris.join("\n"));
        let text: Vec<u8> = paths
            .iter()
            .map(|p| p.as_os_str().as_encoded_bytes().to_vec())
            .collect::<Vec<_>>()
            .join(&b'\n');
        let mut types = vec![
            (URI_LIST, list.into_bytes()),
            (GNOME_FILES, gnome.into_bytes()),
            ("text/plain;charset=utf-8", text),
        ];
        if cut {
            types.insert(2, (KDE_CUT, b"1".to_vec()));
        }
        Some(Offer { types })
    }

    pub fn single(&self) -> &(&'static str, Vec<u8>) {
        self.types
            .iter()
            .find(|(mime, _)| *mime == GNOME_FILES)
            .unwrap_or(&self.types[0])
    }
}

pub fn uri_of(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in address_bytes(path) {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

pub fn path_of(uri: &str) -> Option<PathBuf> {
    let (scheme, rest) = uri.trim().split_once(':')?;
    if !scheme.eq_ignore_ascii_case("file") {
        return None;
    }
    let after = rest.strip_prefix("//").unwrap_or(rest);
    let path = if rest.starts_with("//") {
        let (host, path) = after.split_at(after.find('/')?);
        if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
            return None;
        }
        path
    } else {
        after
    };
    let bytes = decode(path)?;
    if bytes.first() != Some(&b'/') || bytes.contains(&0) {
        return None;
    }
    local_path(bytes)
}

#[cfg(unix)]
fn address_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}

#[cfg(windows)]
fn address_bytes(path: &Path) -> Vec<u8> {
    format!("/{}", path.to_string_lossy().replace('\\', "/")).into_bytes()
}

#[cfg(unix)]
fn local_path(bytes: Vec<u8>) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

#[cfg(windows)]
fn local_path(bytes: Vec<u8>) -> Option<PathBuf> {
    let text = String::from_utf8(bytes).ok()?;
    let path = PathBuf::from(text[1..].replace('/', "\\"));
    path.is_absolute().then_some(path)
}

fn decode(text: &str) -> Option<Vec<u8>> {
    let raw = text.as_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' {
            let pair = raw.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(raw[i]);
            i += 1;
        }
    }
    Some(out)
}

fn collect<'a>(lines: impl Iterator<Item = &'a str>, cut: bool) -> ClipboardFiles {
    let mut files = ClipboardFiles {
        cut,
        ..Default::default()
    };
    for line in lines
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        match path_of(line) {
            Some(path) => files.paths.push(path),
            None => files.refused += 1,
        }
    }
    files
}

pub fn parse_uri_list(text: &str, cut: bool) -> ClipboardFiles {
    collect(text.lines(), cut)
}

pub fn parse_gnome(text: &str) -> ClipboardFiles {
    let mut lines = text.lines();
    let cut = lines
        .next()
        .is_some_and(|first| first.trim().eq_ignore_ascii_case("cut"));
    collect(lines, cut)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt as _;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn spaces_accents_and_a_hash_are_encoded_and_read_back() {
        let path = Path::new("/home/a b/caf\u{e9} #1/50% done.md");
        let uri = uri_of(path);
        assert_eq!(uri, "file:///home/a%20b/caf%C3%A9%20%231/50%25%20done.md");
        assert_eq!(path_of(&uri).as_deref(), Some(path));
    }

    #[test]
    fn a_name_that_is_not_utf8_survives() {
        let path = PathBuf::from(std::ffi::OsString::from_vec(
            b"/tmp/bad\xff\xfename".to_vec(),
        ));
        assert_eq!(path_of(&uri_of(&path)), Some(path));
    }

    #[test]
    fn the_offer_carries_the_three_types_and_the_marker_of_a_cut() {
        let list = paths(&["/vault/a b.md", "/vault/dir"]);
        let copy = Offer::new(&list, false).unwrap();
        let kinds: Vec<&str> = copy.types.iter().map(|(m, _)| *m).collect();
        assert_eq!(kinds, [URI_LIST, GNOME_FILES, "text/plain;charset=utf-8"]);
        assert_eq!(
            copy.types[0].1,
            b"file:///vault/a%20b.md\r\nfile:///vault/dir\r\n"
        );
        assert_eq!(
            copy.types[1].1,
            b"copy\nfile:///vault/a%20b.md\nfile:///vault/dir"
        );
        assert_eq!(copy.types[2].1, b"/vault/a b.md\n/vault/dir");
        let cut = Offer::new(&list, true).unwrap();
        assert!(
            cut.types
                .iter()
                .any(|(m, body)| *m == KDE_CUT && body == b"1")
        );
        assert!(cut.types[1].1.starts_with(b"cut\n"));
        assert_eq!(cut.single().0, GNOME_FILES);
    }

    #[test]
    fn a_relative_path_or_nothing_makes_no_offer() {
        assert!(Offer::new(&[], false).is_none());
        assert!(Offer::new(&paths(&["relative.md"]), false).is_none());
    }

    #[test]
    fn what_is_written_is_read_back_in_both_forms() {
        let list = paths(&["/v/a b#.md", "/v/caf\u{e9}"]);
        let offer = Offer::new(&list, true).unwrap();
        let gnome = parse_gnome(std::str::from_utf8(&offer.types[1].1).unwrap());
        assert_eq!(
            (gnome.paths.as_slice(), gnome.cut, gnome.refused),
            (list.as_slice(), true, 0)
        );
        let plain = parse_uri_list(std::str::from_utf8(&offer.types[0].1).unwrap(), false);
        assert_eq!(
            (plain.paths.as_slice(), plain.cut),
            (list.as_slice(), false)
        );
    }

    #[test]
    fn remote_addresses_are_refused_and_counted() {
        let text = "copy\nfile:///v/a.md\nsftp://host/home/x.md\nsmb://srv/share/y.md\nfile://other-host/etc/passwd\nfile://localhost/v/b.md\nfile:/v/c.md\nfile:///v/bad%zz\nfile:///v/%00nul\nfile://v/relative\nhttps://example.org/z\n";
        let files = parse_gnome(text);
        assert_eq!(files.paths, paths(&["/v/a.md", "/v/b.md", "/v/c.md"]));
        assert_eq!(files.refused, 7);
    }

    #[test]
    fn comments_and_blank_lines_are_not_addresses() {
        let files = parse_uri_list("# a comment\r\n\r\nfile:///v/a.md\r\n", false);
        assert_eq!((files.paths, files.refused), (paths(&["/v/a.md"]), 0));
    }
}

use crate::Error;
use semver::Version;

pub(crate) struct Listing {
    pub version: Version,
    pub files: Vec<(String, [u8; 32])>,
}

pub(crate) fn parse(text: &str) -> Result<Listing, Error> {
    let mut version: Option<Version> = None;
    let mut files = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let malformed = || Error::Malformed(line.to_string());
        let (digest, name) = line.split_once(' ').ok_or_else(malformed)?;
        let name = name.strip_prefix([' ', '*']).ok_or_else(malformed)?;
        let digest = digest_of(digest).ok_or_else(malformed)?;
        let named = version_of(name).ok_or_else(malformed)?;
        match &version {
            Some(known) if *known != named => return Err(malformed()),
            Some(_) => {}
            None => version = Some(named),
        }
        files.push((name.to_string(), digest));
    }
    let version = version.ok_or_else(|| Error::Malformed("no file listed".into()))?;
    Ok(Listing { version, files })
}

fn version_of(name: &str) -> Option<Version> {
    if name.contains(['/', '\\']) {
        return None;
    }
    let rest = name
        .strip_prefix("focus-")
        .or_else(|| name.strip_prefix("Focus-"))?;
    let (version, _) = rest.split_once('-')?;
    Version::parse(version).ok()
}

fn digest_of(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut digest = [0u8; 32];
    for (byte, pair) in digest.iter_mut().zip(hex.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0f343b0931126a20f133d67c2b018a3b5e1b2d1d1a1bd2c4a3d0e8f9a0b1c2d3";
    const B: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    #[test]
    fn every_file_of_a_release_is_listed_with_its_digest() {
        let listing = parse(&format!(
            "{A}  focus-1.4.2-linux-x86_64.tar.gz\n{B} *Focus-1.4.2-x86_64.AppImage\n\n"
        ))
        .unwrap();
        assert_eq!(listing.version, Version::new(1, 4, 2));
        assert_eq!(listing.files.len(), 2);
        assert_eq!(listing.files[0].0, "focus-1.4.2-linux-x86_64.tar.gz");
        assert_eq!(listing.files[0].1[0], 0x0f);
        assert_eq!(
            listing.files[1],
            ("Focus-1.4.2-x86_64.AppImage".into(), [0xff; 32])
        );
    }

    #[test]
    fn two_versions_in_one_list_are_refused() {
        let text =
            format!("{A}  focus-1.4.2-linux-x86_64.tar.gz\n{B}  focus-1.4.3-windows-x64.zip\n");
        assert!(matches!(parse(&text), Err(Error::Malformed(line)) if line.contains("1.4.3")));
    }

    #[test]
    fn a_line_that_is_not_a_digest_and_a_release_file_is_refused() {
        for line in [
            "focus-1.4.2-linux-x86_64.tar.gz".to_string(),
            format!("{A}  ../focus-1.4.2-linux-x86_64.tar.gz"),
            format!("{A}  notes.txt"),
            format!("{A}  focus-latest-linux-x86_64.tar.gz"),
            format!("{}  focus-1.4.2-linux-x86_64.tar.gz", &A[..62]),
            format!("{}zz  focus-1.4.2-linux-x86_64.tar.gz", &A[..62]),
            format!("{A} focus-1.4.2-linux-x86_64.tar.gz"),
        ] {
            assert!(
                matches!(parse(&line), Err(Error::Malformed(_))),
                "accepted: {line}"
            );
        }
    }

    #[test]
    fn an_empty_list_is_refused() {
        assert!(matches!(parse("\n\n"), Err(Error::Malformed(_))));
    }
}

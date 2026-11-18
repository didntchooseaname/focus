mod fetch;
mod place;
mod sums;

pub use place::{Installation, Package};
pub use semver::Version;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

pub const RELEASES: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

pub const RELEASE_KEY: &str = include_str!("../../../assets/release.pub");

const LIST: &str = "SHA256SUMS.txt";
const MAX_LIST_BYTES: u64 = 64 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 4 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{url}: {detail}")]
    Network { url: String, detail: String },
    #[error("the latest release carries no signature")]
    Unsigned,
    #[error("the release list is not signed by the release key")]
    Signature,
    #[error("the release list is malformed: {0}")]
    Malformed(String),
    #[error("the release has no {0}")]
    Missing(String),
    #[error("{0} does not match its SHA-256 in the release list")]
    Checksum(String),
    #[error("{0} holds no program")]
    Incomplete(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub releases: String,
    pub key: String,
}

impl Source {
    pub fn official() -> Source {
        Source {
            releases: RELEASES.to_string(),
            key: RELEASE_KEY.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    files: Vec<(String, [u8; 32])>,
    releases: String,
}

impl Release {
    pub fn page(&self) -> String {
        format!("{}/tag/v{}", self.releases, self.version)
    }

    fn digest(&self, name: &str) -> Option<[u8; 32]> {
        self.files
            .iter()
            .find(|(listed, _)| listed == name)
            .map(|(_, digest)| *digest)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub received: u64,
    pub total: Option<u64>,
}

pub fn check(source: &Source, current: &Version) -> Result<Option<Release>, Error> {
    let agent = fetch::agent(&source.releases);
    let list = format!("{}/latest/download/{LIST}", source.releases);
    let Some(text) = fetch::text(&agent, &list, MAX_LIST_BYTES)? else {
        return Ok(None);
    };
    let signature = fetch::text(&agent, &format!("{list}.minisig"), MAX_SIGNATURE_BYTES)?
        .ok_or(Error::Unsigned)?;
    verify(&source.key, text.as_bytes(), &signature)?;
    let listing = sums::parse(&text)?;
    Ok((listing.version > *current).then(|| Release {
        version: listing.version,
        files: listing.files,
        releases: source.releases.clone(),
    }))
}

pub fn install(
    release: &Release,
    installation: &Installation,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<(), Error> {
    let name = installation.file(&release.version);
    let expected = release
        .digest(&name)
        .ok_or_else(|| Error::Missing(name.clone()))?;
    installation.tidy();
    let mut staging = place::Staging::new(installation);
    let (path, mut file) = staging.create("download")?;
    let url = format!("{}/download/v{}/{name}", release.releases, release.version);
    let agent = fetch::agent(&release.releases);
    let digest = fetch::file(
        &agent,
        &url,
        (&path, &mut file),
        MAX_FILE_BYTES,
        cancel,
        progress,
    )?;
    if digest != expected {
        return Err(Error::Checksum(name));
    }
    file.sync_all().map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    drop(file);
    staging.place(&path, &name)
}

fn verify(key: &str, bytes: &[u8], signature: &str) -> Result<(), Error> {
    let key = minisign_verify::PublicKey::decode(key).map_err(|_| Error::Signature)?;
    let signature = minisign_verify::Signature::decode(signature).map_err(|_| Error::Signature)?;
    key.verify(bytes, &signature, false)
        .map_err(|_| Error::Signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_key_of_the_repository_is_a_minisign_public_key() {
        assert!(minisign_verify::PublicKey::decode(RELEASE_KEY).is_ok());
    }

    #[test]
    fn the_releases_are_those_of_the_repository_over_https() {
        assert!(RELEASES.starts_with("https://"));
        assert!(RELEASES.ends_with("/releases"));
    }
}

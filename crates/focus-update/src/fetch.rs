use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest as _, Sha256};

use crate::{Error, Progress};

const CONNECT_WITHIN: Duration = Duration::from_secs(10);
const ANSWER_WITHIN: Duration = Duration::from_secs(20);
const READ_WITHIN: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: u32 = 5;
const NOT_FOUND: u16 = 404;

pub(crate) fn agent(releases: &str) -> ureq::Agent {
    let loopback = releases.starts_with("http://127.0.0.1:");
    let mut config = ureq::Agent::config_builder()
        .user_agent("focus")
        .https_only(!loopback)
        .max_redirects(MAX_REDIRECTS)
        .timeout_connect(Some(CONNECT_WITHIN))
        .timeout_recv_response(Some(ANSWER_WITHIN))
        .timeout_per_read(Some(READ_WITHIN));
    if loopback {
        config = config.proxy(None);
    }
    config.build().into()
}

fn network(url: &str, detail: impl ToString) -> Error {
    Error::Network {
        url: url.to_string(),
        detail: detail.to_string(),
    }
}

pub(crate) fn text(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Option<String>, Error> {
    let mut response = match agent.get(url).call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(NOT_FOUND)) => return Ok(None),
        Err(error) => return Err(network(url, error)),
    };
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
        .map(Some)
        .map_err(|error| network(url, error))
}

pub(crate) fn file(
    agent: &ureq::Agent,
    url: &str,
    (path, to): (&PathBuf, &mut File),
    limit: u64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<[u8; 32], Error> {
    let response = match agent.get(url).call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(NOT_FOUND)) => {
            return Err(Error::Missing(url.rsplit('/').next().unwrap_or(url).into()));
        }
        Err(error) => return Err(network(url, error)),
    };
    let total = response.body().content_length();
    let mut reader = response
        .into_body()
        .into_with_config()
        .limit(limit)
        .reader();
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    progress(Progress { received, total });
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| network(url, error))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        to.write_all(&buffer[..read]).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        received += read as u64;
        progress(Progress { received, total });
    }
    Ok(hash.finalize().into())
}

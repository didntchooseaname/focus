use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use gpui::{App, Global};
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

pub(crate) const MAX_DOWNLOAD: u64 = 20 * 1024 * 1024;
const MAX_REDIRECTS: u32 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RemoteImages {
    Never,
    #[default]
    OnClick,
    Always,
}

impl RemoteImages {
    pub fn from_index(index: usize) -> RemoteImages {
        match index {
            0 => RemoteImages::Never,
            2 => RemoteImages::Always,
            _ => RemoteImages::OnClick,
        }
    }
}

struct Policy(RemoteImages);
impl Global for Policy {}

pub fn set_remote_images(policy: RemoteImages, cx: &mut App) {
    cx.set_global(Policy(policy));
    cx.refresh_windows();
}

pub fn remote_images(cx: &App) -> RemoteImages {
    cx.try_global::<Policy>()
        .map_or_else(RemoteImages::default, |p| p.0)
}

pub fn host_of(url: &str) -> Option<String> {
    let uri: ureq::http::Uri = url.parse().ok()?;
    matches!(uri.scheme_str(), Some("http" | "https"))
        .then(|| uri.host().map(str::to_string))
        .flatten()
}

pub fn is_forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => forbidden_v4(v4),
        IpAddr::V6(v6) => forbidden_v6(v6),
    }
}

fn forbidden_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 240
}

fn forbidden_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    if let Some(v4) = ip.to_ipv4_mapped() {
        return forbidden_v4(v4);
    }
    let nat64 = s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0];
    let compatible = s[0..6] == [0, 0, 0, 0, 0, 0] && !(ip.is_unspecified() || ip.is_loopback());
    if nat64 || compatible {
        return forbidden_v4(Ipv4Addr::new(
            (s[6] >> 8) as u8,
            s[6] as u8,
            (s[7] >> 8) as u8,
            s[7] as u8,
        ));
    }
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00
        || (s[0] & 0xffc0) == 0xfe80
        || (s[0] & 0xffc0) == 0xfec0
}

#[derive(Debug, Default)]
struct GuardedResolver(DefaultResolver);

impl Resolver for GuardedResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addresses = self.0.resolve(uri, config, timeout)?;
        if let Some(local) = addresses.iter().find(|a| is_forbidden(a.ip())) {
            return Err(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "{} resolves to a local address ({})",
                    uri.host().unwrap_or(""),
                    local.ip()
                ),
            )));
        }
        Ok(addresses)
    }
}

#[derive(Debug, PartialEq)]
pub enum Refused {
    Scheme,
    Local(String),
    NotImage(String),
    TooLarge,
    Failed(String),
}

impl Refused {
    pub fn text(&self, url: &str) -> String {
        let host = host_of(url).unwrap_or_else(|| url.to_string());
        match self {
            Refused::Scheme => focus_i18n::t_args("reading-remote-scheme", &[("src", url.into())]),
            Refused::Local(_) => {
                focus_i18n::t_args("reading-remote-local", &[("host", host.as_str().into())])
            }
            Refused::NotImage(kind) => focus_i18n::t_args(
                "reading-remote-not-image",
                &[
                    ("host", host.as_str().into()),
                    ("type", kind.as_str().into()),
                ],
            ),
            Refused::TooLarge => focus_i18n::t_args(
                "reading-remote-too-large",
                &[
                    ("host", host.as_str().into()),
                    ("megabytes", ((MAX_DOWNLOAD / (1024 * 1024)) as i64).into()),
                ],
            ),
            Refused::Failed(detail) => focus_i18n::t_args(
                "reading-remote-failed",
                &[
                    ("host", host.as_str().into()),
                    ("detail", detail.as_str().into()),
                ],
            ),
        }
    }
}

pub(crate) fn fetch(url: &str) -> Result<Vec<u8>, Refused> {
    if host_of(url).is_none() {
        return Err(Refused::Scheme);
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(MAX_REDIRECTS)
        .proxy(None)
        .build();
    let agent =
        ureq::Agent::with_parts(config, DefaultConnector::new(), GuardedResolver::default());
    let mut response = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied => {
            Refused::Local(io.to_string())
        }
        other => Refused::Failed(other.to_string()),
    })?;
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !kind.starts_with("image/") || kind.starts_with("image/svg") {
        return Err(Refused::NotImage(kind));
    }
    let declared = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|len| len > MAX_DOWNLOAD) {
        return Err(Refused::TooLarge);
    }
    response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => Refused::TooLarge,
            other => Refused::Failed(other.to_string()),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forbidden(ip: &str) -> bool {
        is_forbidden(ip.parse().expect("an address"))
    }

    #[test]
    fn only_public_addresses_are_allowed() {
        for ip in [
            "127.0.0.1",
            "127.8.9.1",
            "0.0.0.0",
            "10.10.10.5",
            "172.16.0.1",
            "172.31.255.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "192.0.0.8",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "240.0.0.1",
            "::1",
            "::",
            "fd00::1",
            "fc00::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "::7f00:1",
        ] {
            assert!(forbidden(ip), "{ip} must be refused");
        }
        for ip in [
            "93.184.215.14",
            "1.1.1.1",
            "172.32.0.1",
            "100.128.0.1",
            "2606:4700::1111",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
        ] {
            assert!(!forbidden(ip), "{ip} is public");
        }
    }

    #[test]
    fn local_addresses_and_names_are_refused_before_any_request() {
        for url in [
            "http://127.0.0.1:9/a.png",
            "http://[::1]:9/a.png",
            "https://10.0.0.1/a.png",
            "http://169.254.169.254/latest/meta-data",
            "http://localhost:9/a.png",
        ] {
            assert!(matches!(fetch(url), Err(Refused::Local(_))), "{url}");
        }
        assert_eq!(fetch("file:///etc/passwd"), Err(Refused::Scheme));
        assert_eq!(fetch("ftp://example.com/a.png"), Err(Refused::Scheme));
    }

    #[test]
    fn the_setting_index_maps_to_a_policy() {
        assert_eq!(RemoteImages::from_index(0), RemoteImages::Never);
        assert_eq!(RemoteImages::from_index(1), RemoteImages::OnClick);
        assert_eq!(RemoteImages::from_index(2), RemoteImages::Always);
        assert_eq!(RemoteImages::from_index(7), RemoteImages::OnClick);
        assert_eq!(
            host_of("https://images.example.org:8443/x.png").as_deref(),
            Some("images.example.org")
        );
    }
}

use std::net::IpAddr;

use ipnet::IpNet;
use thiserror::Error;
use url::{Host, Url};

pub const MAX_URL_BYTES: usize = 4 * 1024;
pub const MAX_URLS_PER_JOB: i64 = 500;
pub const MAX_DEPTH: i32 = 3;
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REDIRECTS: usize = 5;
pub const MAX_LINK_CANDIDATES: usize = 1_000;
pub const MAX_ATTEMPTS: i32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUrl {
    pub canonical: String,
    pub host: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UrlPolicyError {
    #[error("URL is empty, malformed, or too long")]
    InvalidUrl,
    #[error("only HTTP and HTTPS URLs are allowed")]
    UnsupportedScheme,
    #[error("URL credentials are not allowed")]
    Credentials,
    #[error("only default HTTP and HTTPS ports are allowed")]
    NonDefaultPort,
    #[error("IP literal hosts are not allowed")]
    IpLiteral,
    #[error("host is invalid")]
    InvalidHost,
}

/// Parse and canonicalize one crawl URL. `url` performs IDNA conversion,
/// lowercases hostnames, removes default ports and resolves dot segments.
pub fn normalize_url(input: &str) -> Result<NormalizedUrl, UrlPolicyError> {
    if input.is_empty() || input.len() > MAX_URL_BYTES || input.chars().any(char::is_control) {
        return Err(UrlPolicyError::InvalidUrl);
    }
    let mut url = Url::parse(input).map_err(|_| UrlPolicyError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(UrlPolicyError::UnsupportedScheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(UrlPolicyError::Credentials);
    }
    if url.port().is_some() {
        return Err(UrlPolicyError::NonDefaultPort);
    }
    let host = match url.host().ok_or(UrlPolicyError::InvalidHost)? {
        Host::Domain(host) => host.to_owned(),
        Host::Ipv4(_) | Host::Ipv6(_) => return Err(UrlPolicyError::IpLiteral),
    };
    url.set_fragment(None);
    let canonical = url.to_string();
    if canonical.len() > MAX_URL_BYTES {
        return Err(UrlPolicyError::InvalidUrl);
    }
    Ok(NormalizedUrl { canonical, host })
}

/// Normalize an allowlist entry. Entries are hostnames, not URLs, wildcard
/// expressions, IP addresses, or host-and-port pairs.
pub fn normalize_host(input: &str) -> Result<String, UrlPolicyError> {
    if input.is_empty()
        || input.len() > 253
        || input.chars().any(char::is_control)
        || input.contains(['/', '@', ':', '?', '#', ' ', '*', '%'])
    {
        return Err(UrlPolicyError::InvalidHost);
    }
    let url = Url::parse(&format!("https://{input}/")).map_err(|_| UrlPolicyError::InvalidHost)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(UrlPolicyError::InvalidHost);
    }
    match url.host().ok_or(UrlPolicyError::InvalidHost)? {
        Host::Domain(host)
            if host.len() <= 253
                && !host.ends_with('.')
                && host.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                }) =>
        {
            Ok(host.to_owned())
        }
        Host::Domain(_) | Host::Ipv4(_) | Host::Ipv6(_) => Err(UrlPolicyError::InvalidHost),
    }
}

/// Reject addresses which are not suitable destinations for an Internet
/// crawler. The caller must validate every DNS answer and connect to one of
/// these already-validated answers rather than resolving the name again.
pub fn is_public_destination(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            const DENIED: &[&str] = &[
                "0.0.0.0/8",
                "10.0.0.0/8",
                "100.64.0.0/10",
                "127.0.0.0/8",
                "169.254.0.0/16",
                "172.16.0.0/12",
                "192.0.0.0/24",
                "192.0.2.0/24",
                "192.88.99.0/24",
                "192.168.0.0/16",
                "198.18.0.0/15",
                "198.51.100.0/24",
                "203.0.113.0/24",
                "224.0.0.0/4",
                "240.0.0.0/4",
            ];
            !DENIED.iter().any(|cidr| contains(cidr, ip.into()))
        }
        IpAddr::V6(ip) => {
            const ALLOWED_GLOBAL_UNICAST: &str = "2000::/3";
            const DENIED: &[&str] = &[
                "2001::/23",
                "2001:2::/48",
                "2001:db8::/32",
                "2002::/16",
                "3fff::/20",
            ];
            contains(ALLOWED_GLOBAL_UNICAST, ip.into())
                && !DENIED.iter().any(|cidr| contains(cidr, ip.into()))
        }
    }
}

fn contains(cidr: &str, ip: IpAddr) -> bool {
    cidr.parse::<IpNet>()
        .is_ok_and(|network| network.contains(&ip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn canonicalizes_host_default_port_path_and_fragment() {
        let normalized = normalize_url("HTTPS://ExAmPlE.org:443/a/../b#section").unwrap();
        assert_eq!(normalized.canonical, "https://example.org/b");
        assert_eq!(normalized.host, "example.org");
    }

    #[test]
    fn rejects_unsafe_or_unsupported_url_authorities() {
        assert_eq!(
            normalize_url("file:///etc/passwd"),
            Err(UrlPolicyError::UnsupportedScheme)
        );
        assert_eq!(
            normalize_url("https://user:secret@example.org/"),
            Err(UrlPolicyError::Credentials)
        );
        assert_eq!(
            normalize_url("https://example.org:8443/"),
            Err(UrlPolicyError::NonDefaultPort)
        );
        assert_eq!(
            normalize_url("http://127.0.0.1/"),
            Err(UrlPolicyError::IpLiteral)
        );
    }

    #[test]
    fn normalizes_idna_hosts_and_rejects_wildcards_ports_and_ip_literals() {
        assert_eq!(
            normalize_host("BÜCHER.example").unwrap(),
            "xn--bcher-kva.example"
        );
        assert!(normalize_host("*.example.org").is_err());
        assert!(normalize_host("example.org:443").is_err());
        assert!(normalize_host("192.168.1.2").is_err());
    }

    #[test]
    fn drops_fragments_but_preserves_queries() {
        let normalized = normalize_url("https://example.org/a?x=1#first").unwrap();
        assert_eq!(normalized.canonical, "https://example.org/a?x=1");
    }

    #[test]
    fn classifies_private_special_and_public_addresses() {
        for address in [
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            "10.1.2.3".parse().unwrap(),
            "169.254.169.254".parse().unwrap(),
            "100.64.0.1".parse().unwrap(),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            "fc00::1".parse().unwrap(),
            "fe80::1".parse().unwrap(),
            "2001:db8::1".parse().unwrap(),
        ] {
            assert!(!is_public_destination(address), "{address}");
        }
        assert!(is_public_destination("8.8.8.8".parse().unwrap()));
        assert!(is_public_destination(
            "2606:4700:4700::1111".parse().unwrap()
        ));
    }
}

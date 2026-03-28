use anyhow::{bail, Result};

/// Supported proxy protocols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Socks4,
    Socks4a,
    Socks5,
}

impl Protocol {
    /// Parse a protocol string (case-insensitive).
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "http" | "https" => Ok(Protocol::Http),
            "socks4" => Ok(Protocol::Socks4),
            "socks4a" => Ok(Protocol::Socks4a),
            "socks5" | "socks5h" => Ok(Protocol::Socks5),
            other => bail!("Unknown protocol: {}", other),
        }
    }

    pub fn scheme(&self) -> &'static str {
        match self {
            Protocol::Http => "http",
            Protocol::Socks4 => "socks4",
            Protocol::Socks4a => "socks4a",
            Protocol::Socks5 => "socks5",
        }
    }
}

/// A fully parsed proxy entry.
#[derive(Debug, Clone)]
pub struct Proxy {
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl Proxy {
    /// Build the proxy URL for use with reqwest.
    pub fn to_url(&self) -> String {
        match (&self.username, &self.password) {
            (Some(u), Some(p)) => format!(
                "{}://{}:{}@{}:{}",
                self.protocol.scheme(),
                u,
                p,
                self.host,
                self.port
            ),
            _ => format!("{}://{}:{}", self.protocol.scheme(), self.host, self.port),
        }
    }

}

/// Parse a single proxy line, optionally falling back to `default_protocol`.
///
/// Supported formats (with or without a scheme prefix):
///   - `host:port`
///   - `host:port:user:pass`
///   - `host:port@user:pass`
///   - `user:pass:host:port`
///   - `user:pass@host:port`
///   - `scheme://host:port`
///   - `scheme://host:port:user:pass`
///   - `scheme://host:port@user:pass`
///   - `scheme://user:pass:host:port`
///   - `scheme://user:pass@host:port`
pub fn parse_proxy(line: &str, default_protocol: Option<&Protocol>) -> Result<Proxy> {
    let line = line.trim();
    if line.is_empty() {
        bail!("Empty proxy line");
    }

    // Split off scheme if present.
    let (protocol_opt, rest) = if let Some(pos) = line.find("://") {
        let scheme = &line[..pos];
        let rest = &line[pos + 3..];
        (Some(Protocol::from_str(scheme)?), rest)
    } else {
        (None, line)
    };

    let protocol = match (protocol_opt, default_protocol) {
        (Some(p), _) => p,
        (None, Some(p)) => p.clone(),
        (None, None) => bail!(
            "No protocol in '{}' and no --protocol flag specified",
            line
        ),
    };

    // Now parse the rest: various colon/at-sign separated formats.
    parse_address(rest, protocol)
}

/// Parse the address portion (after the scheme has been stripped) into host, port,
/// and optional credentials.
///
/// We need to handle these variants:
///   1. `host:port`
///   2. `host:port:user:pass`
///   3. `host:port@user:pass`
///   4. `user:pass:host:port`
///   5. `user:pass@host:port`
fn parse_address(s: &str, protocol: Protocol) -> Result<Proxy> {
    // Format 3: `host:port@user:pass`
    // Format 5: `user:pass@host:port`
    if let Some(at_pos) = s.find('@') {
        let before = &s[..at_pos];
        let after = &s[at_pos + 1..];

        // Try "before" as host:port, "after" as user:pass (format 3).
        if let Ok((host, port)) = split_host_port(before) {
            if let Some((user, pass)) = split_two(after) {
                return Ok(Proxy {
                    protocol,
                    host,
                    port,
                    username: Some(user),
                    password: Some(pass),
                });
            }
        }

        // Try "before" as user:pass, "after" as host:port (format 5).
        if let Some((user, pass)) = split_two(before) {
            if let Ok((host, port)) = split_host_port(after) {
                return Ok(Proxy {
                    protocol,
                    host,
                    port,
                    username: Some(user),
                    password: Some(pass),
                });
            }
        }

        bail!("Cannot parse proxy with '@': {}", s);
    }

    // No '@' — split on colons.
    let parts: Vec<&str> = s.splitn(5, ':').collect();
    match parts.len() {
        // `host:port`
        2 => {
            let (host, port) = split_host_port(s)?;
            Ok(Proxy {
                protocol,
                host,
                port,
                username: None,
                password: None,
            })
        }
        // `host:port:user:pass` OR `user:pass:host:port`
        4 => {
            // Try host:port:user:pass
            if let Ok(port) = parts[1].parse::<u16>() {
                return Ok(Proxy {
                    protocol,
                    host: parts[0].to_string(),
                    port,
                    username: Some(parts[2].to_string()),
                    password: Some(parts[3].to_string()),
                });
            }
            // Try user:pass:host:port
            if let Ok(port) = parts[3].parse::<u16>() {
                return Ok(Proxy {
                    protocol,
                    host: parts[2].to_string(),
                    port,
                    username: Some(parts[0].to_string()),
                    password: Some(parts[1].to_string()),
                });
            }
            bail!("Cannot parse 4-part proxy: {}", s);
        }
        _ => bail!("Unexpected proxy format: {}", s),
    }
}

/// Split "host:port" returning (host, port).
fn split_host_port(s: &str) -> Result<(String, u16)> {
    // Handle IPv6 addresses like [::1]:1080
    if s.starts_with('[') {
        if let Some(bracket_end) = s.find(']') {
            let host = s[1..bracket_end].to_string();
            let rest = &s[bracket_end + 1..];
            if let Some(port_str) = rest.strip_prefix(':') {
                let port = port_str
                    .parse::<u16>()
                    .map_err(|_| anyhow::anyhow!("Invalid port: {}", port_str))?;
                return Ok((host, port));
            }
        }
        bail!("Invalid IPv6 address format: {}", s);
    }

    let mut parts = s.rsplitn(2, ':');
    let port_str = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing port in: {}", s))?;
    let host = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing host in: {}", s))?
        .to_string();
    let port = port_str
        .parse::<u16>()
        .map_err(|_| anyhow::anyhow!("Invalid port '{}' in: {}", port_str, s))?;
    Ok((host, port))
}

/// Split "a:b" into ("a", "b"). Returns None if there is not exactly one colon.
fn split_two(s: &str) -> Option<(String, String)> {
    let mut parts = s.splitn(2, ':');
    let a = parts.next()?.to_string();
    let b = parts.next()?.to_string();
    if b.contains(':') {
        None // more than one colon — ambiguous
    } else {
        Some((a, b))
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn http() -> Option<Protocol> {
        Some(Protocol::Http)
    }
    fn s5() -> Option<Protocol> {
        Some(Protocol::Socks5)
    }

    #[test]
    fn test_scheme_host_port() {
        let p = parse_proxy("socks5://1.2.3.4:1080", None).unwrap();
        assert_eq!(p.protocol, Protocol::Socks5);
        assert_eq!(p.host, "1.2.3.4");
        assert_eq!(p.port, 1080);
        assert!(p.username.is_none());
    }

    #[test]
    fn test_host_port_with_default() {
        let p = parse_proxy("1.2.3.4:8080", s5().as_ref()).unwrap();
        assert_eq!(p.protocol, Protocol::Socks5);
        assert_eq!(p.host, "1.2.3.4");
        assert_eq!(p.port, 8080);
    }

    #[test]
    fn test_scheme_host_port_user_pass_colon() {
        let p = parse_proxy("socks5://1.2.3.4:1080:user:pass", None).unwrap();
        assert_eq!(p.host, "1.2.3.4");
        assert_eq!(p.port, 1080);
        assert_eq!(p.username.as_deref(), Some("user"));
        assert_eq!(p.password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_scheme_host_port_at_user_pass() {
        let p = parse_proxy("socks5://1.2.3.4:1080@user:pass", None).unwrap();
        assert_eq!(p.host, "1.2.3.4");
        assert_eq!(p.port, 1080);
        assert_eq!(p.username.as_deref(), Some("user"));
        assert_eq!(p.password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_scheme_user_pass_colon_host_port() {
        let p = parse_proxy("socks5://user:pass:1.2.3.4:1080", None).unwrap();
        assert_eq!(p.host, "1.2.3.4");
        assert_eq!(p.port, 1080);
        assert_eq!(p.username.as_deref(), Some("user"));
        assert_eq!(p.password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_scheme_user_pass_at_host_port() {
        let p = parse_proxy("socks5://user:pass@1.2.3.4:1080", None).unwrap();
        assert_eq!(p.host, "1.2.3.4");
        assert_eq!(p.port, 1080);
        assert_eq!(p.username.as_deref(), Some("user"));
        assert_eq!(p.password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_no_protocol_no_default_fails() {
        assert!(parse_proxy("1.2.3.4:1080", None).is_err());
    }

    #[test]
    fn test_http_protocol() {
        let p = parse_proxy("http://1.2.3.4:3128", None).unwrap();
        assert_eq!(p.protocol, Protocol::Http);
    }

    #[test]
    fn test_socks4() {
        let p = parse_proxy("socks4://1.2.3.4:1080", None).unwrap();
        assert_eq!(p.protocol, Protocol::Socks4);
    }

    #[test]
    fn test_to_url_no_creds() {
        let p = parse_proxy("socks5://1.2.3.4:1080", None).unwrap();
        assert_eq!(p.to_url(), "socks5://1.2.3.4:1080");
    }

    #[test]
    fn test_to_url_with_creds() {
        let p = parse_proxy("socks5://user:pass@1.2.3.4:1080", None).unwrap();
        assert_eq!(p.to_url(), "socks5://user:pass@1.2.3.4:1080");
    }

    #[test]
    fn test_no_creds_with_default_http() {
        let p = parse_proxy("192.168.1.1:3128", http().as_ref()).unwrap();
        assert_eq!(p.protocol, Protocol::Http);
        assert_eq!(p.host, "192.168.1.1");
        assert_eq!(p.port, 3128);
    }
}

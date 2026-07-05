use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc;
use std::time::Duration;

use crate::error::{Result, SpoutError};

pub const HTTP_TIMEOUT_SECS: u64 = 30;
pub const DNS_TIMEOUT_MS: u64 = 1500;
pub const MAX_RESPONSE: u64 = 10 * 1024 * 1024;
pub const MAX_URL_LEN: usize = 2048;

pub fn uri_encode(s: &str) -> String {
    use std::fmt::Write;

    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{:02X}", b);
            }
        }
    }
    out
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127
                || o[0] == 10
                || o[0] == 0
                || (o[0] == 169 && o[1] == 254)
                || (o[0] == 172 && (16..=31).contains(&o[1]))
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 100 && (64..=127).contains(&o[1]))
        }
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            v6.is_loopback() || (segs[0] & 0xfe00) == 0xfc00 || (segs[0] & 0xffc0) == 0xfe80
        }
    }
}

pub fn resolve_url(raw: &str) -> Result<(reqwest::Url, SocketAddr)> {
    let url = parse_http_url(raw)?;

    let host = url.host_str().ok_or(SpoutError::NoHost)?;
    let port = url.port_or_known_default().ok_or(SpoutError::NoPort)?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(SpoutError::PrivateIp);
        }
        return Ok((url, SocketAddr::new(ip, port)));
    }

    let host_port = format!("{}:{}", host, port);
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = host_port.to_socket_addrs();
        let _ = tx.send(result);
    });

    let addrs: Vec<SocketAddr> = rx
        .recv_timeout(Duration::from_millis(DNS_TIMEOUT_MS))
        .map_err(|_| SpoutError::DnsTimeout)?
        .map_err(SpoutError::DnsResolution)?
        .collect();

    if addrs.is_empty() {
        return Err(SpoutError::NoAddresses);
    }

    for addr in &addrs {
        if is_private_ip(addr.ip()) {
            return Err(SpoutError::PrivateIp);
        }
    }

    Ok((url, addrs[0]))
}

pub fn parse_http_url(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw).map_err(|e| SpoutError::InvalidUrl(e.to_string()))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        s => Err(SpoutError::UnsupportedScheme(s.to_string())),
    }
}

pub fn extract_response_value(body: &str, path: &str) -> Result<String> {
    if path == "." {
        return Ok(body.trim().to_string());
    }

    let json: serde_json::Value = serde_json::from_str(body)?;

    let node = path
        .split('.')
        .try_fold(&json, |cur, key| {
            cur.get(key)
                .or_else(|| key.parse::<usize>().ok().and_then(|i| cur.get(i)))
                .ok_or(key)
        })
        .map_err(|k| SpoutError::KeyNotFound(k.to_string()))?;

    node.as_str()
        .map(str::to_string)
        .ok_or_else(|| SpoutError::NotAString(path.to_string()))
}

pub fn validate_response_url(value: &str) -> Result<()> {
    if value.len() > MAX_URL_LEN {
        return Err(SpoutError::ResponseTooLarge);
    }
    let url =
        reqwest::Url::parse(value).map_err(|e| SpoutError::ResponseInvalidUrl(e.to_string()))?;
    match url.scheme() {
        "http" | "https" => Ok(()),
        s => Err(SpoutError::ResponseUnexpectedScheme(s.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_response_value, resolve_url, uri_encode, validate_response_url};

    #[test]
    fn encodes_filename_for_url_template() {
        assert_eq!(uri_encode("a b+c.png"), "a%20b%2Bc.png");
    }

    #[test]
    fn rejects_private_upload_targets() {
        let err = resolve_url("https://127.0.0.1/upload").unwrap_err();
        assert_eq!(err.to_string(), "url resolves to a private ip address");
    }

    #[test]
    fn validates_response_url_scheme_and_size() {
        validate_response_url("https://example.com/image.png").unwrap();

        let err = validate_response_url("file:///tmp/image.png").unwrap_err();
        assert_eq!(err.to_string(), "unexpected scheme in response url: file");

        let too_long = format!("https://example.com/{}", "a".repeat(2048));
        let err = validate_response_url(&too_long).unwrap_err();
        assert_eq!(
            err.to_string(),
            "response body is too large to be a valid url"
        );
    }

    #[test]
    fn extracts_plain_and_json_response_urls() {
        assert_eq!(
            extract_response_value(" https://example.com/a.png\n", ".").unwrap(),
            "https://example.com/a.png"
        );
        assert_eq!(
            extract_response_value(r#"{"data":{"links":["https://e.test/a"]}}"#, "data.links.0")
                .unwrap(),
            "https://e.test/a"
        );
    }
}

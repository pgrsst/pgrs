use crate::core::enums::tls_mode::TlsMode;

#[derive(Debug, Default)]
pub struct ParsedUrl {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
}

pub fn optional_option(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix(key).and_then(|r| r.strip_prefix('=')).map(ToString::to_string))
}

pub fn parse_tls_mode(value: &str) -> Result<TlsMode, String> {
    match value {
        "disable" => Ok(TlsMode::Disable),
        "require" => Ok(TlsMode::Require),
        "verify-full" => Ok(TlsMode::VerifyFull),
        other => Err(format!(
            "unknown tls mode '{other}' — supported: disable, require, verify-full"
        )),
    }
}

// Individual CLI flags take precedence over URL-parsed values.
pub fn parse_connection_url(url: &str) -> Result<ParsedUrl, String> {
    let rest = url
        .strip_prefix("postgresql://")
        .or_else(|| url.strip_prefix("postgres://"))
        .ok_or_else(|| {
            format!("URL must start with postgresql:// or postgres://, got '{}'", url)
        })?;

    let (userinfo_str, hostinfo) = if let Some(at) = rest.rfind('@') {
        (Some(&rest[..at]), &rest[at + 1..])
    } else {
        (None, rest)
    };

    let (username, password) = match userinfo_str {
        Some(ui) => {
            if let Some(colon) = ui.find(':') {
                (
                    Some(percent_decode(&ui[..colon])),
                    Some(percent_decode(&ui[colon + 1..])),
                )
            } else {
                (Some(percent_decode(ui)), None)
            }
        }
        None => (None, None),
    };

    let (hostport, database) = if let Some(slash) = hostinfo.find('/') {
        let db = &hostinfo[slash + 1..];
        (
            &hostinfo[..slash],
            if db.is_empty() { None } else { Some(percent_decode(db)) },
        )
    } else {
        (hostinfo, None)
    };

    let (host, port) = if hostport.starts_with('[') {
        // IPv6 bracket notation: [::1] or [::1]:5432
        let bracket_end = hostport
            .find(']')
            .ok_or_else(|| format!("unclosed '[' in URL host '{}'", hostport))?;
        let h = &hostport[1..bracket_end];
        let rest = &hostport[bracket_end + 1..];
        let port = if let Some(port_str) = rest.strip_prefix(':') {
            let p = port_str
                .parse::<u16>()
                .map_err(|_| format!("invalid port '{}' in URL", port_str))?;
            Some(p)
        } else if rest.is_empty() {
            None
        } else {
            return Err(format!("unexpected content after IPv6 address: '{}'", rest));
        };
        (if h.is_empty() { None } else { Some(h.to_string()) }, port)
    } else if let Some(colon) = hostport.rfind(':') {
        let h = &hostport[..colon];
        let p_str = &hostport[colon + 1..];
        let p = p_str
            .parse::<u16>()
            .map_err(|_| format!("invalid port '{}' in URL", p_str))?;
        (if h.is_empty() { None } else { Some(h.to_string()) }, Some(p))
    } else {
        (
            if hostport.is_empty() { None } else { Some(hostport.to_string()) },
            None,
        )
    };

    Ok(ParsedUrl { host, port, username, password, database })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            decoded.push((hi * 16 + lo) as u8);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|e| {
        String::from_utf8_lossy(e.as_bytes()).into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::enums::tls_mode::TlsMode;

    #[test]
    fn parse_url_full_postgresql_scheme() {
        let parsed =
            parse_connection_url("postgresql://user:pass@localhost:5432/mydb").unwrap();
        assert_eq!(parsed.host, Some("localhost".to_string()));
        assert_eq!(parsed.port, Some(5432));
        assert_eq!(parsed.username, Some("user".to_string()));
        assert_eq!(parsed.password, Some("pass".to_string()));
        assert_eq!(parsed.database, Some("mydb".to_string()));
    }

    #[test]
    fn parse_url_postgres_scheme() {
        let parsed = parse_connection_url("postgres://user:pass@localhost/db").unwrap();
        assert_eq!(parsed.host, Some("localhost".to_string()));
        assert_eq!(parsed.username, Some("user".to_string()));
        assert_eq!(parsed.database, Some("db".to_string()));
    }

    #[test]
    fn parse_url_without_port_returns_none() {
        let parsed = parse_connection_url("postgresql://user:pass@localhost/db").unwrap();
        assert!(parsed.port.is_none());
    }

    #[test]
    fn parse_url_ipv6_with_port() {
        let parsed = parse_connection_url("postgresql://user:pass@[::1]:5432/mydb").unwrap();
        assert_eq!(parsed.host, Some("::1".to_string()));
        assert_eq!(parsed.port, Some(5432));
        assert_eq!(parsed.database, Some("mydb".to_string()));
    }

    #[test]
    fn parse_url_ipv6_without_port() {
        let parsed = parse_connection_url("postgresql://user:pass@[::1]/mydb").unwrap();
        assert_eq!(parsed.host, Some("::1".to_string()));
        assert!(parsed.port.is_none());
    }

    #[test]
    fn parse_url_ipv6_full_address() {
        let parsed = parse_connection_url("postgresql://user:pass@[2001:db8::1]:5433/db").unwrap();
        assert_eq!(parsed.host, Some("2001:db8::1".to_string()));
        assert_eq!(parsed.port, Some(5433));
    }

    #[test]
    fn parse_url_invalid_scheme_returns_error() {
        let result = parse_connection_url("mysql://user:pass@host/db");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("postgresql://"));
    }

    #[test]
    fn parse_url_invalid_port_returns_error() {
        let result = parse_connection_url("postgresql://user:pass@host:abc/db");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("port"));
    }

    #[test]
    fn parse_url_decodes_percent_encoded_password() {
        let parsed =
            parse_connection_url("postgresql://user:p%40ss%23word@localhost/db").unwrap();
        assert_eq!(parsed.password, Some("p@ss#word".to_string()));
    }

    #[test]
    fn parse_url_decodes_percent_encoded_username() {
        let parsed =
            parse_connection_url("postgresql://admin%40corp:pass@localhost/db").unwrap();
        assert_eq!(parsed.username, Some("admin@corp".to_string()));
    }

    #[test]
    fn parse_url_decodes_percent_encoded_database() {
        let parsed =
            parse_connection_url("postgresql://user:pass@localhost/my%20db").unwrap();
        assert_eq!(parsed.database, Some("my db".to_string()));
    }

    #[test]
    fn parse_tls_mode_disable_returns_disable() {
        assert_eq!(parse_tls_mode("disable"), Ok(TlsMode::Disable));
    }

    #[test]
    fn parse_tls_mode_require_returns_require() {
        assert_eq!(parse_tls_mode("require"), Ok(TlsMode::Require));
    }

    #[test]
    fn parse_tls_mode_verify_full_returns_verify_full() {
        assert_eq!(parse_tls_mode("verify-full"), Ok(TlsMode::VerifyFull));
    }

    #[test]
    fn parse_tls_mode_unknown_returns_error_mentioning_value() {
        let err = parse_tls_mode("starttls").unwrap_err();
        assert!(err.contains("starttls"), "got: {err}");
    }

    #[test]
    fn percent_decode_handles_uppercase_hex() {
        assert_eq!(percent_decode("hello%2Fworld"), "hello/world");
    }

    #[test]
    fn percent_decode_leaves_plain_text_unchanged() {
        assert_eq!(percent_decode("plaintext"), "plaintext");
    }

    #[test]
    fn percent_decode_multibyte_utf8_accent() {
        // %C3%A9 is the UTF-8 encoding of é (U+00E9)
        assert_eq!(percent_decode("%C3%A9"), "é");
    }

    #[test]
    fn percent_decode_multibyte_utf8_in_password() {
        // postgresql://user:caf%C3%A9@host/db — password should decode to "café"
        let parsed = parse_connection_url("postgresql://user:caf%C3%A9@host/db").unwrap();
        assert_eq!(parsed.password, Some("café".to_string()));
    }

    #[test]
    fn percent_decode_multibyte_three_byte_sequence() {
        // %E2%82%AC is the UTF-8 encoding of € (U+20AC)
        assert_eq!(percent_decode("%E2%82%AC"), "€");
    }
}

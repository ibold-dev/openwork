//! Session tokens and cookies.
//!
//! The raw token is 32 random bytes encoded as hex and only ever exists in the
//! client cookie. The database stores its SHA-256, so a database leak does not
//! yield usable sessions.

use axum::http::HeaderMap;
use rand::RngCore;

use crate::ctx::header_string;

pub const SESSION_COOKIE: &str = "openwork_session";

/// Generates a 64 character hex token from 32 bytes of OS randomness.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    super::siws::sha256_hex(token)
}

pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = header_string(headers, "cookie")?;
    parse_cookie(&cookie, SESSION_COOKIE)
}

/// Parses a single cookie value out of a `Cookie` header.
pub fn parse_cookie(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key.trim() == name {
            let value = value.trim().trim_matches('"');
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        } else {
            None
        }
    })
}

/// `Set-Cookie` value for a fresh session.
pub fn session_cookie(token: &str, max_age_seconds: i64, secure: bool) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age_seconds}"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// `Set-Cookie` value that deletes the session cookie.
pub fn clear_session_cookie(secure: bool) -> String {
    let mut cookie = format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_32_random_bytes_hex() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert_eq!(hash_token(&a).len(), 64);
        assert_ne!(hash_token(&a), a);
    }

    #[test]
    fn parses_cookies() {
        let header = format!("theme=dark; {SESSION_COOKIE}=abc123; other=1");
        assert_eq!(
            parse_cookie(&header, SESSION_COOKIE),
            Some("abc123".to_string())
        );
        assert_eq!(parse_cookie(&header, "theme"), Some("dark".to_string()));
        assert_eq!(parse_cookie(&header, "missing"), None);
        assert_eq!(
            parse_cookie(&format!("{SESSION_COOKIE}="), SESSION_COOKIE),
            None
        );
    }

    #[test]
    fn cookie_flags_are_safe() {
        let cookie = session_cookie("token", 3600, true);
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("Max-Age=3600"));

        let cookie = session_cookie("token", 3600, false);
        assert!(!cookie.contains("Secure"));

        let cleared = clear_session_cookie(true);
        assert!(cleared.contains("Max-Age=0"));
        assert!(cleared.contains("Secure"));
    }
}

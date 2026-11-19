//! Removing secrets from text before it is logged (Phase 18).
//!
//! Oxide never logs keys on purpose, but log lines also carry text it doesn't control: MCP
//! servers' stderr, provider error bodies, command lines, URLs. [`redact_secrets`] replaces
//! what looks like a credential with `[redacted]`:
//! - API keys with a well-known prefix (`sk-…`, `sk-ant-…`, `gsk_…`, `AIza…`, `ghp_…`, …);
//! - `Bearer …` / `Basic …` credentials;
//! - JSON web tokens (`eyJ….….…`);
//! - values of keys whose name says they are secret (`api_key=…`, `"token": "…"`,
//!   `Authorization: …`, `?access_token=…`, `PASSWORD=…`).
//!
//! It is a safety net, not a parser: it errs on the side of hiding too much.

use std::borrow::Cow;

/// What replaces a secret.
pub const REDACTED: &str = "[redacted]";

/// Prefixes of API keys and tokens, longest first where one starts another.
const KEY_PREFIXES: &[&str] = &[
    "sk-ant-",
    "sk-proj-",
    "sk-or-",
    "sk-",
    "gsk_",
    "xai-",
    "AIza",
    "github_pat_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "glpat-",
    "hf_",
    "xoxb-",
    "xoxp-",
    "xapp-",
    "tvly-",
    "pplx-",
    "r8_",
    "nvapi-",
];

/// Characters after a key prefix needed before it counts as a key.
const MIN_KEY_BODY: usize = 16;

/// Words that make a `name=value` / `"name": "value"` pair secret (matched in lower case,
/// with `-` and `_` removed).
const SECRET_NAMES: &[&str] = &[
    "apikey",
    "xapikey",
    "token",
    "accesstoken",
    "refreshtoken",
    "idtoken",
    "authtoken",
    "sessiontoken",
    "bearertoken",
    "secret",
    "clientsecret",
    "password",
    "passwd",
    "pwd",
    "authorization",
    "proxyauthorization",
    "cookie",
    "setcookie",
    "privatekey",
    "signingkey",
    "credentials",
];

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn is_b64url(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'='
}

/// Returns `text` with anything that looks like a secret replaced by `[redacted]`.
/// Text without secrets is returned borrowed.
pub fn redact_secrets(text: &str) -> Cow<'_, str> {
    let pass1 = redact_tokens(text);
    match redact_named(&pass1) {
        Cow::Borrowed(_) => pass1,
        Cow::Owned(s) => Cow::Owned(s),
    }
}

/// Known key prefixes, `Bearer`/`Basic` credentials and JWTs.
fn redact_tokens(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        // Only look at the start of a word.
        let at_word_start = i == 0 || !is_token_char(bytes[i - 1]);
        if !at_word_start || !bytes[i].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        let mut hit: Option<(usize, usize)> = None; // (keep up to, end)

        // `Bearer xyz` / `Basic xyz`.
        for scheme in ["bearer ", "basic "] {
            let n = scheme.len();
            if bytes.len() >= i + n && bytes[i..i + n].eq_ignore_ascii_case(scheme.as_bytes()) {
                let start = i + n;
                let mut end = start;
                while end < bytes.len()
                    && (is_b64url(bytes[end]) || matches!(bytes[end], b'.' | b'+' | b'/' | b'~'))
                {
                    end += 1;
                }
                if end - start >= 8 {
                    hit = Some((start, end));
                }
                break;
            }
        }

        // JWT: three base64url parts, the first starting with `eyJ`.
        if hit.is_none() && bytes[i..].starts_with(b"eyJ") {
            let mut end = i;
            let mut dots = 0;
            while end < bytes.len() && (is_b64url(bytes[end]) || bytes[end] == b'.') {
                if bytes[end] == b'.' {
                    dots += 1;
                }
                end += 1;
            }
            if dots >= 2 && end - i >= 30 {
                hit = Some((i, end));
            }
        }

        // API keys with a known prefix.
        if hit.is_none() {
            for prefix in KEY_PREFIXES {
                if bytes[i..].starts_with(prefix.as_bytes()) {
                    let start = i + prefix.len();
                    let mut end = start;
                    while end < bytes.len() && is_token_char(bytes[end]) {
                        end += 1;
                    }
                    if end - start >= MIN_KEY_BODY {
                        hit = Some((start, end));
                    }
                    break;
                }
            }
        }

        match hit {
            Some((keep, end)) => {
                let o = out.get_or_insert_with(|| String::with_capacity(text.len()));
                o.push_str(&text[copied..keep]);
                o.push_str(REDACTED);
                copied = end;
                i = end;
            }
            None => {
                // Skip the rest of this word.
                while i < bytes.len() && is_token_char(bytes[i]) {
                    i += 1;
                }
            }
        }
    }
    match out {
        None => Cow::Borrowed(text),
        Some(mut o) => {
            o.push_str(&text[copied..]);
            Cow::Owned(o)
        }
    }
}

fn is_secret_name(name: &str) -> bool {
    let norm: String = name
        .chars()
        .filter(|c| *c != '-' && *c != '_' && *c != '.')
        .flat_map(char::to_lowercase)
        .collect();
    if norm.is_empty() {
        return false;
    }
    SECRET_NAMES
        .iter()
        .any(|s| norm == *s || (norm.ends_with(s) && s.len() >= 5))
}

/// `name=value`, `name: value`, `"name": "value"` with a secret-sounding name.
fn redact_named(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b != b'=' && b != b':' {
            i += 1;
            continue;
        }
        // The name before the separator: optional closing quote, then name characters.
        let mut name_end = i;
        while name_end > 0 && bytes[name_end - 1] == b' ' {
            name_end -= 1;
        }
        let quoted_name = name_end > 0 && matches!(bytes[name_end - 1], b'"' | b'\'');
        if quoted_name {
            name_end -= 1;
        }
        let mut name_start = name_end;
        while name_start > 0
            && (is_token_char(bytes[name_start - 1]) || bytes[name_start - 1] == b'.')
        {
            name_start -= 1;
        }
        let name = &text[name_start..name_end];
        // `https://` and `C:\` are not pairs.
        let is_url_scheme = b == b':' && bytes.get(i + 1) == Some(&b'/');
        if name.is_empty() || is_url_scheme || !is_secret_name(name) {
            i += 1;
            continue;
        }
        // The value after the separator.
        let mut start = i + 1;
        while start < bytes.len() && bytes[start] == b' ' {
            start += 1;
        }
        if start >= bytes.len() {
            break;
        }
        let (vstart, vend) = match bytes[start] {
            q @ (b'"' | b'\'') => {
                let mut end = start + 1;
                while end < bytes.len() && bytes[end] != q {
                    if bytes[end] == b'\\' {
                        end += 1;
                    }
                    end += 1;
                }
                (start + 1, end.min(bytes.len()))
            }
            _ => {
                let mut end = start;
                // `Authorization: Bearer xyz` keeps going over one space after a scheme word.
                while end < bytes.len()
                    && !matches!(
                        bytes[end],
                        b' ' | b'\t' | b'\r' | b'\n' | b'&' | b',' | b';' | b'}' | b']' | b'"'
                    )
                {
                    end += 1;
                }
                let word = &text[start..end];
                if (word.eq_ignore_ascii_case("bearer") || word.eq_ignore_ascii_case("basic"))
                    && end < bytes.len()
                    && bytes[end] == b' '
                {
                    end += 1;
                    while end < bytes.len()
                        && !matches!(
                            bytes[end],
                            b' ' | b'\t' | b'\r' | b'\n' | b'&' | b',' | b'"'
                        )
                    {
                        end += 1;
                    }
                }
                (start, end)
            }
        };
        let value = &text[vstart..vend];
        // Empty values, booleans, numbers that are clearly counts and already-redacted values stay.
        let harmless = value.is_empty()
            || value == REDACTED
            || value.starts_with(REDACTED)
            || value.eq_ignore_ascii_case("null")
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("false")
            || value.eq_ignore_ascii_case("none")
            || (value.len() <= 6 && value.bytes().all(|c| c.is_ascii_digit()));
        if harmless {
            i = vend.max(i + 1);
            continue;
        }
        let o = out.get_or_insert_with(|| String::with_capacity(text.len()));
        o.push_str(&text[copied..vstart]);
        o.push_str(REDACTED);
        copied = vend;
        i = vend.max(i + 1);
    }
    match out {
        None => Cow::Borrowed(text),
        Some(mut o) => {
            o.push_str(&text[copied..]);
            Cow::Owned(o)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> String {
        redact_secrets(s).into_owned()
    }

    #[test]
    fn plain_text_is_borrowed_and_unchanged() {
        let s = "MCP server filesystem started in 120 ms (pid 4411); see https://example.com/a:b";
        assert!(matches!(redact_secrets(s), Cow::Borrowed(_)));
        assert_eq!(r("C:\\Users\\info\\AppData"), "C:\\Users\\info\\AppData");
        assert_eq!(
            r("tokens: 1234, max_tokens=4096"),
            "tokens: 1234, max_tokens=4096"
        );
        assert_eq!(r("skip-this sk-short"), "skip-this sk-short");
    }

    #[test]
    fn known_key_prefixes_are_hidden() {
        assert_eq!(
            r("key sk-ant-api03-AbCdEfGhIjKlMnOpQrStUv rejected"),
            "key sk-ant-[redacted] rejected"
        );
        assert_eq!(r("gsk_0123456789abcdefABCDEF!"), "gsk_[redacted]!");
        assert_eq!(
            r("url=https://x/?key=AIzaSyA1234567890abcdefghij"),
            "url=https://x/?key=AIza[redacted]"
        );
        assert_eq!(r("ghp_abcdefghijklmnopqrstuvwxyz0123"), "ghp_[redacted]");
        // Inside another word it is not a key prefix.
        assert_eq!(r("task-1234567890abcdefghij"), "task-1234567890abcdefghij");
    }

    #[test]
    fn bearer_and_jwt_are_hidden() {
        assert_eq!(
            r("Authorization: Bearer abc.def-123_456"),
            "Authorization: [redacted]"
        );
        assert_eq!(r("sent bearer 0123456789abcdef"), "sent bearer [redacted]");
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert_eq!(r(&format!("id {jwt} end")), "id [redacted] end");
    }

    #[test]
    fn secret_named_values_are_hidden() {
        assert_eq!(r("API_KEY=hunter2hunter2 next"), "API_KEY=[redacted] next");
        assert_eq!(
            r(r#"{"access_token": "abc123xyz", "expires_in": 3600}"#),
            r#"{"access_token": "[redacted]", "expires_in": 3600}"#
        );
        assert_eq!(
            r("GET /cb?code=1&client_secret=s3cr3t&state=x"),
            "GET /cb?code=1&client_secret=[redacted]&state=x"
        );
        assert_eq!(r("x-api-key: abcdef"), "x-api-key: [redacted]");
        assert_eq!(r("PASSWORD='p@ss word'"), "PASSWORD='[redacted]'");
        assert_eq!(r("token=null"), "token=null");
        assert_eq!(r("max_tokens: 800"), "max_tokens: 800");
    }

    #[test]
    fn never_panics_on_odd_input() {
        for s in [
            "",
            "=",
            ":",
            "token=",
            "token=\"",
            "\"token\":",
            "Bearer ",
            "eyJ.",
            "sk-",
            "æøå token=æøå",
            "password: \"\\",
            "🔑 api_key=🔑🔑🔑",
        ] {
            let _ = r(s);
        }
        assert_eq!(r("æøå token=æøå"), "æøå token=[redacted]");
    }
}

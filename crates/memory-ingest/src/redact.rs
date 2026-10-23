//! Secret and sensitive-path redaction plus length limits for captured text.
//!
//! shortcut: pattern based (known token prefixes, NAME=VALUE, `Bearer`, URL passwords, PEM
//! private keys), no entropy detection, so an unknown secret format passes through; upgrade
//! when the extractor is allowed to see more than short summaries.

const MARK: &str = "[REDACTED]";
const TRUNCATED: &str = "…[truncated]";
/// Redaction scans at most this many times the output limit, so a huge payload costs little.
const SCAN_FACTOR: usize = 4;

const PREFIXES: [&str; 12] = [
    "sk-",
    "sk_live_",
    "sk_test_",
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "glpat-",
    "npm_",
];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cleaned {
    pub text: String,
    /// Reason codes: `secret`, `truncated`.
    pub reasons: Vec<&'static str>,
}

/// Redact secrets, then cut to `max_chars` (in that order, so a cut can never expose half a
/// secret that redaction would have caught).
pub fn clean(input: &str, max_chars: usize) -> Cleaned {
    let head: String = input.chars().take(max_chars * SCAN_FACTOR).collect();
    let mut reasons = Vec::new();
    let (red, n) = redact(&head);
    if n > 0 {
        reasons.push("secret");
    }
    let cut = head.len() < input.len() || red.chars().count() > max_chars;
    let text = if cut {
        reasons.push("truncated");
        red.chars()
            .take(max_chars)
            .chain(TRUNCATED.chars())
            .collect()
    } else {
        red
    };
    Cleaned { text, reasons }
}

/// Returns the text with secrets replaced and how many replacements were made.
pub fn redact(input: &str) -> (String, usize) {
    let (text, mut count) = strip_pem(input);
    let mut out = String::with_capacity(text.len());
    let (mut prev_name, mut armed) = (false, false);
    for (is_word, tok) in tokens(&text) {
        if !is_word {
            out.push_str(tok);
            continue;
        }
        let is_scheme = matches!(
            tok.to_ascii_lowercase().trim_end_matches([':', '=']),
            "bearer" | "basic"
        );
        if armed && is_scheme {
            out.push_str(tok);
            continue;
        }
        if armed && !is_sep(tok) {
            out.push_str(MARK);
            count += 1;
            armed = false;
            prev_name = false;
            continue;
        }
        if let Some(rep) = redact_inline(tok) {
            out.push_str(&rep);
            count += 1;
            prev_name = false;
            continue;
        }
        out.push_str(tok);
        if is_sep(tok) {
            armed = prev_name;
            prev_name = false;
        } else if is_scheme || (tok.ends_with([':', '=']) && sensitive_name(tok)) {
            armed = true;
        } else {
            prev_name = sensitive_name(tok);
        }
    }
    (out, count)
}

fn is_delim(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '`'
        )
}

fn is_sep(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| c == ':' || c == '=')
}

/// Alternating runs of (is_word, text): delimiter runs are kept verbatim.
fn tokens(s: &str) -> Vec<(bool, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut cur: Option<bool> = None;
    for (i, c) in s.char_indices() {
        let word = !is_delim(c);
        if cur.is_some_and(|w| w != word) {
            out.push((cur.unwrap_or(false), &s[start..i]));
            start = i;
        }
        cur = Some(word);
    }
    if let Some(w) = cur {
        out.push((w, &s[start..]));
    }
    out
}

/// `api_key`, `AWS_SECRET_ACCESS_KEY`, `password`, `authorization`: any alphanumeric segment
/// that is, or ends with, a secret-ish word.
fn sensitive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|seg| {
            matches!(
                seg,
                "auth" | "pwd" | "passwd" | "credential" | "credentials" | "authorization"
            ) || ["key", "token", "secret", "password"]
                .iter()
                .any(|s| seg.ends_with(s))
        })
}

fn redact_inline(w: &str) -> Option<String> {
    if let Some((name, val)) = w.split_once('=') {
        if !name.is_empty() && !val.is_empty() && val != MARK && sensitive_name(name) {
            return Some(format!("{name}={MARK}"));
        }
    }
    if let Some(i) = w.find("://") {
        let rest = &w[i + 3..];
        let authority = rest.split('/').next().unwrap_or("");
        if let Some((userinfo, _)) = authority.split_once('@') {
            if let Some((user, _)) = userinfo.split_once(':') {
                let tail = &rest[userinfo.len()..];
                return Some(format!("{}{user}:{MARK}{tail}", &w[..i + 3]));
            }
        }
    }
    let secret_like = if w.starts_with("AKIA") {
        w.len() == 20
            && w.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    } else if w.starts_with("eyJ") {
        w.len() >= 20 && w.matches('.').count() >= 2
    } else {
        w.len() >= 20 && PREFIXES.iter().any(|p| w.starts_with(p))
    };
    secret_like.then(|| MARK.to_string())
}

fn strip_pem(input: &str) -> (String, usize) {
    let mut text = input.to_string();
    let mut n = 0;
    while let Some(start) = text.find("-----BEGIN ") {
        let Some(hdr_end) = text[start..]
            .find("-----\n")
            .or_else(|| text[start..].find("-----"))
        else {
            break;
        };
        if !text[start..start + hdr_end + 5].contains("PRIVATE KEY") {
            // Not a private key: skip past this header so the loop terminates.
            let tail = text.split_off(start + 11);
            let (rest, k) = strip_pem(&tail);
            text.push_str(&rest);
            return (text, n + k);
        }
        let end = text[start..]
            .find("-----END ")
            .and_then(|e| {
                text[start + e + 9..]
                    .find("-----")
                    .map(|x| start + e + 9 + x + 5)
            })
            .unwrap_or(text.len());
        text.replace_range(start..end, MARK);
        n += 1;
    }
    (text, n)
}

/// True when `s` is, or mentions, a path that usually holds credentials.
pub fn mentions_sensitive_path(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    const FRAGMENTS: [&str; 10] = [
        "/.ssh/",
        "id_rsa",
        "id_ed25519",
        ".aws/credentials",
        ".npmrc",
        ".netrc",
        ".kube/config",
        ".gnupg",
        ".pem",
        ".p12",
    ];
    if FRAGMENTS.iter().any(|f| l.contains(f)) {
        return true;
    }
    // `.env`, `.env.local` but not `.environment`.
    l.match_indices(".env")
        .any(|(i, m)| !l[i + m.len()..].starts_with(|c: char| c.is_ascii_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red(s: &str) -> String {
        redact(s).0
    }

    #[test]
    fn known_token_shapes() {
        let key = "sk-abcdefghijklmnopqrstuvwxyz0123456789";
        assert_eq!(red(&format!("use {key} now")), "use [REDACTED] now");
        assert_eq!(
            red("ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
            "[REDACTED]"
        );
        assert_eq!(red("AKIAIOSFODNN7EXAMPLE"), "[REDACTED]");
        assert_eq!(
            red("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abcdefghijk"),
            "[REDACTED]"
        );
        // Too short to be a real key: left alone.
        assert_eq!(red("sk-learn is a library"), "sk-learn is a library");
    }

    #[test]
    fn name_value_forms() {
        assert_eq!(red("API_KEY=hunter2 run"), "API_KEY=[REDACTED] run");
        assert_eq!(
            red("export AWS_SECRET_ACCESS_KEY=abc"),
            "export AWS_SECRET_ACCESS_KEY=[REDACTED]"
        );
        assert_eq!(
            red(r#"{"password": "hunter2", "user": "bob"}"#),
            r#"{"password": "[REDACTED]", "user": "bob"}"#
        );
        assert_eq!(red("password: hunter2"), "password: [REDACTED]");
        assert_eq!(
            red("Authorization: Bearer abc123"),
            "Authorization: Bearer [REDACTED]"
        );
        assert_eq!(red("curl -H Bearer abc123"), "curl -H Bearer [REDACTED]");
        // Plain prose mentioning a secret word is untouched.
        assert_eq!(
            red("the token is rotated daily"),
            "the token is rotated daily"
        );
        assert_eq!(red("author=bob monkeyhead=1"), "author=bob monkeyhead=1");
    }

    #[test]
    fn url_password_and_pem() {
        assert_eq!(
            red("postgres://app:s3cret@db:5432/x"),
            "postgres://app:[REDACTED]@db:5432/x"
        );
        assert_eq!(red("https://example.com/path"), "https://example.com/path");
        let pem = "a\n-----BEGIN RSA PRIVATE KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----\nb";
        assert_eq!(red(pem), "a\n[REDACTED]\nb");
        let cert = "-----BEGIN CERTIFICATE-----\nMIIE\n-----END CERTIFICATE-----";
        assert_eq!(red(cert), cert);
    }

    #[test]
    fn clean_redacts_before_cutting_and_cuts_on_char_boundary() {
        let key = "sk-abcdefghijklmnopqrstuvwxyz0123456789";
        let c = clean(&format!("{key} {}", "x".repeat(50)), 30);
        assert!(!c.text.contains("sk-abc"));
        assert_eq!(c.reasons, vec!["secret", "truncated"]);
        let cjk = clean(&"中".repeat(100), 10);
        assert_eq!(cjk.text.chars().count(), 10 + TRUNCATED.chars().count());
        let short = clean("hello", 10);
        assert_eq!((short.text.as_str(), short.reasons.len()), ("hello", 0));
    }

    #[test]
    fn sensitive_paths() {
        for p in [
            "/home/u/.env",
            "cat .env.local",
            "~/.ssh/id_rsa",
            "x/.aws/credentials",
            "k.pem",
        ] {
            assert!(mentions_sensitive_path(p), "{p}");
        }
        for p in ["src/environment.rs", "README.md", "/home/u/project/.envrc"] {
            assert!(!mentions_sensitive_path(p), "{p}");
        }
    }
}

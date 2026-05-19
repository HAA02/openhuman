//! PII / secret redaction used by audit pipelines (PRD §2.5).
//!
//! The intent is **defense-in-depth**: even if a payload leaks through
//! upstream redaction or is logged verbatim by a caller, this final pass
//! strips the highest-risk substring patterns before they hit JSONL files.
//!
//! Patterns covered (PRD §2.5):
//!   1. Provider API keys — `sk-…`, `sk_live_…`, `ghp_…`, `AKIA…`, `xoxp-…`.
//!   2. JSON Web Tokens — `eyJ…` triple-part base64url tokens.
//!   3. Email addresses — local part preserved, host masked to `***.tld`.
//!   4. URL userinfo — password (`https://u:p@h/`) replaced with `:***@`.
//!   5. Credit-card-like PANs — 13–19 digit runs preserve only the last 4.
//!
//! Each match is replaced with a stable marker so downstream filters can
//! grep for `[REDACTED]` to confirm. The function is allocation-conscious:
//! input strings that contain none of the patterns are returned via a
//! single regex `is_match` short-circuit.

use once_cell::sync::Lazy;
use regex::Regex;

const REDACTED: &str = "[REDACTED]";

static API_KEY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        \b(
            sk-[A-Za-z0-9_-]{16,}                 # OpenAI / Anthropic-style
          | sk_live_[A-Za-z0-9]{16,}              # Stripe live secret
          | sk_test_[A-Za-z0-9]{16,}              # Stripe test secret
          | ghp_[A-Za-z0-9]{20,}                  # GitHub PAT (classic)
          | github_pat_[A-Za-z0-9_]{20,}          # GitHub PAT (fine-grained)
          | AKIA[A-Z0-9]{16}                      # AWS access key id
          | xox[pboas]-[A-Za-z0-9-]{10,}          # Slack
          | AIza[A-Za-z0-9_-]{20,}                # Google API key
        )\b
        ",
    )
    .expect("API_KEY regex compiles")
});

static JWT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b")
        .expect("JWT regex compiles")
});

static EMAIL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b([A-Za-z0-9._%+-]+)@([A-Za-z0-9.-]+)\.([A-Za-z]{2,24})\b")
        .expect("EMAIL regex compiles")
});

static URL_USERINFO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<scheme>https?|ftp|sftp|ssh|postgres|mysql|redis|amqp|amqps)://(?P<user>[^:\s/@]{1,64}):(?P<pw>[^@\s/]{1,256})@")
        .expect("URL_USERINFO regex compiles")
});

static PAN: Lazy<Regex> = Lazy::new(|| {
    // 13–19 digits possibly separated by spaces or dashes. We rely on a
    // separator-tolerant match and then collapse to digits for the trailing
    // four-digit keep.
    Regex::new(r"\b(?:\d[ -]?){12,18}\d\b").expect("PAN regex compiles")
});

/// Redact sensitive substrings in `text`. Returns the original string when
/// none of the configured patterns match (single hot-path allocation only on
/// a hit).
pub fn redact(text: &str) -> String {
    // Fast path — common case: no sensitive content. Run the cheapest pattern
    // first via `is_match` on the full set.
    if !API_KEY.is_match(text)
        && !JWT.is_match(text)
        && !EMAIL.is_match(text)
        && !URL_USERINFO.is_match(text)
        && !PAN.is_match(text)
    {
        return text.to_string();
    }

    let mut out = API_KEY.replace_all(text, REDACTED).to_string();
    out = JWT.replace_all(&out, REDACTED).to_string();
    out = URL_USERINFO
        .replace_all(&out, |caps: &regex::Captures| {
            format!("{}://{}:{REDACTED}@", &caps["scheme"], &caps["user"])
        })
        .to_string();
    out = EMAIL
        .replace_all(&out, |caps: &regex::Captures| {
            let local = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let tld = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            format!("{local}@***.{tld}")
        })
        .to_string();
    out = PAN
        .replace_all(&out, |caps: &regex::Captures| {
            let raw: String = caps[0].chars().filter(|c| c.is_ascii_digit()).collect();
            if raw.len() < 13 || raw.len() > 19 {
                return caps[0].to_string();
            }
            let tail = &raw[raw.len() - 4..];
            format!("**** **** **** {tail}")
        })
        .to_string();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_no_sensitive_content() {
        let input = "Meeting at 3pm with the team; agenda attached.";
        assert_eq!(redact(input), input);
    }

    #[test]
    fn redacts_openai_style_key() {
        let input = "token=sk-abc123DEF456ghiJKL789mnoPQR";
        let out = redact(input);
        assert!(!out.contains("sk-abc123DEF456ghiJKL789mnoPQR"), "got {out}");
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_github_pat() {
        let out = redact("Authorization: token ghp_AbCdEfGhIjKlMnOpQrStUvWxYz1234567890");
        assert!(!out.contains("ghp_AbCdEf"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_aws_access_key_id() {
        let out = redact("Caller used AKIAIOSFODNN7EXAMPLE today");
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_stripe_secret() {
        let out = redact("STRIPE_KEY=sk_live_51HabcDefGhi0lmnopQrSt");
        assert!(!out.contains("sk_live_51HabcDefGhi0lmnopQrSt"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_slack_token() {
        let out = redact("hook xoxb-12345-67890-abcDEF token");
        assert!(!out.contains("xoxb-12345-67890-abcDEF"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_google_api_key() {
        let out = redact("AIzaSyA-1234567890abcdefghijKL");
        assert!(!out.contains("AIzaSyA-1234567890abcdefghijKL"));
    }

    #[test]
    fn redacts_jwt() {
        // Three dot-separated base64url segments.
        let jwt = "eyJhbGciOiJIUzI1NiIs.eyJzdWIiOiIxMjM0NTY3.SflKxwRJSMeKKF2QT4fwpMeJf36P";
        let out = redact(&format!("Bearer {jwt} expires soon"));
        assert!(!out.contains(jwt), "got {out}");
    }

    #[test]
    fn redacts_email_keeping_local_part() {
        let out = redact("Contact me at john.doe+work@corp.example.com please.");
        assert!(out.contains("john.doe+work@***.com"), "got {out}");
        assert!(!out.contains("corp.example.com"));
    }

    #[test]
    fn redacts_url_password() {
        let out = redact("connect postgres://admin:s3cr3t@db.internal/prod");
        assert!(!out.contains("s3cr3t"), "got {out}");
        assert!(out.contains("admin:[REDACTED]@"));
    }

    #[test]
    fn redacts_credit_card_keeping_last_four() {
        let out = redact("card: 4111 1111 1111 1234 on file");
        assert!(out.contains("1234"));
        assert!(!out.contains("4111 1111 1111 1234"), "got {out}");
        assert!(out.contains("**** **** ****"));
    }

    #[test]
    fn redacts_credit_card_no_separators() {
        let out = redact("pan=4111111111111234");
        assert!(out.contains("1234"));
        assert!(!out.contains("4111111111111234"));
    }

    #[test]
    fn redacts_multiple_categories_in_one_pass() {
        let leak = "user@host.io with sk-AAAAAAAAAAAAAAAAAAAA and Bearer eyJabcDEFgh.iJKLMNOpqr.STUVwxyz0123 from postgres://u:p@h.io/db";
        let out = redact(leak);
        assert!(!out.contains("user@host.io"));
        assert!(!out.contains("sk-AAAAAAAAAAAAAAAAAAAA"));
        assert!(!out.contains("eyJabcDEFgh.iJKLMNOpqr.STUVwxyz0123"));
        assert!(!out.contains("u:p@"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn does_not_redact_short_numbers() {
        let out = redact("zip code 90210 fits a small range");
        assert_eq!(out, "zip code 90210 fits a small range");
    }

    #[test]
    fn does_not_redact_non_secret_phrases_that_mention_keys() {
        let out = redact("The user asked about how API keys work.");
        assert_eq!(out, "The user asked about how API keys work.");
    }
}

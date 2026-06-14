use std::sync::OnceLock;
use regex::Regex;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Sensitivity {
    None,
    Personal,
    Credential,
    Secret,
}

/// Sanitizes plain text by removing null bytes and control characters
/// to prevent rendering/terminal issues, while preserving standard characters.
pub fn sanitize_text(text: &str) -> String {
    text.chars()
        .filter(|&c| c == '\n' || c == '\r' || c == '\t' || !c.is_control())
        .collect()
}

/// Sanitizes an SVG string by stripping out <script>, <foreignObject>, <iframe>, 
/// <object>, <embed>, <form>, inline event handlers, and javascript: links.
pub fn sanitize_svg(svg_content: &str) -> Result<String, String> {
    // 1. Basic check if it is actually SVG
    let trimmed = svg_content.trim();
    if !trimmed.starts_with("<svg") && !trimmed.contains("<svg") {
        return Err("Not a valid SVG format".to_string());
    }

    let mut sanitized = svg_content.to_string();

    // Regex definitions (using OnceLock for performance)
    static SCRIPT_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static FOREIGN_OBJECT_RE: OnceLock<Regex> = OnceLock::new();
    static IFRAME_RE: OnceLock<Regex> = OnceLock::new();
    static OBJECT_RE: OnceLock<Regex> = OnceLock::new();
    static EMBED_RE: OnceLock<Regex> = OnceLock::new();
    static FORM_RE: OnceLock<Regex> = OnceLock::new();
    static EVENT_HANDLER_RE: OnceLock<Regex> = OnceLock::new();
    static JAVASCRIPT_URL_RE: OnceLock<Regex> = OnceLock::new();

    let script_re = SCRIPT_TAG_RE.get_or_init(|| {
        Regex::new(r"(?i)<script\b[^>]*>([\s\S]*?)</script>|<script\b[^>]*/>").unwrap()
    });
    let foreign_obj_re = FOREIGN_OBJECT_RE.get_or_init(|| {
        Regex::new(r"(?i)<foreignObject\b[^>]*>([\s\S]*?)</foreignObject>|<foreignObject\b[^>]*/>").unwrap()
    });
    let iframe_re = IFRAME_RE.get_or_init(|| {
        Regex::new(r"(?i)<iframe\b[^>]*>([\s\S]*?)</iframe>|<iframe\b[^>]*/>").unwrap()
    });
    let object_re = OBJECT_RE.get_or_init(|| {
        Regex::new(r"(?i)<object\b[^>]*>([\s\S]*?)</object>|<object\b[^>]*/>").unwrap()
    });
    let embed_re = EMBED_RE.get_or_init(|| {
        Regex::new(r"(?i)<embed\b[^>]*>([\s\S]*?)</embed>|<embed\b[^>]*/>").unwrap()
    });
    let form_re = FORM_RE.get_or_init(|| {
        Regex::new(r"(?i)<form\b[^>]*>([\s\S]*?)</form>|<form\b[^>]*/>").unwrap()
    });
    let event_re = EVENT_HANDLER_RE.get_or_init(|| {
        Regex::new(r#"(?i)\bon[a-z]+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).unwrap()
    });
    let js_url_re = JAVASCRIPT_URL_RE.get_or_init(|| {
        Regex::new(r#"(?i)\b(href|xlink:href)\s*=\s*(?:"\s*javascript:[^"]*"|'\s*javascript:[^']*')"#).unwrap()
    });

    // Strip malicious tags
    sanitized = script_re.replace_all(&sanitized, "").into_owned();
    sanitized = foreign_obj_re.replace_all(&sanitized, "").into_owned();
    sanitized = iframe_re.replace_all(&sanitized, "").into_owned();
    sanitized = object_re.replace_all(&sanitized, "").into_owned();
    sanitized = embed_re.replace_all(&sanitized, "").into_owned();
    sanitized = form_re.replace_all(&sanitized, "").into_owned();

    // Strip event handlers (e.g., onload, onclick)
    sanitized = event_re.replace_all(&sanitized, "").into_owned();

    // Strip javascript: URLs inside href or xlink:href
    sanitized = js_url_re.replace_all(&sanitized, "href=\"#\"").into_owned();

    Ok(sanitized)
}

/// Computes the Shannon entropy of a string to detect high-entropy keys/passwords
fn calculate_entropy(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let mut counts = HashMap::new();
    for c in text.chars() {
        *counts.entry(c).or_insert(0) += 1;
    }
    let len = text.chars().count() as f64;
    let mut entropy = 0.0;
    for &count in counts.values() {
        let p = (count as f64) / len;
        entropy -= p * p.log2();
    }
    entropy
}

/// Classifies the sensitivity of the given text content
pub fn classify_sensitivity(text: &str) -> Sensitivity {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Sensitivity::None;
    }

    // Static Regex patterns for known sensitive formats
    static PRIVATE_KEY_RE: OnceLock<Regex> = OnceLock::new();
    static CREDIT_CARD_RE: OnceLock<Regex> = OnceLock::new();
    static API_KEY_RE: OnceLock<Regex> = OnceLock::new();
    static EMAIL_RE: OnceLock<Regex> = OnceLock::new();
    static PHONE_RE: OnceLock<Regex> = OnceLock::new();

    let priv_key_re = PRIVATE_KEY_RE.get_or_init(|| {
        Regex::new(r"(?i)-----BEGIN [A-Z0-9\s_]+ PRIVATE KEY-----").unwrap()
    });
    let cc_re = CREDIT_CARD_RE.get_or_init(|| {
        Regex::new(r"\b(?:4[0-9]{12}(?:[0-9]{3})?|[25][0-9]{14}|6(?:011|5[0-9][0-9])[0-9]{12}|3[47][0-9]{13})\b").unwrap()
    });
    let api_key_re = API_KEY_RE.get_or_init(|| {
        Regex::new(r"(?i)(sk-[a-zA-Z0-9]{48}|ghp_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9]{82}|AKIA[0-9A-Z]{16})").unwrap()
    });
    let email_re = EMAIL_RE.get_or_init(|| {
        Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap()
    });
    let phone_re = PHONE_RE.get_or_init(|| {
        Regex::new(r"\b(?:\+?[0-9]{1,3})?[-.\s]?(?:\(?[0-9]{2,4}\)?)?[-.\s]?[0-9]{3,4}][-.\s]?[0-9]{3,4}\b").unwrap()
    });

    // 1. Check for Secret Level (Private Keys, Credit Cards, High Entropy Passwords)
    if priv_key_re.is_match(trimmed) || cc_re.is_match(trimmed) {
        return Sensitivity::Secret;
    }

    // Check if it looks like a password:
    // Single word (no spaces), length >= 12, contains digits, uppercase, lowercase, and specials, and high entropy.
    if !trimmed.contains(char::is_whitespace) && trimmed.len() >= 12 && trimmed.len() <= 64 {
        let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
        let has_upper = trimmed.chars().any(|c| c.is_ascii_uppercase());
        let has_lower = trimmed.chars().any(|c| c.is_ascii_lowercase());
        let has_special = trimmed.chars().any(|c| !c.is_alphanumeric());
        if has_digit && has_upper && has_lower && has_special && calculate_entropy(trimmed) > 3.8 {
            return Sensitivity::Secret;
        }
    }

    // 2. Check for Credential Level (Known API Keys, high-entropy tokens)
    if api_key_re.is_match(trimmed) {
        return Sensitivity::Credential;
    }

    // If it's a single word with high entropy and >= 32 chars, classify as Credential
    if !trimmed.contains(char::is_whitespace) && trimmed.len() >= 32 && trimmed.len() <= 128 {
        let is_url = trimmed.starts_with("http://") || trimmed.starts_with("https://");
        if !is_url || trimmed.contains('@') {
            if calculate_entropy(trimmed) > 4.2 {
                return Sensitivity::Credential;
            }
        }
    }

    // 3. Check for Personal Level (Email, Phone)
    if email_re.is_match(trimmed) || phone_re.is_match(trimmed) {
        return Sensitivity::Personal;
    }

    // 4. Default
    Sensitivity::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_text() {
        let text = "Hello\0 World\u{0007}!";
        assert_eq!(sanitize_text(text), "Hello World!");
    }

    #[test]
    fn test_sanitize_svg() {
        let svg = r#"<svg><script>alert(1)</script><rect onload="alert(2)" href="javascript:do_evil()"/></svg>"#;
        let sanitized = sanitize_svg(svg).unwrap();
        assert!(!sanitized.contains("<script"));
        assert!(!sanitized.contains("onload"));
        assert!(!sanitized.contains("javascript:"));
    }

    #[test]
    fn test_classify_sensitivity() {
        assert_eq!(classify_sensitivity("Hello there"), Sensitivity::None);
        assert_eq!(classify_sensitivity("test@example.com"), Sensitivity::Personal);
        assert_eq!(classify_sensitivity("ghp_123456789012345678901234567890123456"), Sensitivity::Credential);
        assert_eq!(classify_sensitivity("-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA..."), Sensitivity::Secret);
        assert_eq!(classify_sensitivity("https://github.com/rust-lang/rust/commit/8d37aa908f51a44e6d426315df474e76a6b57cc7"), Sensitivity::None);
        assert_eq!(classify_sensitivity("https://user:password@github.com/rust-lang/rust"), Sensitivity::Credential);
        assert_eq!(classify_sensitivity("https://apnews.com/article/chad-sudan-civil-war-sexual-abuse-refugees-036f69088cd3a96cb3c970ddbfd00696"), Sensitivity::None);
    }
}

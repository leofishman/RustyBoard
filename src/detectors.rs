#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum DetectedType {
    Text,
    Svg,
    Url,
    Json,
}

pub fn classify_text(text: &str) -> DetectedType {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return DetectedType::Text;
    }

    // 1. Detect SVG
    if (trimmed.starts_with("<svg") && trimmed.ends_with("</svg>"))
        || (trimmed.starts_with("<?xml") && trimmed.contains("<svg") && trimmed.ends_with("</svg>"))
    {
        return DetectedType::Svg;
    }

    // 2. Detect URL
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        // Simple sanity check that it's a URL (no spaces)
        if !trimmed.contains(char::is_whitespace) {
            return DetectedType::Url;
        }
    }

    // 3. Detect JSON
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
            return DetectedType::Json;
        }
    }

    DetectedType::Text
}

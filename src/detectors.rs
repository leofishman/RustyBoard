#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum DetectedType {
    Text,
    Svg,
    Url,
    Json,
    Mermaid,
    Markdown,
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

    // 4. Detect Mermaid Diagram
    let lower = trimmed.to_lowercase();
    if lower.starts_with("graph ")
        || lower.starts_with("flowchart ")
        || lower.starts_with("sequencediagram")
        || lower.starts_with("gantt")
        || lower.starts_with("classdiagram")
        || lower.starts_with("statediagram")
        || lower.starts_with("erdiagram")
        || lower.starts_with("journey")
        || lower.starts_with("pie")
        || lower.starts_with("gitgraph")
        || trimmed.starts_with("```mermaid")
    {
        return DetectedType::Mermaid;
    }

    // 5. Detect Markdown
    // Headings, bold, links, lists, code fences
    if trimmed.starts_with("# ")
        || trimmed.starts_with("## ")
        || trimmed.starts_with("### ")
        || trimmed.contains("\n# ")
        || trimmed.contains("\n## ")
        || trimmed.contains("\n### ")
        || trimmed.contains("**")
        || (trimmed.contains("[") && trimmed.contains("](") && trimmed.contains(")"))
        || (trimmed.starts_with("- ") || trimmed.contains("\n- "))
        || (trimmed.starts_with("* ") || trimmed.contains("\n* "))
        || (trimmed.starts_with("```") && trimmed.ends_with("```"))
    {
        return DetectedType::Markdown;
    }

    DetectedType::Text
}

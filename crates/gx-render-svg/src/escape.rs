//! Mandatory escaping of every string that comes from the JSON (PRD §9.1, S12).
//!
//! The apostrophe is `&#39;` and not `&apos;` because PRD §9.1 spells that
//! exact entity and the golden hashes compare byte by byte.

/// Escapes the five XML metacharacters of PRD §9.1.
pub fn escape_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::escape_text;

    #[test]
    fn escapes_the_five_xml_metacharacters() {
        assert_eq!(escape_text("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;");
    }

    #[test]
    fn leaves_plain_text_untouched() {
        assert_eq!(escape_text("read file — Config"), "read file — Config");
    }

    #[test]
    fn does_not_unescape_existing_entities() {
        assert_eq!(escape_text("&lt;"), "&amp;lt;");
    }

    #[test]
    fn escapes_an_empty_string_to_an_empty_string() {
        assert_eq!(escape_text(""), "");
    }
}

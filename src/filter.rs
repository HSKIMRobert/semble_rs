use once_cell::sync::Lazy;
use regex::Regex;

static MULTIPLE_BLANK_LINES: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());

pub fn strip_comments(content: &str, lang: Option<&str>) -> String {
    let patterns = CommentSyntax::from_lang(lang);
    let mut result = String::with_capacity(content.len());
    let mut in_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if let (Some(start), Some(end)) = (patterns.block_start, patterns.block_end) {
            if !in_block && trimmed.contains(start) {
                in_block = true;
            }
            if in_block {
                if trimmed.contains(end) {
                    in_block = false;
                }
                continue;
            }
        }

        if let Some(prefix) = patterns.line_prefix {
            if trimmed.starts_with(prefix) && !trimmed.starts_with(patterns.doc_prefix.unwrap_or("///")) {
                continue;
            }
        }

        if trimmed.is_empty() {
            result.push('\n');
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    let result = MULTIPLE_BLANK_LINES.replace_all(&result, "\n\n");
    result.trim().to_string()
}

struct CommentSyntax {
    line_prefix: Option<&'static str>,
    block_start: Option<&'static str>,
    block_end: Option<&'static str>,
    doc_prefix: Option<&'static str>,
}

impl CommentSyntax {
    fn from_lang(lang: Option<&str>) -> Self {
        match lang.unwrap_or("") {
            "python" => Self {
                line_prefix: Some("#"),
                block_start: None,
                block_end: None,
                doc_prefix: None,
            },
            "ruby" | "shell" | "bash" => Self {
                line_prefix: Some("#"),
                block_start: None,
                block_end: None,
                doc_prefix: None,
            },
            _ => Self {
                line_prefix: Some("//"),
                block_start: Some("/*"),
                block_end: Some("*/"),
                doc_prefix: Some("///"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_line_comments() {
        let code = "// comment\nfn main() {\n    println!(\"hello\");\n}\n";
        let result = strip_comments(code, Some("rust"));
        assert!(!result.contains("// comment"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_keep_doc_comments() {
        let code = "/// Doc comment\nfn main() {}\n";
        let result = strip_comments(code, Some("rust"));
        assert!(result.contains("/// Doc comment"));
    }

    #[test]
    fn test_strip_block_comments() {
        let code = "/* block */\nfn main() {}\n";
        let result = strip_comments(code, Some("rust"));
        assert!(!result.contains("block"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_strip_python_comments() {
        let code = "# comment\ndef main():\n    pass\n";
        let result = strip_comments(code, Some("python"));
        assert!(!result.contains("# comment"));
        assert!(result.contains("def main():"));
    }

    #[test]
    fn test_collapse_blank_lines() {
        let code = "a\n\n\n\n\nb\n";
        let result = strip_comments(code, None);
        assert!(!result.contains("\n\n\n"));
    }
}

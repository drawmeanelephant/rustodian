use crate::message::{MarkdownBlock, ParsedMarkdown};

/// Parse a raw string into Markdown blocks.
pub(crate) fn parse_markdown(text: &str) -> ParsedMarkdown {
    let mut blocks = Vec::new();
    let mut in_code_block = false;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue; // The fence itself isn't a block we render directly here, or we could include it
        }
        if in_code_block {
            blocks.push(MarkdownBlock::CodeFence {
                text: line.to_string(),
            });
            continue;
        }

        if trimmed.is_empty() {
            blocks.push(MarkdownBlock::BlankLine);
            continue;
        }

        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            blocks.push(MarkdownBlock::HorizontalRule);
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("#### ") {
            blocks.push(MarkdownBlock::Header {
                level: 4,
                text: rest.to_string(),
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            blocks.push(MarkdownBlock::Header {
                level: 3,
                text: rest.to_string(),
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            blocks.push(MarkdownBlock::Header {
                level: 2,
                text: rest.to_string(),
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            blocks.push(MarkdownBlock::Header {
                level: 1,
                text: rest.to_string(),
            });
            continue;
        }

        if let Some(rest) = strip_task_prefix(trimmed, true) {
            blocks.push(MarkdownBlock::Task {
                text: rest.to_string(),
                checked: true,
            });
            continue;
        }
        if let Some(rest) = strip_task_prefix(trimmed, false) {
            blocks.push(MarkdownBlock::Task {
                text: rest.to_string(),
                checked: false,
            });
            continue;
        }

        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            blocks.push(MarkdownBlock::BulletList {
                text: rest.to_string(),
            });
            continue;
        }

        if let Some(dot_pos) = trimmed.find(". ") {
            let prefix = &trimmed[..dot_pos];
            if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
                blocks.push(MarkdownBlock::NumberedList {
                    number: trimmed[..=dot_pos].to_string(),
                    text: trimmed[dot_pos + 2..].to_string(),
                });
                continue;
            }
        }

        blocks.push(MarkdownBlock::Text {
            text: line.to_string(),
        });
    }

    ParsedMarkdown { blocks }
}

fn strip_task_prefix(line: &str, checked: bool) -> Option<&str> {
    let patterns: &[&str] = if checked {
        &["- [x] ", "- [X] ", "* [x] ", "* [X] "]
    } else {
        &["- [ ] ", "* [ ] "]
    };
    for pat in patterns {
        if let Some(rest) = line.strip_prefix(pat) {
            return Some(rest);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_tasks() {
        let input = "- [ ] task one\n- [x] task two\n";
        insta::assert_debug_snapshot!(parse_markdown(input));
    }

    #[test]
    fn test_parse_markdown_commands() {
        let input = "## Commands\n```\ncargo test\n```\n";
        insta::assert_debug_snapshot!(parse_markdown(input));
    }
}

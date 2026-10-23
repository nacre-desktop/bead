// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Markdown subset of a chat transcript. Block structure is kept
//! (paragraphs, headings, lists, quotes, rules, fenced code); inline marks are
//! flattened to plain text because a text box has one style. Assistant text only;
//! the user's own text stays plain, as in the QML.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(String),
    Heading(u8, String),
    Item {
        depth: u8,
        marker: String,
        text: String,
    },
    Quote(String),
    Code(String),
    Rule,
}

fn fence(line: &str) -> Option<&str> {
    let line = line.trim_start();
    ["```", "~~~"]
        .into_iter()
        .find_map(|mark| line.strip_prefix(mark))
        .map(str::trim)
}
fn rule(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3
        && ['-', '*', '_']
            .into_iter()
            .any(|c| line.chars().all(|x| x == c || x == ' ') && line.matches(c).count() >= 3)
}
fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    let rest = line[hashes..].strip_prefix(' ')?;
    (1..=6)
        .contains(&hashes)
        .then_some((hashes as u8, rest.trim()))
}
fn item(line: &str) -> Option<(u8, String, &str)> {
    let indent = line.len() - line.trim_start().len();
    let body = line.trim_start();
    let depth = (indent / 2).min(4) as u8;
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = body.strip_prefix(bullet) {
            return Some((depth, "•".into(), rest.trim()));
        }
    }
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let rest = &body[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((depth, format!("{}.", &body[..digits]), rest.trim()));
        }
    }
    None
}

/// Blocks of `text`. An unclosed fence (a streaming answer) is code up to the end.
pub fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut quote = String::new();
    let mut code: Option<Vec<&str>> = None;
    let flush = |blocks: &mut Vec<Block>, paragraph: &mut String, quote: &mut String| {
        if !paragraph.is_empty() {
            blocks.push(Block::Paragraph(std::mem::take(paragraph)));
        }
        if !quote.is_empty() {
            blocks.push(Block::Quote(std::mem::take(quote)));
        }
    };
    for line in text.lines() {
        if let Some(lines) = &mut code {
            if fence(line).is_some_and(str::is_empty) {
                blocks.push(Block::Code(lines.join("\n")));
                code = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        if fence(line).is_some() {
            flush(&mut blocks, &mut paragraph, &mut quote);
            code = Some(Vec::new());
        } else if line.trim().is_empty() {
            flush(&mut blocks, &mut paragraph, &mut quote);
        } else if let Some((level, rest)) = heading(line.trim_start()) {
            flush(&mut blocks, &mut paragraph, &mut quote);
            blocks.push(Block::Heading(level, inline(rest)));
        } else if rule(line) {
            flush(&mut blocks, &mut paragraph, &mut quote);
            blocks.push(Block::Rule);
        } else if let Some((depth, marker, rest)) = item(line) {
            flush(&mut blocks, &mut paragraph, &mut quote);
            blocks.push(Block::Item {
                depth,
                marker,
                text: inline(rest),
            });
        } else if let Some(rest) = line.trim_start().strip_prefix('>') {
            if !paragraph.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut quote);
            }
            if !quote.is_empty() {
                quote.push(' ');
            }
            quote.push_str(&inline(rest.trim()));
        } else if let Some(Block::Item { text, .. }) = blocks
            .last_mut()
            .filter(|_| paragraph.is_empty() && quote.is_empty() && line.starts_with([' ', '\t']))
        {
            // Continuation of the previous list item.
            text.push(' ');
            text.push_str(&inline(line.trim()));
        } else {
            if !quote.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut quote);
            }
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(&inline(line.trim()));
        }
    }
    if let Some(lines) = code {
        blocks.push(Block::Code(lines.join("\n")));
    }
    flush(&mut blocks, &mut paragraph, &mut quote);
    blocks
}

/// Contents of the closed fenced blocks, exactly what the QML "Copy code" buttons copy.
pub fn code_blocks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut open: Option<Vec<&str>> = None;
    for line in text.lines() {
        match &mut open {
            Some(lines) if fence(line).is_some_and(str::is_empty) => {
                out.push(lines.join("\n"));
                open = None;
            }
            Some(lines) => lines.push(line),
            None if fence(line).is_some() => open = Some(Vec::new()),
            None => {}
        }
    }
    out
}

fn closing(chars: &[char], from: usize, mark: &[char]) -> Option<usize> {
    (from..chars.len().saturating_sub(mark.len() - 1))
        .find(|&i| chars[i..].starts_with(mark) && i > from && !chars[i - 1].is_whitespace())
}
/// Flatten inline marks: `code`, emphasis, strike-through, links and images.
pub fn inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let word_before = i > 0 && chars[i - 1].is_alphanumeric();
        match c {
            '\\' if i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() => {
                out.push(chars[i + 1]);
                i += 2;
            }
            '`' => {
                let ticks = chars[i..].iter().take_while(|c| **c == '`').count();
                let mark = vec!['`'; ticks];
                if let Some(end) = (i + ticks..chars.len()).find(|&j| chars[j..].starts_with(&mark))
                {
                    let body: String = chars[i + ticks..end].iter().collect();
                    out.push_str(body.trim());
                    i = end + ticks;
                } else {
                    out.extend(&chars[i..i + ticks]);
                    i += ticks;
                }
            }
            '*' | '_' | '~' => {
                let run = chars[i..].iter().take_while(|x| **x == c).count();
                let width = run.min(if c == '~' { 2 } else { 3 });
                let opens = i + width < chars.len() && !chars[i + width].is_whitespace();
                let intraword = c == '_' && word_before;
                let mark = vec![c; width];
                if opens
                    && !intraword
                    && (c != '~' || width == 2)
                    && let Some(end) = closing(&chars, i + width, &mark)
                {
                    let inner: String = chars[i + width..end].iter().collect();
                    out.push_str(&inline(&inner));
                    i = end + width;
                    continue;
                }
                out.extend(&chars[i..i + run]);
                i += run;
            }
            '!' | '[' => {
                let (image, at) = if c == '!' && chars.get(i + 1) == Some(&'[') {
                    (true, i + 1)
                } else {
                    (false, i)
                };
                let label_end = (c == '[' || image)
                    .then(|| (at + 1..chars.len()).find(|&j| chars[j] == ']'))
                    .flatten();
                let target = label_end.filter(|&e| chars.get(e + 1) == Some(&'('));
                if let Some(e) = target
                    && let Some(close) = (e + 2..chars.len()).find(|&j| chars[j] == ')')
                {
                    let label: String = chars[at + 1..e].iter().collect();
                    let url: String = chars[e + 2..close].iter().collect();
                    let label = inline(&label);
                    if image {
                        out.push_str(&label);
                    } else if label.is_empty() || label == url {
                        out.push_str(&url);
                    } else {
                        out.push_str(&format!("{label} ({url})"));
                    }
                    i = close + 1;
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_cover_the_transcript_subset() {
        let text = "# Title\n\nSome *text*\nacross lines.\n\n- one\n  more\n- two\n1. first\n\n> quoted\n> text\n\n```rust\nlet x = 1;\n\nlet y = 2;\n```\n---\nafter";
        assert_eq!(
            parse(text),
            vec![
                Block::Heading(1, "Title".into()),
                Block::Paragraph("Some text across lines.".into()),
                Block::Item {
                    depth: 0,
                    marker: "•".into(),
                    text: "one more".into()
                },
                Block::Item {
                    depth: 0,
                    marker: "•".into(),
                    text: "two".into()
                },
                Block::Item {
                    depth: 0,
                    marker: "1.".into(),
                    text: "first".into()
                },
                Block::Quote("quoted text".into()),
                Block::Code("let x = 1;\n\nlet y = 2;".into()),
                Block::Rule,
                Block::Paragraph("after".into()),
            ]
        );
    }

    #[test]
    fn an_unclosed_fence_is_code_while_streaming_and_copy_code_needs_a_closed_one() {
        let text = "Here:\n```\nfn main() {";
        assert_eq!(
            parse(text),
            vec![
                Block::Paragraph("Here:".into()),
                Block::Code("fn main() {".into())
            ]
        );
        assert!(code_blocks(text).is_empty());
        let closed = "a\n```sh\nls\n```\nb\n```\npwd\n```\n";
        assert_eq!(code_blocks(closed), vec!["ls", "pwd"]);
    }

    #[test]
    fn inline_marks_flatten_without_eating_identifiers() {
        assert_eq!(
            inline("**bold** and *it* and `code()`"),
            "bold and it and code()"
        );
        assert_eq!(inline("snake_case_name stays"), "snake_case_name stays");
        assert_eq!(inline("2 * 3 * 4"), "2 * 3 * 4");
        assert_eq!(inline("~~gone~~"), "gone");
        assert_eq!(
            inline("see [the docs](https://example.org/a) now"),
            "see the docs (https://example.org/a) now"
        );
        assert_eq!(inline("![logo](x.png) text"), "logo text");
        assert_eq!(inline("\\*literal\\*"), "*literal*");
        assert_eq!(inline("``a ` b``"), "a ` b");
        assert_eq!(inline("unclosed `tick"), "unclosed `tick");
        assert_eq!(inline("héllo **wörld**"), "héllo wörld");
    }
}

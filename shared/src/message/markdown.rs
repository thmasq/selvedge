use matrix_sdk::ruma::{UserId, events::Mentions};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use std::collections::HashSet;
use std::ops::Range;

pub fn parse_matrix_markdown(text: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_MATH);

    // First pass: locate code spans/blocks and math spans so spoiler
    // substitution can never split a "||" that belongs to `a || b`,
    // fenced code, or `$||v||$`-style LaTeX norm notation.
    let protected = protected_ranges(text, options);
    let with_spoilers = process_spoilers(text, &protected);

    // Second, real pass: spoilers are now genuine <span> HTML in the
    // source, so CommonMark parses markdown inside them normally
    // (||some *bold* text||) instead of splitting it into separate,
    // unpaired Text events the way AST-level splitting would.
    let parser = Parser::new_ext(&with_spoilers, options);
    let events = parser.map(|event| match event {
        Event::InlineMath(cow) => {
            let escaped = html_escape(&cow);
            Event::Html(
                format!("<span data-mx-maths=\"{escaped}\"><code>{escaped}</code></span>").into(),
            )
        }
        Event::DisplayMath(cow) => {
            let escaped = html_escape(&cow);
            Event::Html(
                format!("<div data-mx-maths=\"{escaped}\"><code>{escaped}</code></div>").into(),
            )
        }
        _ => event,
    });

    let mut raw_html = String::new();
    html::push_html(&mut raw_html, events);
    // pulldown-cmark renders table alignment as style="text-align: ..."
    // which isn't in the sanitizer's default td/th allow-list; rewrite
    // it to the equivalent, already-permitted `align` attribute.
    let raw_html = raw_html
        .replace(r#"style="text-align: left""#, r#"align="left""#)
        .replace(r#"style="text-align: center""#, r#"align="center""#)
        .replace(r#"style="text-align: right""#, r#"align="right""#);
    let safe_html = crate::sanitize_matrix_html(&raw_html);
    safe_html.trim().to_string()
}

pub fn extract_mentions(text: &str) -> Option<Mentions> {
    const MAX_MXID_LEN: usize = 255;
    const TRAILING_PUNCT: &[char] = &[')', ']', '}', '>', ',', '.', '!', '?', ';', ':', '\'', '"'];

    let mut user_ids = HashSet::new();
    let mut room = false;
    let mut prev_char: Option<char> = None;

    for (start, ch) in text.char_indices() {
        if ch == '@' && !prev_char.is_some_and(|c| c.is_alphanumeric()) {
            let mut end = start + ch.len_utf8();
            for c in text[end..].chars() {
                if c.is_whitespace() || end - start >= MAX_MXID_LEN {
                    break;
                }
                end += c.len_utf8();
            }
            let mut candidate = &text[start..end];
            loop {
                if candidate == "@room" {
                    room = true;
                    break;
                }
                if let Ok(user_id) = UserId::parse(candidate) {
                    user_ids.insert(user_id);
                    break;
                }
                match candidate.chars().last() {
                    Some(c) if candidate.len() > 1 && TRAILING_PUNCT.contains(&c) => {
                        candidate = &candidate[..candidate.len() - c.len_utf8()];
                    }
                    _ => break,
                }
            }
        }
        prev_char = Some(ch);
    }

    if user_ids.is_empty() && !room {
        None
    } else {
        let mut mentions = Mentions::with_user_ids(user_ids.into_iter());
        mentions.room = room;
        Some(mentions)
    }
}

// Byte ranges, in the original source, that spoiler substitution must
// never split a "||" across: code spans/blocks and math spans.
fn protected_ranges(text: &str, options: Options) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut block_start: Option<usize> = None;
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => block_start = Some(range.start),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = block_start.take() {
                    ranges.push(start..range.end);
                }
            }
            Event::Code(_) | Event::InlineMath(_) | Event::DisplayMath(_) => ranges.push(range),
            _ => {}
        }
    }
    ranges
}

fn is_protected(pos: usize, ranges: &[Range<usize>], cursor: &mut usize) -> bool {
    while *cursor < ranges.len() && ranges[*cursor].end <= pos {
        *cursor += 1;
    }
    *cursor < ranges.len() && ranges[*cursor].start <= pos && pos < ranges[*cursor].end
}

// Byte offsets of unescaped "||" pairs outside protected ranges, left
// to right. Respects CommonMark backslash-escaping (`\|` can never
// start or complete a pair). An odd match count drops the final,
// unpaired "||" so it stays literal instead of opening a spoiler that
// swallows the rest of the message.
fn find_spoiler_markers(text: &str, protected: &[Range<usize>]) -> Vec<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut markers = Vec::new();
    let mut cursor = 0;
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if c == '\\' && i + 1 < chars.len() && chars[i + 1].1.is_ascii_punctuation() {
            i += 2;
            continue;
        }
        if c == '|' && i + 1 < chars.len() && chars[i + 1].1 == '|' {
            let (pos2, _) = chars[i + 1];
            if !is_protected(pos, protected, &mut cursor)
                && !is_protected(pos2, protected, &mut cursor)
            {
                markers.push(pos);
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    if markers.len() % 2 != 0 {
        markers.pop();
    }
    markers
}

// A tag alone at the start of a line (only whitespace before it) can be
// parsed by CommonMark as the start of a raw HTML block rather than
// inline HTML, which would stop nested markdown from being re-parsed
// inside the spoiler. A zero-width space guarantees the line never
// starts with just the tag, without being visible when rendered.
fn at_line_start(text: &str, pos: usize) -> bool {
    let line_start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    text[line_start..pos].chars().all(|c| c == ' ' || c == '\t')
}

// fn process_spoilers(text: &str, protected: &[Range<usize>]) -> String {
//     let markers = find_spoiler_markers(text, protected);
//     let mut result = String::with_capacity(text.len());
//     let mut cursor = 0;
//     for (idx, &pos) in markers.iter().enumerate() {
//         result.push_str(&text[cursor..pos]);
//         result.push_str(if idx % 2 == 0 {
//             "<span data-mx-spoiler>"
//         } else {
//             "</span>"
//         });
//         cursor = pos + 2;
//     }
//     result.push_str(&text[cursor..]);
//     result
// }

fn process_spoilers(text: &str, protected: &[Range<usize>]) -> String {
    let markers = find_spoiler_markers(text, protected);
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    for (idx, &pos) in markers.iter().enumerate() {
        result.push_str(&text[cursor..pos]);
        if at_line_start(text, pos) {
            result.push_str("<wbr>");
        }
        result.push_str(if idx % 2 == 0 {
            "<span data-mx-spoiler>"
        } else {
            "</span>"
        });
        cursor = pos + 2;
    }
    result.push_str(&text[cursor..]);
    result
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spoiler_open_variants() -> [&'static str; 2] {
        ["<span data-mx-spoiler>", "<span data-mx-spoiler=\"\">"]
    }

    fn contains_spoiler_wrapping(html: &str, inner: &str) -> bool {
        spoiler_open_variants()
            .iter()
            .any(|open| html.contains(&format!("{open}{inner}</span>")))
    }

    // --- spoilers ---

    #[test]
    fn spoiler_basic() {
        let html = parse_matrix_markdown("||secret||");
        assert!(contains_spoiler_wrapping(&html, "secret"));
    }

    #[test]
    fn spoiler_with_nested_markdown() {
        let html = parse_matrix_markdown("||This is **bold**||");
        assert!(
            spoiler_open_variants()
                .iter()
                .any(|open| html.contains(open))
        );
        // Nested markdown must be re-parsed, not left as literal asterisks.
        assert!(html.contains("<strong>bold</strong>"));
        assert!(!html.contains('*'));
    }

    #[test]
    fn spoiler_escaped_pipes_stay_literal() {
        let html = parse_matrix_markdown(r"a \|\| b");
        assert!(!html.contains("data-mx-spoiler"));
        assert!(html.contains("a || b"));
    }

    #[test]
    fn spoiler_pipes_inside_inline_code_are_untouched() {
        let html = parse_matrix_markdown("`if (a || b) return;`");
        assert!(!html.contains("data-mx-spoiler"));
        assert!(html.contains("if (a || b) return;"));
    }

    #[test]
    fn spoiler_pipes_inside_fenced_code_block_are_untouched() {
        let html = parse_matrix_markdown("```\ncmd1 || cmd2\n```");
        assert!(!html.contains("data-mx-spoiler"));
        assert!(html.contains("cmd1 || cmd2"));
    }

    #[test]
    fn spoiler_pipes_inside_inline_math_are_untouched() {
        // ||v|| is standard vector-norm notation; must not be eaten as a spoiler.
        let html = parse_matrix_markdown("$||v||$");
        assert!(!html.contains("data-mx-spoiler"));
        assert!(html.contains(r#"data-mx-maths="||v||""#));
    }

    #[test]
    fn spoiler_unpaired_trailing_marker_stays_literal() {
        // Three "||" occurrences: first pair forms a spoiler, the
        // leftover third marker must not open an unterminated spoiler
        // that swallows the rest of the message.
        let html = parse_matrix_markdown("a||b||c||d");
        assert!(contains_spoiler_wrapping(&html, "b"));
        assert!(html.contains("c||d"));
        // Only one spoiler span should have been created.
        assert_eq!(html.matches("data-mx-spoiler").count(), 1);
    }

    #[test]
    fn spoiler_at_start_of_message_still_parses_nested_markdown() {
        // Regression check for the HTML-block-detection edge case: a
        // spoiler as the very first thing on a line must not fall back
        // to raw-HTML parsing (which would skip nested markdown).
        let html = parse_matrix_markdown("||*bold*||");
        assert!(html.contains("data-mx-spoiler"));
        assert!(html.contains("<em>bold</em>"));
    }

    #[test]
    fn spoiler_at_start_of_message_leaves_no_selectable_character() {
        // The fix for the above must not leak a literal invisible
        // character (e.g. a zero-width space) into the message text.
        let html = parse_matrix_markdown("||*bold*||");
        assert!(!html.contains('\u{200B}'));
        assert!(html.contains("<wbr>"));
    }

    #[test]
    fn at_line_start_detects_start_of_string_and_after_newline() {
        assert!(at_line_start("||x||", 0));
        assert!(at_line_start("hello\n||x||", 6));
        assert!(!at_line_start("hello ||x||", 6));
    }

    // --- tables ---

    #[test]
    fn table_alignment_uses_align_attribute_not_style() {
        let md = "| L | C | R |\n|:---|:---:|---:|\n| a | b | c |\n";
        let html = parse_matrix_markdown(md);
        assert!(html.contains(r#"align="left""#));
        assert!(html.contains(r#"align="center""#));
        assert!(html.contains(r#"align="right""#));
        assert!(!html.contains("text-align"));
    }

    // --- mentions ---

    #[test]
    fn extract_mentions_plain_user_id() {
        let mentions = extract_mentions("hey @alice:example.org check this").unwrap();
        assert!(
            mentions
                .user_ids
                .contains(&UserId::parse("@alice:example.org").unwrap())
        );
        assert!(!mentions.room);
    }

    #[test]
    fn extract_mentions_at_room() {
        let mentions = extract_mentions("@room heads up").unwrap();
        assert!(mentions.room);
        assert!(mentions.user_ids.is_empty());
    }

    #[test]
    fn extract_mentions_at_room_with_trailing_punctuation() {
        let mentions = extract_mentions("@room! please read").unwrap();
        assert!(mentions.room);
    }

    #[test]
    fn extract_mentions_strips_surrounding_punctuation() {
        let mentions = extract_mentions("ping (@alice:example.org).").unwrap();
        assert!(
            mentions
                .user_ids
                .contains(&UserId::parse("@alice:example.org").unwrap())
        );
    }

    #[test]
    fn extract_mentions_finds_id_inside_matrix_to_link() {
        let mentions =
            extract_mentions("ping [Alice](https://matrix.to/#/@alice:example.org) now").unwrap();
        assert!(
            mentions
                .user_ids
                .contains(&UserId::parse("@alice:example.org").unwrap())
        );
    }

    #[test]
    fn extract_mentions_ignores_email_addresses() {
        assert!(extract_mentions("reach me at foo@bar.com for details").is_none());
    }

    #[test]
    fn extract_mentions_none_when_absent() {
        assert!(extract_mentions("just a normal message").is_none());
    }
}

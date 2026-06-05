use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

pub fn highlight_code_line(
    line: &str,
    theme: &crate::tui::theme::ActiveTheme,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let n = chars.len();

    // Helper to peek next non-whitespace character index
    let peek_next_non_ws = |mut idx: usize| -> Option<char> {
        while idx < n {
            if !chars[idx].is_whitespace() {
                return Some(chars[idx]);
            }
            idx += 1;
        }
        None
    };

    while i < n {
        // 1. C-style Block comments: /* ... */ (if it starts and ends on the same line)
        if chars[i] == '/' && i + 1 < n && chars[i + 1] == '*' {
            let mut s = String::new();
            s.push('/');
            s.push('*');
            i += 2;
            while i < n {
                s.push(chars[i]);
                if chars[i] == '*' && i + 1 < n && chars[i + 1] == '/' {
                    s.push('/');
                    i += 2;
                    break;
                }
                i += 1;
            }
            spans.push(Span::styled(s, Style::default().fg(theme.syntax_comment)));
            continue;
        }

        // 2. Preprocessor directive vs Comment for '#'
        if chars[i] == '#' {
            // Check if it's a preprocessor directive (e.g., #include, #define, #import)
            let mut is_directive = false;
            let mut j = i + 1;
            while j < n && chars[j].is_whitespace() {
                j += 1;
            }
            if j < n && chars[j].is_alphabetic() {
                let mut word = String::new();
                while j < n && chars[j].is_alphabetic() {
                    word.push(chars[j]);
                    j += 1;
                }
                if matches!(
                    word.as_str(),
                    "include"
                        | "define"
                        | "undef"
                        | "ifdef"
                        | "ifndef"
                        | "if"
                        | "else"
                        | "elif"
                        | "endif"
                        | "error"
                        | "pragma"
                        | "import"
                ) {
                    is_directive = true;
                }
            }

            if is_directive {
                // Style the directive word as Magenta and keep parsing
                let mut directive_text = String::new();
                directive_text.push('#');
                i += 1;
                while i < n && (chars[i].is_alphanumeric() || chars[i].is_whitespace()) {
                    directive_text.push(chars[i]);
                    i += 1;
                }
                spans.push(Span::styled(
                    directive_text,
                    Style::default()
                        .fg(theme.syntax_keyword)
                        .add_modifier(Modifier::BOLD),
                ));
                continue;
            } else {
                // Ordinary comment
                let comment_text: String = chars[i..].iter().collect();
                spans.push(Span::styled(
                    comment_text,
                    Style::default().fg(theme.syntax_comment),
                ));
                break;
            }
        }

        // 3. Line comments: //
        if chars[i] == '/' && i + 1 < n && chars[i + 1] == '/' {
            let comment_text: String = chars[i..].iter().collect();
            spans.push(Span::styled(
                comment_text,
                Style::default().fg(theme.syntax_comment),
            ));
            break;
        }

        // 4. String check: double quote, single quote, backtick
        if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
            let quote = chars[i];
            let mut s = String::new();
            s.push(quote);
            i += 1;
            let mut escaped = false;
            while i < n {
                let c = chars[i];
                s.push(c);
                i += 1;
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    break;
                }
            }

            // JSON key heuristic: if next non-whitespace char is ':', style it as Blue instead of Green
            if quote == '"' && peek_next_non_ws(i) == Some(':') {
                spans.push(Span::styled(s, Style::default().fg(theme.syntax_type)));
            } else {
                spans.push(Span::styled(s, Style::default().fg(theme.syntax_string)));
            }
            continue;
        }

        // 5. Python/TS Decorators: @decorator
        if chars[i] == '@' {
            let mut decorator = String::new();
            decorator.push('@');
            i += 1;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                decorator.push(chars[i]);
                i += 1;
            }
            spans.push(Span::styled(
                decorator,
                Style::default().fg(theme.syntax_keyword),
            ));
            continue;
        }

        // 6. Identifier, Keywords, and Types check
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let mut word = String::new();
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                word.push(chars[i]);
                i += 1;
            }
            if is_keyword(&word) {
                spans.push(Span::styled(
                    word,
                    Style::default()
                        .fg(theme.syntax_keyword)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if is_built_in_type(&word) {
                spans.push(Span::styled(word, Style::default().fg(theme.syntax_type)));
            } else {
                spans.push(Span::styled(
                    word,
                    Style::default().fg(theme.syntax_variable),
                ));
            }
            continue;
        }

        // 7. Number check
        if chars[i].is_ascii_digit() {
            let mut num = String::new();
            while i < n
                && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i].is_alphabetic())
            {
                num.push(chars[i]);
                i += 1;
            }
            spans.push(Span::styled(num, Style::default().fg(theme.syntax_number)));
            continue;
        }

        // 8. Otherwise (operators, punctuation, whitespace)
        let mut other = String::new();
        while i < n {
            let c = chars[i];
            if c.is_alphabetic()
                || c == '_'
                || c.is_ascii_digit()
                || c == '"'
                || c == '\''
                || c == '`'
                || c == '#'
                || c == '@'
                || (c == '/' && i + 1 < n && (chars[i + 1] == '/' || chars[i + 1] == '*'))
            {
                break;
            }
            other.push(c);
            i += 1;
        }
        if !other.is_empty() {
            spans.push(Span::styled(
                other,
                Style::default().fg(theme.syntax_punctuation),
            ));
        }
    }

    spans
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "let"
            | "mut"
            | "pub"
            | "struct"
            | "impl"
            | "enum"
            | "use"
            | "mod"
            | "match"
            | "if"
            | "else"
            | "for"
            | "while"
            | "loop"
            | "in"
            | "return"
            | "break"
            | "continue"
            | "const"
            | "static"
            | "class"
            | "def"
            | "import"
            | "as"
            | "from"
            | "try"
            | "except"
            | "finally"
            | "with"
            | "self"
            | "true"
            | "false"
            | "None"
            | "null"
            | "var"
            | "function"
            | "new"
            | "typeof"
            | "instanceof"
            | "switch"
            | "case"
            | "default"
            | "type"
            | "interface"
            | "package"
            | "func"
            | "go"
            | "select"
            | "chan"
            | "nil"
            | "then"
            | "fi"
            | "done"
            | "do"
            | "elif"
            | "until"
            | "local"
            | "export"
            | "echo"
            | "printf"
            | "exit"
            | "alias"
            | "read"
            | "map"
            | "range"
    )
}

fn is_built_in_type(word: &str) -> bool {
    matches!(
        word,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "isize"
            | "usize"
            | "f32"
            | "f64"
            | "str"
            | "String"
            | "Option"
            | "Result"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
            | "bool"
            | "char"
            | "int"
            | "float"
            | "double"
            | "void"
            | "string"
            | "vector"
            | "map"
            | "list"
            | "set"
            | "Vec"
            | "HashMap"
            | "BTreeMap"
            | "HashSet"
            | "BTreeSet"
            | "Box"
            | "Rc"
            | "Arc"
            | "nil"
            | "null"
            | "undefined"
            | "error"
            | "int64"
            | "float64"
    )
}

pub fn parse_markdown_lines(
    text: &str,
    theme: &crate::tui::theme::ActiveTheme,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut is_diff = false;

    for raw_line in text.split('\n') {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block && trimmed.ends_with("diff") {
                is_diff = true;
            } else if !in_code_block {
                is_diff = false;
            }
            lines.push(Line::from(Span::styled(
                raw_line.to_owned(),
                Style::default().fg(theme.text_muted),
            )));
            continue;
        }

        if in_code_block {
            if is_diff {
                let mut style = Style::default().fg(theme.diff_context);
                if let Some(bg) = theme.diff_context_bg {
                    style = style.bg(bg);
                }
                let mut prefix = "";
                if raw_line.starts_with('+') {
                    style = Style::default().fg(theme.diff_added);
                    if let Some(bg) = theme.diff_added_bg {
                        style = style.bg(bg);
                    }
                    prefix = " ";
                } else if raw_line.starts_with('-') {
                    style = Style::default().fg(theme.diff_removed);
                    if let Some(bg) = theme.diff_removed_bg {
                        style = style.bg(bg);
                    }
                    prefix = " ";
                } else if raw_line.starts_with("@@") {
                    style = Style::default().fg(theme.diff_hunk_header);
                }
                lines.push(Line::from(Span::styled(
                    format!("{prefix}{raw_line}"),
                    style,
                )));
            } else {
                lines.push(Line::from(highlight_code_line(raw_line, theme)));
            }
            continue;
        }

        // Headers
        if trimmed.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                raw_line.to_owned(),
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::UNDERLINED)
                    .fg(theme.markdown_heading),
            )));
        } else if trimmed.starts_with("## ") || trimmed.starts_with("### ") {
            lines.push(Line::from(Span::styled(
                raw_line.to_owned(),
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(theme.markdown_heading),
            )));
        } else if trimmed.starts_with("* ")
            || trimmed.starts_with("- ")
            || (trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
                && trimmed.contains(". "))
        {
            // Lists: highlight the marker
            let mut spans = parse_inline_markdown(raw_line, theme);
            if !spans.is_empty() {
                spans[0].style = spans[0]
                    .style
                    .fg(theme.markdown_list_item)
                    .add_modifier(Modifier::BOLD);
            }
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(parse_inline_markdown(raw_line, theme)));
        }
    }

    lines
}

fn parse_inline_markdown(line: &str, theme: &crate::tui::theme::ActiveTheme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();

    let mut is_bold = false;
    let mut is_code = false;

    while let Some(c) = chars.next() {
        if c == '`' {
            if !current.is_empty() {
                spans.push(Span::styled(
                    current.clone(),
                    style_for(is_bold, is_code, theme),
                ));
                current.clear();
            }
            is_code = !is_code;
        } else if c == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consume second '*'
            if !current.is_empty() {
                spans.push(Span::styled(
                    current.clone(),
                    style_for(is_bold, is_code, theme),
                ));
                current.clear();
            }
            is_bold = !is_bold;
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, style_for(is_bold, is_code, theme)));
    }

    spans
}

fn style_for(is_bold: bool, is_code: bool, theme: &crate::tui::theme::ActiveTheme) -> Style {
    let mut style = Style::default().fg(theme.markdown_text);
    if is_bold {
        style = style.fg(theme.markdown_strong).add_modifier(Modifier::BOLD);
    }
    if is_code {
        style = style.fg(theme.markdown_code);
    }
    style
}

pub fn wrap_spans(spans: Vec<Span<'static>>, max_width: usize) -> Vec<Vec<Span<'static>>> {
    let mut wrapped_lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_width = 0;

    for span in spans {
        let text = span.content.as_ref();
        let mut char_idx = 0;
        let chars: Vec<char> = text.chars().collect();

        while char_idx < chars.len() {
            let remaining_width = max_width.saturating_sub(current_width);
            if remaining_width == 0 {
                wrapped_lines.push(current_line);
                current_line = Vec::new();
                current_width = 0;
                continue;
            }

            let take = (chars.len() - char_idx).min(remaining_width);
            let chunk: String = chars[char_idx..char_idx + take].iter().collect();
            current_line.push(Span::styled(chunk, span.style));
            current_width += take;
            char_idx += take;
        }
    }

    if !current_line.is_empty() {
        wrapped_lines.push(current_line);
    }

    if wrapped_lines.is_empty() {
        wrapped_lines.push(Vec::new());
    }

    wrapped_lines
}

pub fn wrap_line(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    let spans: Vec<Span<'static>> = line.spans.into_iter().collect();
    wrap_spans(spans, max_width)
        .into_iter()
        .map(Line::from)
        .collect()
}

pub fn wrap_lines(lines: Vec<Line<'static>>, max_width: usize) -> Vec<Line<'static>> {
    let mut wrapped = Vec::new();
    for line in lines {
        wrapped.extend(wrap_line(line, max_width));
    }
    wrapped
}

pub fn wrap_text_to_lines(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.split('\n') {
        if line.is_empty() {
            lines.push(String::new());
        } else {
            let chars: Vec<char> = line.chars().collect();
            for chunk in chars.chunks(width) {
                lines.push(chunk.iter().collect());
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::ActiveTheme;

    #[test]
    fn test_rich_highlighting() {
        let theme = ActiveTheme::default();
        // Test JSON key
        let spans = highlight_code_line("\"key\": \"value\"", &theme);
        assert_eq!(spans[0].content, "\"key\"");
        assert_eq!(spans[0].style.fg, Some(theme.syntax_type)); // JSON key styled as blue

        assert_eq!(spans[2].content, "\"value\"");
        assert_eq!(spans[2].style.fg, Some(theme.syntax_string)); // String value styled as green

        // Test Built-in type
        let spans_types = highlight_code_line("let x: usize = 42;", &theme);
        let usize_span = spans_types.iter().find(|s| s.content == "usize").unwrap();
        assert_eq!(usize_span.style.fg, Some(theme.syntax_type)); // Type styled as blue

        // Test Decorator
        let spans_decorator = highlight_code_line("@app.route('/')", &theme);
        assert_eq!(spans_decorator[0].content, "@app.route");
        assert_eq!(spans_decorator[0].style.fg, Some(theme.syntax_keyword)); // Decorator styled as magenta
    }

    #[test]
    fn test_wrap_text_to_lines() {
        let text = "hello\nworld";
        let wrapped = wrap_text_to_lines(text, 3);
        assert_eq!(wrapped, vec!["hel", "lo", "wor", "ld"]);
    }

    #[test]
    fn test_parse_inline_markdown_bold() {
        let theme = ActiveTheme::default();
        let spans = parse_inline_markdown("This is **bold** text", &theme);
        let bold_span = spans.iter().find(|s| s.content == "bold").unwrap();
        assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_parse_inline_markdown_bold_and_code() {
        let theme = ActiveTheme::default();
        let spans = parse_inline_markdown("This is **bold and `code`** text", &theme);
        let code_span = spans.iter().find(|s| s.content == "code").unwrap();
        assert!(code_span.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(code_span.style.fg, Some(theme.markdown_code));
    }

    #[test]
    fn test_parse_inline_markdown_code() {
        let theme = ActiveTheme::default();
        let spans = parse_inline_markdown("This is `code` text", &theme);
        let code_span = spans.iter().find(|s| s.content == "code").unwrap();
        assert_eq!(code_span.style.fg, Some(theme.markdown_code));
    }
}

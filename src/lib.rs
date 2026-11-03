use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

// Lexer based on Django’s:
// https://github.com/django/django/blob/main/django/template/base.py

static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\{%.*?%\}|\{\{.*?\}\}|\{#.*?#\})").unwrap());

const BLOCK_TAG_START: &str = "{%";
const VARIABLE_TAG_START: &str = "{{";
const COMMENT_TAG_START: &str = "{#";

#[derive(Debug, Clone, PartialEq)]
enum Token<'a> {
    Text {
        contents: Cow<'a, str>,
        lineno: usize,
    },
    Variable {
        filter_expression: FilterExpression<'a>,
        lineno: usize,
    },
    Block {
        bits: Vec<Cow<'a, str>>,
        lineno: usize,
    },
    Comment {
        contents: Cow<'a, str>,
        lineno: usize,
    },
}

fn lex<'a>(template_string: &'a str) -> Vec<Token<'a>> {
    let mut result = Vec::new();
    let mut literal_until = None;
    let mut lineno = 1;
    let mut last_end = 0;

    for cap in TAG_RE.captures_iter(template_string) {
        let token_match = cap.get(0).unwrap();
        let (start, end) = (token_match.start(), token_match.end());

        if start > last_end {
            let text = &template_string[last_end..start];
            result.push(create_token(text, lineno, false, &mut literal_until));
            lineno += text.matches('\n').count();
        }

        let token_string = token_match.as_str();
        result.push(create_token(token_string, lineno, true, &mut literal_until));
        lineno += token_string.matches('\n').count();

        last_end = end;
    }

    if last_end < template_string.len() {
        let text = &template_string[last_end..];
        result.push(create_token(text, lineno, false, &mut literal_until));
    }

    result
}

fn create_token<'a>(
    token_string: &'a str,
    lineno: usize,
    in_tag: bool,
    literal_until: &mut Option<String>,
) -> Token<'a> {
    if in_tag && !has_jinja_whitespace_control(token_string) {
        let content = token_string[2..token_string.len() - 2].trim();
        if content.is_empty() && !token_string.starts_with(COMMENT_TAG_START) {
            // Django rejects empty block and variable tags, so leave them.
            return Token::Text {
                contents: Cow::Borrowed(token_string),
                lineno,
            };
        }
        if token_string.starts_with(BLOCK_TAG_START) {
            if let Some(end) = &literal_until {
                if content != end {
                    return Token::Text {
                        contents: Cow::Borrowed(token_string),
                        lineno,
                    };
                }
                *literal_until = None;
            } else if content.starts_with("verbatim") {
                *literal_until = Some(format!("end{}", content));
            } else if content.split_whitespace().next() == Some("comment") {
                // Django skips everything up to the first {% endcomment %}.
                *literal_until = Some("endcomment".to_string());
            }
            Token::Block {
                bits: split_contents(content),
                lineno,
            }
        } else if literal_until.is_none() {
            if token_string.starts_with(VARIABLE_TAG_START) {
                Token::Variable {
                    filter_expression: lex_filter_expression(content),
                    lineno,
                }
            } else {
                debug_assert!(token_string.starts_with(COMMENT_TAG_START));
                Token::Comment {
                    contents: Cow::Borrowed(content),
                    lineno,
                }
            }
        } else {
            Token::Text {
                contents: Cow::Borrowed(token_string),
                lineno,
            }
        }
    } else {
        Token::Text {
            contents: Cow::Borrowed(token_string),
            lineno,
        }
    }
}

/// Detect Jinja-style whitespace control, like `{%- if x -%}`, which Django
/// does not support. Such tags are left unchanged, rather than mangled.
fn has_jinja_whitespace_control(token_string: &str) -> bool {
    let bytes = token_string.as_bytes();
    bytes.len() >= 5
        && (matches!(bytes[2], b'-' | b'+') || matches!(bytes[bytes.len() - 3], b'-' | b'+'))
}

// Expression lexer based on Django’s FilterExpression:
// https://github.com/django/django/blob/ad7f8129f3d2de937611d72e257fb07d1306a855/django/template/base.py#L617

static FILTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    let constant_string = format!(
        r#"(?x)
        (?:{i18n_open}{strdq}{i18n_close}|
           {i18n_open}{strsq}{i18n_close}|
           {strdq}|
           {strsq})
    "#,
        strdq = r#""[^"\\]*(?:\\.[^"\\]*)*""#,
        strsq = r#"'[^'\\]*(?:\\.[^'\\]*)*'"#,
        i18n_open = regex::escape("_("),
        i18n_close = regex::escape(")"),
    );

    regex::RegexBuilder::new(&format!(
        r#"(?x)
        ^(?P<constant>{constant})|
        ^(?P<var>[{var_chars}]+)|
         (?:\s*{filter_sep}\s*
             (?P<filter_name>\w+)
                 (?:{arg_sep}
                     (?:
                      (?P<constant_arg>{constant})|
                      (?P<var_arg>[{var_chars}]+)
                     )
                 )?
         )"#,
        constant = constant_string,
        var_chars = r"\w\.\+-",
        filter_sep = regex::escape("|"),
        arg_sep = regex::escape(":"),
    ))
    .build()
    .unwrap()
});

#[derive(Debug, Clone, PartialEq)]
enum Expression<'a> {
    Constant(Cow<'a, str>),
    Variable(Cow<'a, str>),
    Unparsed(Cow<'a, str>),
}

#[derive(Debug, Clone, PartialEq)]
struct FilterExpression<'a> {
    var: Expression<'a>,
    filters: Vec<Filter<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
struct Filter<'a> {
    name: Cow<'a, str>,
    arg: Option<Expression<'a>>,
}

fn lex_filter_expression<'a>(expr: &'a str) -> FilterExpression<'a> {
    let mut filter_expression = FilterExpression {
        var: Expression::Unparsed(Cow::Borrowed(expr)),
        filters: Vec::new(),
    };
    let mut upto = 0;
    let mut variable = false;
    for captures in FILTER_RE.captures_iter(expr) {
        let start = captures.get(0).unwrap().start();
        if upto != start {
            // Syntax error - ignore it and return whole expression as constant
            return FilterExpression {
                var: Expression::Unparsed(Cow::Borrowed(expr)),
                filters: Vec::new(),
            };
        }

        if !variable {
            if let Some(constant) = captures.name("constant") {
                filter_expression.var = Expression::Constant(Cow::Borrowed(constant.as_str()));
            } else if let Some(variable) = captures.name("var") {
                filter_expression.var = Expression::Variable(Cow::Borrowed(variable.as_str()));
            }
            variable = true;
        } else {
            let filter_name = Cow::Borrowed(captures.name("filter_name").unwrap().as_str());
            if let Some(constant_arg) = captures.name("constant_arg") {
                filter_expression.filters.push(Filter {
                    name: filter_name,
                    arg: Some(Expression::Constant(Cow::Borrowed(constant_arg.as_str()))),
                });
            } else if let Some(var_arg) = captures.name("var_arg") {
                filter_expression.filters.push(Filter {
                    name: filter_name,
                    arg: Some(Expression::Variable(Cow::Borrowed(var_arg.as_str()))),
                });
            } else {
                filter_expression.filters.push(Filter {
                    name: filter_name,
                    arg: None,
                });
            }
        }
        upto = captures.get(0).unwrap().end();
    }
    if upto != expr.len() {
        // Syntax error - ignore it and return whole expression as constant
        return FilterExpression {
            var: Expression::Unparsed(Cow::Borrowed(expr)),
            filters: Vec::new(),
        };
    }
    filter_expression
}

static SMART_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)
        ((?:
            [^\s'"]*
            (?:
                (?:"(?:[^"\\]|\\.)*" | '(?:[^'\\]|\\.)*')
                [^\s'"]*
            )+
        ) | \S+)"#,
    )
    .unwrap()
});

fn smart_split<'a>(text: &'a str) -> Vec<Cow<'a, str>> {
    SMART_SPLIT_RE
        .captures_iter(text)
        .map(|cap| Cow::Borrowed(cap.get(0).unwrap().as_str()))
        .collect()
}

fn split_contents<'a>(contents: &'a str) -> Vec<Cow<'a, str>> {
    let mut split = Vec::new();
    let mut bits = smart_split(contents).into_iter();

    while let Some(bit) = bits.next() {
        let mut chars = bit.chars();
        if matches!(chars.next(), Some('_'))
            && matches!(chars.next(), Some('('))
            && matches!(chars.next(), Some('"' | '\''))
        {
            let sentinel = format!("{})", &bit[2..3]);
            let mut trans_bit = vec![bit];
            while !trans_bit.last().unwrap().ends_with(&sentinel) {
                if let Some(next_bit) = bits.next() {
                    trans_bit.push(next_bit);
                } else {
                    break;
                }
            }
            let joined = trans_bit
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join(" ");
            split.push(Cow::Owned(joined));
        } else {
            split.push(bit);
        }
    }
    split
}

pub fn format(content: &str, target_version: Option<(u8, u8)>) -> String {
    // Lex
    let newline = detect_newline(content);
    let mut tokens = lex(content);

    // Fixers
    migrate_length_is(&mut tokens, target_version);
    migrate_empty_json_script(&mut tokens, target_version);
    migrate_translation_tags(&mut tokens, target_version);
    migrate_ifequal_tags(&mut tokens, target_version);
    migrate_static_load_tags(&mut tokens, target_version);
    migrate_assignments(&mut tokens);

    // Formatters
    update_leading_trailing_whitespace(&mut tokens, newline);
    update_load_tags(&mut tokens);
    update_endblock_and_endpartialdef_labels(&mut tokens);
    update_top_level_block_indentation(&mut tokens);
    update_top_level_block_spacing(&mut tokens, newline);

    // Final build
    let mut result = String::new();
    for token in tokens {
        match token {
            Token::Text { contents, .. } => result.push_str(&contents),
            Token::Variable {
                filter_expression, ..
            } => {
                result.push_str("{{ ");
                format_variable(filter_expression, &mut result);
                result.push_str(" }}");
            }
            Token::Block { bits, .. } => {
                result.push_str("{% ");
                result.push_str(&bits.join(" "));
                result.push_str(" %}");
            }
            Token::Comment { contents, .. } => {
                result.push_str("{# ");
                result.push_str(&contents);
                result.push_str(" #}");
            }
        }
    }
    result
}

fn detect_newline(content: &str) -> &str {
    // Prefer newlines outside of leading blank lines and trailing whitespace,
    // which are rewritten, so the result is the same when formatting again.
    let kept =
        content[leading_blank_lines_len(content)..].trim_end_matches([' ', '\t', '\r', '\n']);
    let content = if kept.contains('\n') { kept } else { content };
    match content.split_once('\n') {
        Some((s, _)) if s.ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// Length of the blank lines at the start of `s`, which may contain spaces,
/// tabs, and carriage returns.
fn leading_blank_lines_len(s: &str) -> usize {
    let mut pos = 0;

    loop {
        let start = pos;
        let chars = s[pos..].chars();

        for c in chars {
            if matches!(c, ' ' | '\t' | '\r') {
                pos += c.len_utf8();
            } else {
                break;
            }
        }

        if s[pos..].starts_with('\n') {
            pos += 1;
        } else {
            return start;
        }
    }
}

#[inline(always)]
fn format_variable(filter_expression: FilterExpression, result: &mut String) {
    match filter_expression.var {
        Expression::Constant(value) | Expression::Variable(value) | Expression::Unparsed(value) => {
            result.push_str(&value);
        }
    }
    for filter in filter_expression.filters {
        result.push('|');
        result.push_str(&filter.name);
        if let Some(arg) = filter.arg {
            result.push(':');
            match arg {
                Expression::Constant(value)
                | Expression::Variable(value)
                | Expression::Unparsed(value) => {
                    result.push_str(&value);
                }
            }
        }
    }
}

// Fixers

static LENGTH_IS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([\w.]+)\|length_is:(\w+)$").unwrap());

fn migrate_length_is(tokens: &mut [Token<'_>], target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (4, 2) {
        return;
    }

    for token in tokens.iter_mut() {
        if let Token::Block { bits, .. } = token {
            if bits[0] != "if" && bits[0] != "elif" {
                continue;
            }
            if !bits.iter().any(|bit| LENGTH_IS_RE.is_match(bit)) {
                continue;
            }

            let mut new_bits = Vec::with_capacity(bits.len() + 2);
            for i in 0..bits.len() {
                // Comparison operators bind tighter than 'and', 'or', and
                // 'not', so the rewrite is only safe next to them.
                let safe_before = i > 0
                    && match bits[i - 1].as_ref() {
                        "if" | "elif" | "and" | "or" => true,
                        "not" => i < 2 || bits[i - 2] != "is",
                        _ => false,
                    };
                let safe_after = i + 1 == bits.len() || bits[i + 1] == "and" || bits[i + 1] == "or";

                if safe_before
                    && safe_after
                    && let Some(captures) = LENGTH_IS_RE.captures(bits[i].as_ref())
                {
                    let var1 = captures.get(1).unwrap().as_str();
                    let var2 = captures.get(2).unwrap().as_str();
                    new_bits.push(Cow::Owned(format!("{}|length", var1)));
                    new_bits.push(Cow::Borrowed("=="));
                    new_bits.push(Cow::Owned(var2.to_string()));
                } else {
                    new_bits.push(bits[i].clone());
                }
            }
            *bits = new_bits;
        }
    }
}

fn migrate_empty_json_script(tokens: &mut [Token<'_>], target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (4, 1) {
        return;
    }

    for token in tokens.iter_mut() {
        if let Token::Variable {
            filter_expression, ..
        } = token
        {
            for filter in &mut filter_expression.filters {
                if filter.name == "json_script"
                    && let Some(Expression::Constant(arg)) = &filter.arg
                    && (arg == "\"\"" || arg == "''")
                {
                    filter.arg = None;
                }
            }
        }
    }
}

fn migrate_translation_tags(tokens: &mut [Token<'_>], target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (3, 1) {
        return;
    }

    for token in tokens.iter_mut() {
        if let Token::Block { bits, .. } = token {
            match bits[0].as_ref() {
                "trans" => {
                    bits[0] = Cow::Borrowed("translate");
                }
                "blocktrans" => {
                    bits[0] = Cow::Borrowed("blocktranslate");
                }
                "endblocktrans" => {
                    bits[0] = Cow::Borrowed("endblocktranslate");
                }
                "load"
                    if bits.len() >= 4
                        && bits[bits.len() - 2] == "from"
                        && bits[bits.len() - 1] == "i18n" =>
                {
                    for i in 1..bits.len() - 2 {
                        if bits[i] == "trans" {
                            bits[i] = Cow::Borrowed("translate");
                        } else if bits[i] == "blocktrans" {
                            bits[i] = Cow::Borrowed("blocktranslate");
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn migrate_ifequal_tags(tokens: &mut [Token<'_>], target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (3, 1) {
        return;
    }

    // First pass: find matching pairs
    let mut stack = Vec::new();
    let mut pairs = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if let Token::Block { bits, .. } = token {
            match bits[0].as_ref() {
                "ifequal" | "ifnotequal" => {
                    if bits.len() == 3 {
                        stack.push(i)
                    }
                }
                "endifequal" | "endifnotequal" => {
                    if let Some(start) = stack.pop()
                        && bits.len() == 1
                    {
                        pairs.push((start, i));
                    }
                }
                _ => {}
            }
        }
    }

    // Second pass: update pairs
    for (start, end) in pairs.into_iter().rev() {
        if let (
            Some(Token::Block {
                bits: start_bits, ..
            }),
            Some(Token::Block { .. }),
        ) = (tokens.get(start), tokens.get(end))
            && start_bits.len() >= 3
        {
            let comparison = if start_bits[0] == "ifequal" {
                "=="
            } else {
                "!="
            };
            let var1 = start_bits[1].clone();
            let var2 = start_bits[2].clone();

            // Update start token
            if let Token::Block { bits, .. } = &mut tokens[start] {
                bits.clear();
                bits.push(Cow::Borrowed("if"));
                bits.push(var1);
                bits.push(Cow::Borrowed(comparison));
                bits.push(var2);
            }

            // Update end token
            if let Token::Block { bits, .. } = &mut tokens[end] {
                bits.clear();
                bits.push(Cow::Borrowed("endif"));
            }
        }
    }
}

fn migrate_static_load_tags(tokens: &mut [Token<'_>], target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (2, 1) {
        return;
    }

    for token in tokens.iter_mut() {
        if let Token::Block { bits, .. } = token
            && bits[0] == "load"
        {
            if bits.iter().any(|b| b == "from") {
                if bits.len() >= 4 && bits[bits.len() - 2] == "from" {
                    let last = bits.len() - 1;
                    let library = bits[last].as_ref();
                    if library == "admin_static" || library == "staticfiles" {
                        bits[last] = Cow::Borrowed("static");
                    }
                }
            } else {
                for bit in bits.iter_mut().skip(1) {
                    if bit == "admin_static" || bit == "staticfiles" {
                        *bit = Cow::Borrowed("static");
                    }
                }
            }
        }
    }
}

fn migrate_assignments(tokens: &mut [Token<'_>]) {
    for token in tokens.iter_mut() {
        if let Token::Block { bits, .. } = token
            && bits.iter().any(|b| b == "as")
        {
            match bits[0].as_ref() {
                "with" => migrate_assignments_with_tag(bits),
                "blocktrans" | "blocktranslate" => migrate_assignments_blocktranslate_tag(bits),
                _ => {}
            }
        }
    }
}

fn migrate_assignments_with_tag(bits: &mut Vec<Cow<'_, str>>) {
    let mut new_bits = vec![Cow::Borrowed("with")];
    let mut i = 1;
    while i < bits.len() {
        if i + 2 < bits.len() && bits[i + 1] == "as" {
            // Legacy format: "value as key"
            new_bits.push(Cow::Owned(format!("{}={}", bits[i + 2], bits[i])));
            i += 3;
        } else if bits[i].contains('=') {
            // Modern format: "key=value"
            new_bits.push(bits[i].clone());
            i += 1;
        } else {
            // Not a keyword argument, stop processing
            new_bits.extend(bits[i..].iter().cloned());
            break;
        }

        // Check for "and" between arguments
        if i < bits.len() && bits[i] == "and" {
            i += 1;
        }
    }
    *bits = new_bits;
}

fn migrate_assignments_blocktranslate_tag(bits: &mut Vec<Cow<'_, str>>) {
    let mut new_bits = vec![bits[0].clone()];
    let mut i = 1;
    let mut found_with = false;
    let mut found_count = false;
    while i < bits.len() {
        match bits[i].as_ref() {
            "with" if !found_with => {
                found_with = true;
                new_bits.push(Cow::Borrowed("with"));
                i += 1;
                while i < bits.len() {
                    if i + 2 < bits.len() && bits[i + 1] == "as" {
                        // Legacy format: "value as key"
                        new_bits.push(Cow::Owned(format!("{}={}", bits[i + 2], bits[i])));
                        i += 3;
                    } else if bits[i].contains('=') {
                        // Modern format: "key=value"
                        new_bits.push(bits[i].clone());
                        i += 1;
                    } else {
                        break;
                    }
                    // Check for "and" between arguments
                    if i < bits.len() && bits[i] == "and" {
                        i += 1;
                    }
                }
            }
            "count" if !found_count => {
                found_count = true;
                new_bits.push(Cow::Borrowed("count"));
                i += 1;
                if i < bits.len() {
                    if i + 2 < bits.len() && bits[i + 1] == "as" {
                        // Legacy format: "value as count"
                        new_bits.push(Cow::Owned(format!("{}={}", bits[i + 2], bits[i])));
                        i += 2;
                    } else if bits[i].contains('=') {
                        // Modern format: "key=value"
                        new_bits.push(bits[i].clone());
                    }
                }
                i += 1;
            }
            _ => {
                new_bits.push(bits[i].clone());
                i += 1;
            }
        }
    }
    *bits = new_bits;
}

// Formatters

fn update_leading_trailing_whitespace<'a>(tokens: &mut Vec<Token<'a>>, newline: &str) {
    if let Some(Token::Text { contents, .. }) = tokens.first_mut() {
        let s = contents.as_ref();
        let pos = leading_blank_lines_len(s);

        if pos > 0 {
            *contents = Cow::Owned(s[pos..].to_string());
        }
    }

    if let Some(mut token) = tokens.last_mut() {
        if let Token::Text { contents, .. } = &mut token {
            let s = contents.as_ref();
            let mut end = s.len();

            for (idx, c) in s.char_indices().rev() {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    end = idx;
                } else {
                    break;
                }
            }

            let trimmed = &s[..end];
            let remainder = &s[end..];
            let already_correct = end == s.len() && newline.is_empty() || remainder == newline;

            if !already_correct {
                *contents = Cow::Owned(format!("{}{}", trimmed, newline));
            }
        } else {
            tokens.push(Token::Text {
                contents: Cow::Owned(newline.to_string()),
                lineno: 0,
            });
        }
    }
}

fn update_load_tags<'a>(tokens: &mut Vec<Token<'a>>) {
    let mut i = 0;
    while i < tokens.len() {
        if let Token::Block { ref bits, .. } = tokens[i]
            && bits[0] == "load"
        {
            // load ... from ...
            if bits.iter().any(|b| b == "from") {
                if bits.len() >= 4 && bits[bits.len() - 2] == "from" {
                    let library = bits[bits.len() - 1].clone();
                    let mut parts = bits[1..bits.len() - 2].to_vec();

                    parts.sort_unstable();
                    parts.dedup();
                    parts.insert(0, Cow::Borrowed("load"));
                    parts.push(Cow::Borrowed("from"));
                    parts.push(library);

                    if let Token::Block { bits, .. } = &mut tokens[i] {
                        bits.clear();
                        bits.extend(parts);
                    }
                }
            // load ...
            } else {
                let mut j = i + 1;
                let mut to_merge = vec![i];
                while j < tokens.len() {
                    match &tokens[j] {
                        Token::Text { contents, .. } if contents.trim().is_empty() => j += 1,
                        Token::Block { bits, .. } if bits[0] == "load" => {
                            if bits.iter().any(|b| b == "from") {
                                break;
                            }
                            to_merge.push(j);
                            j += 1
                        }
                        _ => break,
                    }
                }
                if j > 0
                    && let Token::Text { .. } = tokens[j - 1]
                {
                    j -= 1;
                }

                let mut parts: Vec<Cow<'_, str>> = to_merge
                    .iter()
                    .filter_map(|&idx| {
                        if let Token::Block { bits, .. } = &tokens[idx] {
                            Some(bits.iter().skip(1).cloned().collect::<Vec<_>>())
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .collect();

                parts.sort_unstable();
                parts.dedup();
                parts.insert(0, Cow::Borrowed("load"));

                if let Token::Block { bits, .. } = &mut tokens[i] {
                    bits.clear();
                    bits.extend(parts);
                }

                tokens.drain(i + 1..j);
            }
        }

        i += 1;
    }
}

fn update_endblock_and_endpartialdef_labels<'a>(tokens: &mut [Token<'a>]) {
    let mut block_stack = Vec::new();
    let mut partialdef_stack = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let update = match &tokens[i] {
            Token::Block { bits, lineno } if bits[0] == "block" => {
                let label = bits.get(1).cloned();
                block_stack.push((label, *lineno));
                None
            }
            Token::Block { bits, lineno } if bits[0] == "endblock" => {
                if let Some((Some(label), start_lineno)) = block_stack.pop() {
                    let same_line = start_lineno == *lineno;
                    if same_line && bits.len() == 2 {
                        Some(vec![Cow::Borrowed("endblock")])
                    } else if !same_line && bits.len() == 1 {
                        Some(vec![Cow::Borrowed("endblock"), label])
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Token::Block { bits, lineno } if bits[0] == "partialdef" => {
                let label = bits.get(1).cloned();
                partialdef_stack.push((label, *lineno));
                None
            }
            Token::Block { bits, lineno } if bits[0] == "endpartialdef" => {
                if let Some((Some(label), start_lineno)) = partialdef_stack.pop() {
                    let same_line = start_lineno == *lineno;
                    if same_line && bits.len() == 2 {
                        Some(vec![Cow::Borrowed("endpartialdef")])
                    } else if !same_line && bits.len() == 1 {
                        Some(vec![Cow::Borrowed("endpartialdef"), label])
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(new_bits) = update
            && let Token::Block { lineno, .. } = tokens[i]
        {
            tokens[i] = Token::Block {
                bits: new_bits,
                lineno,
            };
        }
        i += 1;
    }
}

fn update_top_level_block_indentation<'a>(tokens: &mut [Token<'a>]) {
    let mut after_extends = false;
    let mut block_depth = 0;

    for i in 0..tokens.len() {
        match &tokens[i] {
            Token::Block { bits, .. } => {
                if bits[0] == "extends" {
                    after_extends = true;
                    unindent_token(tokens, i);
                } else if bits[0] == "block" {
                    if after_extends && block_depth == 0 {
                        unindent_token(tokens, i);
                    }
                    block_depth += 1;
                } else if bits[0] == "endblock" {
                    block_depth -= 1;
                    if after_extends && block_depth == 0 {
                        unindent_token(tokens, i);
                    }
                } else if block_depth == 0 {
                    return;
                }
            }
            _ => continue,
        }
    }
}

static INDENTATION_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]+$\z").unwrap());

fn unindent_token<'a>(tokens: &mut [Token<'a>], index: usize) {
    if index > 0
        && let Token::Text { contents, .. } = &mut tokens[index - 1]
    {
        let unindented = INDENTATION_LINE.replace_all(contents, "");
        if unindented != *contents {
            *contents = Cow::Owned(unindented.to_string());
        }
    }
}

fn update_top_level_block_spacing<'a>(tokens: &mut [Token<'a>], newline: &str) {
    let mut has_extends = false;
    let mut depth = 0;
    let mut last_top_level_tag = None;
    let mut i = 0;

    while i < tokens.len() {
        if let Token::Block { bits, .. } = &tokens[i] {
            match bits[0].as_ref() {
                "extends" => {
                    has_extends = true;
                    last_top_level_tag = Some(i);
                }
                "block" => {
                    if has_extends && depth == 0 {
                        if let Some(last_end) = last_top_level_tag
                            && last_end == i - 2
                            && let Token::Text { contents, .. } = &mut tokens[i - 1]
                            && contents.trim().is_empty()
                        {
                            let double_newline = format!("{}{}", newline, newline);
                            if contents.as_ref() != double_newline {
                                *contents = Cow::Owned(double_newline);
                            }
                        }
                        last_top_level_tag = Some(i);
                    }
                    depth += 1;
                }
                "endblock" => {
                    depth -= 1;
                    if has_extends && depth == 0 {
                        last_top_level_tag = Some(i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // detect_newline

    #[test]
    fn test_detect_newline_defaults_to_line_feed() {
        assert_eq!(detect_newline(""), "\n");
    }

    #[test]
    fn test_detect_newline_with_carriage_return_first() {
        assert_eq!(detect_newline("foo\r\nbar\n"), "\r\n");
    }

    #[test]
    fn test_detect_newline_with_line_feed_first() {
        assert_eq!(detect_newline("foo\nbar\r\n"), "\n");
    }

    #[test]
    fn test_detect_newline_ignores_leading_blank_lines() {
        assert_eq!(detect_newline("\n{% a %}\r\nb"), "\r\n");
        assert_eq!(detect_newline(" \r\n\t\na\nb"), "\n");
    }

    #[test]
    fn test_detect_newline_ignores_trailing_whitespace() {
        assert_eq!(detect_newline("a\r\nb\n"), "\r\n");
        assert_eq!(detect_newline("\r\na\nb \r\n"), "\n");
    }

    #[test]
    fn test_detect_newline_only_leading_blank_lines() {
        assert_eq!(detect_newline("\r\na"), "\r\n");
        assert_eq!(detect_newline("\na"), "\n");
    }

    #[test]
    fn test_format_mixed_newlines_idempotent() {
        let formatted = format("\n{% a %}\r\nb", None);
        assert_eq!(formatted, "{% a %}\r\nb\r\n");
        assert_eq!(format(&formatted, None), formatted);
    }

    // Fixers

    // migrate_length_is

    #[test]
    fn test_length_is_not_migrated_old_django() {
        let formatted = format("{% if eggs|length_is:1 %}{% endif %}\n", Some((4, 1)));
        assert_eq!(formatted, "{% if eggs|length_is:1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_migrated() {
        let formatted = format("{% if eggs|length_is:1 %}{% endif %}\n", Some((4, 2)));
        assert_eq!(formatted, "{% if eggs|length == 1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_not_migrated_when_no_version_specified() {
        let formatted = format("{% if eggs|length_is:1 %}{% endif %}\n", None);
        assert_eq!(formatted, "{% if eggs|length_is:1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_migrated_with_variable() {
        let formatted = format("{% if eggs|length_is:n %}{% endif %}\n", Some((4, 2)));
        assert_eq!(formatted, "{% if eggs|length == n %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_migrated_with_complex_variable() {
        let formatted = format(
            "{% if basket.eggs|length_is:1 %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(formatted, "{% if basket.eggs|length == 1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_not_migrated_in_variable_tag() {
        let formatted = format("{{ eggs|length_is:1 }}\n", Some((4, 2)));
        assert_eq!(formatted, "{{ eggs|length_is:1 }}\n");
    }

    #[test]
    fn test_length_is_migrated_with_and() {
        let formatted = format(
            "{% if eggs|length_is:1 and spam %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(formatted, "{% if eggs|length == 1 and spam %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_migrated_after_and() {
        let formatted = format(
            "{% if spam and eggs|length_is:1 %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(formatted, "{% if spam and eggs|length == 1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_migrated_multiple() {
        let formatted = format(
            "{% if eggs|length_is:1 and spam|length_is:n %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(
            formatted,
            "{% if eggs|length == 1 and spam|length == n %}{% endif %}\n"
        );
    }

    #[test]
    fn test_length_is_migrated_with_or() {
        let formatted = format(
            "{% if spam or eggs|length_is:1 %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(formatted, "{% if spam or eggs|length == 1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_migrated_with_not() {
        let formatted = format("{% if not eggs|length_is:1 %}{% endif %}\n", Some((4, 2)));
        assert_eq!(formatted, "{% if not eggs|length == 1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_migrated_elif() {
        let formatted = format(
            "{% if spam %}{% elif eggs|length_is:1 and ham %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(
            formatted,
            "{% if spam %}{% elif eggs|length == 1 and ham %}{% endif %}\n"
        );
    }

    #[test]
    fn test_length_is_not_migrated_with_comparison() {
        let formatted = format(
            "{% if eggs|length_is:1 == spam %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(formatted, "{% if eggs|length_is:1 == spam %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_not_migrated_after_comparison() {
        let formatted = format(
            "{% if spam != eggs|length_is:1 %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(formatted, "{% if spam != eggs|length_is:1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_not_migrated_after_is_not() {
        let formatted = format(
            "{% if spam is not eggs|length_is:1 %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(
            formatted,
            "{% if spam is not eggs|length_is:1 %}{% endif %}\n"
        );
    }

    #[test]
    fn test_length_is_not_migrated_after_in() {
        let formatted = format(
            "{% if spam in eggs|length_is:1 %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(formatted, "{% if spam in eggs|length_is:1 %}{% endif %}\n");
    }

    #[test]
    fn test_length_is_not_migrated_with_further_filter() {
        let formatted = format(
            "{% if eggs|length_is:1|yesno:'a,' %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(
            formatted,
            "{% if eggs|length_is:1|yesno:'a,' %}{% endif %}\n"
        );
    }

    #[test]
    fn test_length_is_not_migrated_in_other_tag() {
        let formatted = format("{% firstof eggs|length_is:1 %}\n", Some((4, 2)));
        assert_eq!(formatted, "{% firstof eggs|length_is:1 %}\n");
    }

    // migrate_empty_json_script

    #[test]
    fn test_migrate_empty_json_script_double_quotes() {
        let formatted = format("{{ egg_data|json_script:\"\" }}\n", Some((4, 1)));
        assert_eq!(formatted, "{{ egg_data|json_script }}\n");
    }

    #[test]
    fn test_migrate_empty_json_script_single_quotes() {
        let formatted = format("{{ egg_data|json_script:'' }}\n", Some((4, 1)));
        assert_eq!(formatted, "{{ egg_data|json_script }}\n");
    }

    #[test]
    fn test_migrate_empty_json_script_not_empty() {
        let formatted = format("{{ egg_data|json_script:'egg_id' }}\n", Some((4, 1)));
        assert_eq!(formatted, "{{ egg_data|json_script:'egg_id' }}\n");
    }

    #[test]
    fn test_migrate_empty_json_script_old_django() {
        let formatted = format("{{ egg_data|json_script:\"\" }}\n", Some((4, 0)));
        assert_eq!(formatted, "{{ egg_data|json_script:\"\" }}\n");
    }

    #[test]
    fn test_migrate_empty_json_script_no_version() {
        let formatted = format("{{ egg_data|json_script:\"\" }}\n", None);
        assert_eq!(formatted, "{{ egg_data|json_script:\"\" }}\n");
    }

    #[test]
    fn test_migrate_empty_json_script_after_another_filter() {
        let formatted = format("{{ egg_data|upper|json_script:\"\" }}\n", Some((4, 1)));
        assert_eq!(formatted, "{{ egg_data|upper|json_script }}\n");
    }

    #[test]
    fn test_migrate_empty_json_script_before_another_filter() {
        let formatted = format("{{ egg_data|json_script:\"\"|safe }}\n", Some((4, 1)));
        assert_eq!(formatted, "{{ egg_data|json_script|safe }}\n");
    }

    // migrate_ifequal_tags

    #[test]
    fn test_format_ifequal_old_django_not_migrated() {
        let formatted = format("{% ifequal a b %}\n{% endifequal %}\n", Some((3, 0)));
        assert_eq!(formatted, "{% ifequal a b %}\n{% endifequal %}\n");
    }

    #[test]
    fn test_format_ifequal_too_few_args_not_migrated() {
        let formatted = format("{% ifequal a %}\n{% endifequal %}\n", Some((3, 1)));
        assert_eq!(formatted, "{% ifequal a %}\n{% endifequal %}\n");
    }

    #[test]
    fn test_format_ifequal_too_many_args_not_migrated() {
        let formatted = format("{% ifequal a b c %}\n{% endifequal %}\n", Some((3, 1)));
        assert_eq!(formatted, "{% ifequal a b c %}\n{% endifequal %}\n");
    }

    #[test]
    fn test_format_ifequal_incorrect_pairing_start_not_migrated() {
        let formatted = format("{% ifequal a b %}\n{% endif %}\n", Some((3, 1)));
        assert_eq!(formatted, "{% ifequal a b %}\n{% endif %}\n");
    }

    #[test]
    fn test_format_ifequal_incorrect_pairing_end_not_migrated() {
        let formatted = format("{% if a == b %}\n{% endifequal %}\n", Some((3, 1)));
        assert_eq!(formatted, "{% if a == b %}\n{% endifequal %}\n");
    }

    #[test]
    fn test_format_ifequal_migrated() {
        let formatted = format("{% ifequal a b %}\n{% endifequal %}\n", Some((3, 1)));
        assert_eq!(formatted, "{% if a == b %}\n{% endif %}\n");
    }

    #[test]
    fn test_format_ifequal_migrated_constant() {
        let formatted = format(
            "{% ifequal a 'the golden goose' %}\n{% endifequal %}\n",
            Some((3, 1)),
        );
        assert_eq!(formatted, "{% if a == 'the golden goose' %}\n{% endif %}\n");
    }

    #[test]
    fn test_format_ifequal_migrated_with_dots() {
        let formatted = format(
            "{% ifequal user.name author.name %}\n{% endifequal %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% if user.name == author.name %}\n{% endif %}\n"
        );
    }

    #[test]
    fn test_format_ifequal_migrated_with_filters() {
        let formatted = format(
            "{% ifequal user.name|lower 'admin' %}\n{% endifequal %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% if user.name|lower == 'admin' %}\n{% endif %}\n"
        );
    }

    #[test]
    fn test_format_ifnotequal_old_django_not_migrated() {
        let formatted = format("{% ifnotequal a b %}\n{% endifnotequal %}\n", Some((3, 0)));
        assert_eq!(formatted, "{% ifnotequal a b %}\n{% endifnotequal %}\n");
    }

    #[test]
    fn test_format_ifnotequal_migrated() {
        let formatted = format("{% ifnotequal a b %}\n{% endifnotequal %}\n", Some((3, 1)));
        assert_eq!(formatted, "{% if a != b %}\n{% endif %}\n");
    }

    #[test]
    fn test_format_ifequal_migrated_with_translated_string() {
        let formatted = format(
            "{% ifequal message _('Welcome') %}\n{% endifequal %}\n",
            Some((3, 1)),
        );
        assert_eq!(formatted, "{% if message == _('Welcome') %}\n{% endif %}\n");
    }

    #[test]
    fn test_format_ifequal_nested_migrated() {
        let formatted = format(
            "{% ifequal a b %}\n{% ifnotequal b c %}\n{% endifnotequal %}\n{% endifequal %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% if a == b %}\n{% if b != c %}\n{% endif %}\n{% endif %}\n"
        );
    }

    // migrate_translation_tags

    #[test]
    fn test_trans_not_migrated_old_django() {
        let formatted = format(
            "{% load trans from i18n %}\n{% trans 'Hello' %}\n",
            Some((3, 0)),
        );
        assert_eq!(
            formatted,
            "{% load trans from i18n %}\n{% trans 'Hello' %}\n"
        );
    }

    #[test]
    fn test_trans_migrated() {
        let formatted = format(
            "{% load trans from i18n %}\n{% trans 'Hello' %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% load translate from i18n %}\n{% translate 'Hello' %}\n"
        );
    }

    #[test]
    fn test_blocktrans_not_migrated_old_django() {
        let formatted = format(
            "{% load blocktrans from i18n %}\n{% blocktrans %}Hello{% endblocktrans %}\n",
            Some((3, 0)),
        );
        assert_eq!(
            formatted,
            "{% load blocktrans from i18n %}\n{% blocktrans %}Hello{% endblocktrans %}\n"
        );
    }

    #[test]
    fn test_blocktrans_migrated() {
        let formatted = format(
            "{% load blocktrans from i18n %}\n{% blocktrans %}Hello{% endblocktrans %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% load blocktranslate from i18n %}\n{% blocktranslate %}Hello{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_blocktrans_with_args_migrated() {
        let formatted = format(
            "{% blocktrans with name='John' %}Hello {{ name }}{% endblocktrans %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with name='John' %}Hello {{ name }}{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_multiple_translation_tags_migrated() {
        let formatted = format(
            "{% trans 'Hello' %}\n{% blocktrans %}World{% endblocktrans %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% translate 'Hello' %}\n{% blocktranslate %}World{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_translation_tags_not_migrated_when_no_version_specified() {
        let formatted = format(
            "{% trans 'Hello' %}\n{% blocktrans %}World{% endblocktrans %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% trans 'Hello' %}\n{% blocktrans %}World{% endblocktrans %}\n"
        );
    }

    #[test]
    fn test_translation_tags_within_other_blocks() {
        let formatted = format(
            "{% if condition %}\n  {% trans 'Hello' %}\n{% endif %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% if condition %}\n  {% translate 'Hello' %}\n{% endif %}\n"
        );
    }

    #[test]
    fn test_translation_tags_with_filters() {
        let formatted = format(
            "{% blocktrans trimmed %}\n  Hello\n{% endblocktrans %}\n",
            Some((3, 1)),
        );
        assert_eq!(
            formatted,
            "{% blocktranslate trimmed %}\n  Hello\n{% endblocktranslate %}\n"
        );
    }

    // migrate_static_load_tags

    #[test]
    fn test_admin_static_migrated() {
        let formatted = format("{% load admin_static %}\n", Some((2, 1)));
        assert_eq!(formatted, "{% load static %}\n");
    }

    #[test]
    fn test_admin_static_not_migrated() {
        let formatted = format("{% load admin_static %}\n", Some((2, 0)));
        assert_eq!(formatted, "{% load admin_static %}\n");
    }

    #[test]
    fn test_staticfiles_migrated() {
        let formatted = format("{% load staticfiles %}\n", Some((2, 1)));
        assert_eq!(formatted, "{% load static %}\n");
    }

    #[test]
    fn test_staticfiles_not_migrated() {
        let formatted = format("{% load staticfiles %}\n", Some((2, 0)));
        assert_eq!(formatted, "{% load staticfiles %}\n");
    }

    #[test]
    fn test_from_admin_static_migrated() {
        let formatted = format("{% load static from admin_static %}\n", Some((2, 1)));
        assert_eq!(formatted, "{% load static from static %}\n");
    }

    #[test]
    fn test_from_admin_static_not_migrated() {
        let formatted = format("{% load static from admin_static %}\n", Some((2, 0)));
        assert_eq!(formatted, "{% load static from admin_static %}\n");
    }

    #[test]
    fn test_from_staticfiles_migrated() {
        let formatted = format("{% load static from staticfiles %}\n", Some((2, 1)));
        assert_eq!(formatted, "{% load static from static %}\n");
    }

    #[test]
    fn test_from_staticfiles_not_migrated() {
        let formatted = format("{% load static from staticfiles %}\n", Some((2, 0)));
        assert_eq!(formatted, "{% load static from staticfiles %}\n");
    }

    // migrate_assignments

    #[test]
    fn test_migrate_assignments_with_single_legacy() {
        let formatted = format(
            "{% with engines.count as total %}{{ total }}{% endwith %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% with total=engines.count %}{{ total }}{% endwith %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_with_multiple_legacy() {
        let formatted = format(
            "{% with engines.count as total and cars.count as vehicles %}{{ total }} {{ vehicles }}{% endwith %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% with total=engines.count vehicles=cars.count %}{{ total }} {{ vehicles }}{% endwith %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_with_mixed() {
        let formatted = format(
            "{% with engines.count as total and vehicles=cars.count %}{{ total }} {{ vehicles }}{% endwith %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% with total=engines.count vehicles=cars.count %}{{ total }} {{ vehicles }}{% endwith %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_with_new_unchanged() {
        let formatted = format(
            "{% with total=engines.count %}{{ total }}{% endwith %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% with total=engines.count %}{{ total }}{% endwith %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_with_nested() {
        let formatted = format(
            "{% with outer=1 %}{% with 2 as inner %}{{ outer }} {{ inner }}{% endwith %}{% endwith %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% with outer=1 %}{% with inner=2 %}{{ outer }} {{ inner }}{% endwith %}{% endwith %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_with_unknown_end() {
        let formatted = format(
            "{% with 'Go' as exclamation loud %}{{ exclamation }}{% endwith %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% with exclamation='Go' loud %}{{ exclamation }}{% endwith %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_with_unknown_start() {
        let formatted = format(
            "{% with loud 'Go' as exclamation %}{{ exclamation }}{% endwith %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% with loud 'Go' as exclamation %}{{ exclamation }}{% endwith %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktrans_legacy_with() {
        let formatted = format(
            "{% blocktrans with engine.name as name %}Hello {{ name }}{% endblocktrans %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktrans with name=engine.name %}Hello {{ name }}{% endblocktrans %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_legacy_with() {
        let formatted = format(
            "{% blocktranslate with engine.name as name %}Hello {{ name }}{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with name=engine.name %}Hello {{ name }}{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_modern_with() {
        let formatted = format(
            "{% blocktranslate with name=engine.name %}Hello {{ name }}{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with name=engine.name %}Hello {{ name }}{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_legacy_count() {
        let formatted = format(
            "{% blocktranslate count engines.count as total %}{{ total }} user{% plural %}{{ total }} users{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate count total=engines.count %}{{ total }} user{% plural %}{{ total }} users{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_modern_count() {
        let formatted = format(
            "{% blocktranslate count total=engines.count %}{{ total }} engine{% plural %}{{ total }} engines{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate count total=engines.count %}{{ total }} engine{% plural %}{{ total }} engines{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_legacy_with_and_count() {
        let formatted = format(
            "{% blocktranslate with engine.name as name count engines.count as total %}Hello {{ name }}, there is {{ total }} engine{% plural %}Hello {{ name }}, there are {{ total }} engines{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with name=engine.name count total=engines.count %}Hello {{ name }}, there is {{ total }} engine{% plural %}Hello {{ name }}, there are {{ total }} engines{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_modern_with_and_count() {
        let formatted = format(
            "{% blocktranslate with name=engine.name count total=engines.count %}Hello {{ name }}, there is {{ total }} engines{% plural %}Hello {{ name }}, there are {{ total }} engines{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with name=engine.name count total=engines.count %}Hello {{ name }}, there is {{ total }} engines{% plural %}Hello {{ name }}, there are {{ total }} engines{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_multiple_with_legacy() {
        let formatted = format(
            "{% blocktranslate with engine.name as name and engine.number as number %}Hello {{ name }} #{{ number }}{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with name=engine.name number=engine.number %}Hello {{ name }} #{{ number }}{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_multiple_with_mixed() {
        let formatted = format(
            "{% blocktranslate with engine.name as name and number=engine.number %}Hello {{ name }} #{{ number }}{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with name=engine.name number=engine.number %}Hello {{ name }} #{{ number }}{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_with_filters() {
        let formatted = format(
            "{% blocktranslate with engine.name|upper as shouty %}HELLO {{ shouty }}{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with shouty=engine.name|upper %}HELLO {{ shouty }}{% endblocktranslate %}\n"
        );
    }

    #[test]
    fn test_migrate_assignments_blocktranslate_context() {
        let formatted = format(
            "{% blocktranslate with name=engine.name context 'greeting' %}Hello {{ name }}{% endblocktranslate %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% blocktranslate with name=engine.name context 'greeting' %}Hello {{ name }}{% endblocktranslate %}\n"
        );
    }

    // Formatters

    // update_leading_trailing_whitespace

    #[test]
    fn test_format_trim_leading_whitespace() {
        let formatted = format("  \n  {% yolk %}\n", None);
        assert_eq!(formatted, "  {% yolk %}\n");
    }

    #[test]
    fn test_format_trim_trailing_whitespace() {
        let formatted = format("{% yolk %}  \n  ", None);
        assert_eq!(formatted, "{% yolk %}\n");
    }

    #[test]
    fn test_format_trim_whitespace_mixed_crlf() {
        let formatted = format(" \r\n {% yolk %}  \n  ", None);
        assert_eq!(formatted, " {% yolk %}\r\n");
    }

    #[test]
    fn test_format_preserve_content_whitespace() {
        let formatted = format("{% block crack %}\n  Yum  \n{% endblock crack %}", None);
        assert_eq!(
            formatted,
            "{% block crack %}\n  Yum  \n{% endblock crack %}\n"
        );
    }

    #[test]
    fn test_format_add_trailing_newline() {
        let formatted = format("{% block crack %}Yum{% endblock %}", None);
        assert_eq!(formatted, "{% block crack %}Yum{% endblock %}\n");
    }

    #[test]
    fn test_format_whitespace_only_template() {
        let formatted = format("  \t\n  ", None);
        assert_eq!(formatted, "\n");
    }

    #[test]
    fn test_format_whitespace_only_template_with_crlf() {
        let formatted = format("  \t\r\n  ", None);
        assert_eq!(formatted, "\r\n");
    }

    #[test]
    fn test_format_no_text_tokens() {
        let formatted = format("{% yolk %}", None);
        assert_eq!(formatted, "{% yolk %}\n");
    }

    // update_load_tags

    #[test]
    fn test_format_load_sorted() {
        let formatted = format("{% load z y x %}\n", None);
        assert_eq!(formatted, "{% load x y z %}\n");
    }

    #[test]
    fn test_format_load_whitespace_cleaned() {
        let formatted = format("{% load   x  y %}\n", None);
        assert_eq!(formatted, "{% load x y %}\n");
    }

    #[test]
    fn test_format_load_consecutive_merged() {
        let formatted = format("{% load x %}{% load y %}\n", None);
        assert_eq!(formatted, "{% load x y %}\n");
    }

    #[test]
    fn test_format_load_consecutive_space_merged() {
        let formatted = format("{% load x %} {% load y %}\n", None);
        assert_eq!(formatted, "{% load x y %}\n");
    }

    #[test]
    fn test_format_load_consecutive_newline_merged() {
        let formatted = format("{% load x %}\n{% load y %}\n", None);
        assert_eq!(formatted, "{% load x y %}\n");
    }

    #[test]
    fn test_format_load_from_too_short_untouched() {
        let formatted = format("{% load from a %}\n", None);
        assert_eq!(formatted, "{% load from a %}\n");
    }

    #[test]
    fn test_format_load_from_incorrect_untouched() {
        let formatted = format("{% load c b from a thing %}\n", None);
        assert_eq!(formatted, "{% load c b from a thing %}\n");
    }

    #[test]
    fn test_format_load_from_sorted() {
        let formatted = format("{% load c b from a %}\n", None);
        assert_eq!(formatted, "{% load b c from a %}\n");
    }

    #[test]
    fn test_format_load_from_unmerged_plain() {
        let formatted = format("{% load b from a %}\n{% load c %}\n", None);
        assert_eq!(formatted, "{% load b from a %}\n{% load c %}\n");
    }

    #[test]
    fn test_format_load_plain_unmerged_from() {
        let formatted = format("{% load c %}\n{% load b from a %}\n", None);
        assert_eq!(formatted, "{% load c %}\n{% load b from a %}\n");
    }

    #[test]
    fn test_format_load_from_unmerged_from() {
        let formatted = format("{% load b from a %}\n{% load d from c %}\n", None);
        assert_eq!(formatted, "{% load b from a %}\n{% load d from c %}\n");
    }

    #[test]
    fn test_format_load_trailing_empty_lines_left() {
        let formatted = format("{% load albumen %}\n\n{% albu %}\n", None);
        assert_eq!(formatted, "{% load albumen %}\n\n{% albu %}\n");
    }

    // update_endblock_labels

    #[test]
    fn test_format_block_no_label() {
        let formatted = format("{% block %}\n{% endblock %}\n", None);
        assert_eq!(formatted, "{% block %}\n{% endblock %}\n");
    }

    #[test]
    fn test_format_endblock_broken() {
        let formatted = format("{% endblock %}\n", None);
        assert_eq!(formatted, "{% endblock %}\n");
    }

    #[test]
    fn test_format_endblock_broken_nesting() {
        let formatted = format("{% block a %}\n{% endblock b %}\n", None);
        assert_eq!(formatted, "{% block a %}\n{% endblock b %}\n");
    }

    #[test]
    fn test_format_endblock_label_added() {
        let formatted = format("{% block h %}\n{% endblock %}\n", None);
        assert_eq!(formatted, "{% block h %}\n{% endblock h %}\n");
    }

    #[test]
    fn test_format_endblock_label_added_nested() {
        let formatted = format(
            "{% block h %}\n{% block i %}\n{% endblock %}\n{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% block h %}\n{% block i %}\n{% endblock i %}\n{% endblock h %}\n"
        );
    }

    #[test]
    fn test_format_endblock_label_removed() {
        let formatted = format("{% block h %}i{% endblock h %}\n", None);
        assert_eq!(formatted, "{% block h %}i{% endblock %}\n");
    }

    #[test]
    fn test_format_endblock_with_blocktranslate() {
        let formatted = format(
            "{% block h %}\n{% blocktranslate %}ovo{% endblocktranslate %}\n{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% block h %}\n{% blocktranslate %}ovo{% endblocktranslate %}\n{% endblock h %}\n"
        );
    }

    #[test]
    fn test_format_partialdef_no_label() {
        let formatted = format("{% partialdef %}\n{% endpartialdef %}\n", None);
        assert_eq!(formatted, "{% partialdef %}\n{% endpartialdef %}\n");
    }

    #[test]
    fn test_format_endpartialdef_broken() {
        let formatted = format("{% endpartialdef %}\n", None);
        assert_eq!(formatted, "{% endpartialdef %}\n");
    }

    #[test]
    fn test_format_endpartialdef_broken_nesting() {
        let formatted = format("{% partialdef a %}\n{% endpartialdef b %}\n", None);
        assert_eq!(formatted, "{% partialdef a %}\n{% endpartialdef b %}\n");
    }

    #[test]
    fn test_format_endpartialdef_label_added() {
        let formatted = format("{% partialdef button %}\n{% endpartialdef %}\n", None);
        assert_eq!(
            formatted,
            "{% partialdef button %}\n{% endpartialdef button %}\n"
        );
    }

    #[test]
    fn test_format_endpartialdef_label_added_nested() {
        let formatted = format(
            "{% partialdef a %}\n{% partialdef b %}\n{% endpartialdef %}\n{% endpartialdef %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% partialdef a %}\n{% partialdef b %}\n{% endpartialdef b %}\n{% endpartialdef a %}\n"
        );
    }

    #[test]
    fn test_format_endpartialdef_label_removed() {
        let formatted = format("{% partialdef button %}i{% endpartialdef button %}\n", None);
        assert_eq!(formatted, "{% partialdef button %}i{% endpartialdef %}\n");
    }

    #[test]
    fn test_format_block_and_partialdef_independent_stacks() {
        let formatted = format(
            "{% block a %}\n{% partialdef b %}\n{% endpartialdef %}\n{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% block a %}\n{% partialdef b %}\n{% endpartialdef b %}\n{% endblock a %}\n"
        );
    }

    // update_top_level_block_indentation

    #[test]
    fn test_format_extends_unindented() {
        let formatted = format("  {% extends 'egg.html' %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n");
    }

    #[test]
    fn test_format_top_level_blocks_unindented() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n  {% block yolk %}\n    yellow\n  {% endblock yolk %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block yolk %}\n    yellow\n{% endblock yolk %}\n"
        );
    }

    #[test]
    fn test_format_top_level_blocks_trailing_spaces_unchanged() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n{% block classes %}{{ block.super }} yellow {% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block classes %}{{ block.super }} yellow {% endblock %}\n"
        );
    }

    #[test]
    fn test_format_top_level_blocks_trailing_tabs_unchanged() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n{% block classes %}{{ block.super }}\tyellow\t{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block classes %}{{ block.super }}\tyellow\t{% endblock %}\n"
        );
    }

    #[test]
    fn test_format_top_level_blocks_unindented_with_crlf() {
        let formatted = format(
            "{% extends 'egg.html' %}\r\n\r\n  {% block yolk %}\r\n    yellow\r\n  {% endblock yolk %}\r\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\r\n\r\n{% block yolk %}\r\n    yellow\r\n{% endblock yolk %}\r\n"
        );
    }

    #[test]
    fn test_format_second_level_blocks_indented() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n{% block yolk %}\n  {% block white %}\n    protein\n  {% endblock white %}\n{% endblock yolk %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block yolk %}\n  {% block white %}\n    protein\n  {% endblock white %}\n{% endblock yolk %}\n"
        );
    }

    #[test]
    fn test_no_unindent_without_extends() {
        let formatted = format(
            "  {% block yolk %}\n    yellow\n  {% endblock yolk %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "  {% block yolk %}\n    yellow\n  {% endblock yolk %}\n"
        );
    }

    #[test]
    fn test_unindent_multiple_blocks() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n  {% block yolk %}\n  yellow\n  {% endblock yolk %}\n\n  {% block white %}\n    protein\n  {% endblock white %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block yolk %}\n  yellow\n{% endblock yolk %}\n\n{% block white %}\n    protein\n{% endblock white %}\n"
        );
    }

    #[test]
    fn test_no_unindenting_inside_if() {
        let formatted = format(
            "{% extends 'engine.html' %}\n{% if steam %}\n  {% block whistle %}\n  peep\n  {% endblock whistle %}\n{% endif %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'engine.html' %}\n{% if steam %}\n  {% block whistle %}\n  peep\n  {% endblock whistle %}\n{% endif %}\n"
        );
    }

    #[test]
    fn test_unindent_with_if_inside() {
        let formatted = format(
            "{% extends 'engine.html' %}\n\n  {% block whistle %}\n  {% if steam %}\n  peep\n  {% endif %}\n  {% endblock whistle %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'engine.html' %}\n\n{% block whistle %}\n  {% if steam %}\n  peep\n  {% endif %}\n{% endblock whistle %}\n"
        );
    }

    // update_top_level_block_spacing

    #[test]
    fn test_update_top_level_block_spacing_no_change() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n{% block yolk %}Sunny side up{% endblock %}\n\n{% block white %}Albumin{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block yolk %}Sunny side up{% endblock %}\n\n{% block white %}Albumin{% endblock %}\n"
        );
    }

    #[test]
    fn test_update_top_level_block_spacing_add_line() {
        let formatted = format(
            "{% extends 'egg.html' %}\n{% block yolk %}Sunny side up{% endblock %}\n{% block white %}Albumin{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block yolk %}Sunny side up{% endblock %}\n\n{% block white %}Albumin{% endblock %}\n"
        );
    }

    #[test]
    fn test_update_top_level_block_spacing_add_line_with_crlf_first() {
        let formatted = format(
            "{% extends 'egg.html' %}\r\n{% block yolk %}Sunny side up{% endblock %}\n{% block white %}Albumin{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\r\n\r\n{% block yolk %}Sunny side up{% endblock %}\r\n\r\n{% block white %}Albumin{% endblock %}\r\n"
        );
    }

    #[test]
    fn test_update_top_level_block_spacing_remove_extra_lines() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n\n{% block yolk %}Sunny side up{% endblock %}\n\n\n{% block white %}Albumin{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block yolk %}Sunny side up{% endblock %}\n\n{% block white %}Albumin{% endblock %}\n"
        );
    }

    #[test]
    fn test_update_top_level_block_spacing_remove_extra_line_with_crlf_first() {
        let formatted = format(
            "{% extends 'egg.html' %}\r\n\r\n\r\n{% block yolk %}Sunny side up{% endblock %}\n\n\n{% block white %}Albumin{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\r\n\r\n{% block yolk %}Sunny side up{% endblock %}\r\n\r\n{% block white %}Albumin{% endblock %}\r\n"
        );
    }

    #[test]
    fn test_update_top_level_block_spacing_nested_blocks() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n{% block yolk %}{% block inner_yolk %}Runny{% endblock %}{% endblock %}\n\n{% block white %}Firm{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block yolk %}{% block inner_yolk %}Runny{% endblock %}{% endblock %}\n\n{% block white %}Firm{% endblock %}\n"
        );
    }

    #[test]
    fn test_update_top_level_block_spacing_no_extends() {
        let formatted = format(
            "{% block yolk %}Sunny side up{% endblock %}\n{% block white %}Albumin{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% block yolk %}Sunny side up{% endblock %}\n{% block white %}Albumin{% endblock %}\n"
        );
    }

    #[test]
    fn test_update_top_level_block_spacing_content() {
        let formatted = format(
            "{% extends 'egg.html' %}\n\n(not rendered)\n\n{% block yolk %}Sunny side up{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n(not rendered)\n\n{% block yolk %}Sunny side up{% endblock %}\n"
        );
    }

    #[test]
    fn test_update_top_level_block_spacing_comment() {
        let formatted = format(
            "{% extends 'egg.html' %}\n{# bla #}\n{% block yolk %}Sunny side up{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n{# bla #}\n{% block yolk %}Sunny side up{% endblock %}\n"
        );
    }

    // Final build

    #[test]
    fn test_format_spaces_added() {
        let formatted = format("a {{var}} {%tag%} {#comment#}\n", None);
        assert_eq!(formatted, "a {{ var }} {% tag %} {# comment #}\n");
    }

    #[test]
    fn test_format_spaces_removed() {
        let formatted = format("a {{  var  }} {%  tag  %} {#  comment  #}\n", None);
        assert_eq!(formatted, "a {{ var }} {% tag %} {# comment #}\n");
    }

    #[test]
    fn test_format_verbatim_left() {
        let formatted = format(
            "a {% verbatim %} {{var}} {%tag%} {#comment#} {% endverbatim %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "a {% verbatim %} {{var}} {%tag%} {#comment#} {% endverbatim %}\n"
        );
    }

    #[test]
    fn test_format_comment_tag_contents_left() {
        let formatted = format(
            "a {% comment %} {{var}} {%tag%} {#comment#} {% endcomment %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "a {% comment %} {{var}} {%tag%} {#comment#} {% endcomment %}\n"
        );
    }

    #[test]
    fn test_format_comment_tag_with_note_contents_left() {
        let formatted = format(
            "{%  comment  \"old\"  %}\n{% load  b a %}\n{%endcomment%}\n{{var}}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% comment \"old\" %}\n{% load  b a %}\n{% endcomment %}\n{{ var }}\n"
        );
    }

    #[test]
    fn test_format_comment_tag_not_nested() {
        let formatted = format(
            "{% comment %}{% comment %}{{a}}{% endcomment %}{{b}}{% endcomment %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% comment %}{% comment %}{{a}}{% endcomment %}{{ b }}{% endcomment %}\n"
        );
    }

    #[test]
    fn test_format_commented_out_block_ignored() {
        let formatted = format(
            "{% extends 'egg.html' %}\n{% block yolk %}\n{% comment %}{% block white %}{% endcomment %}\n{% endblock %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n\n{% block yolk %}\n{% comment %}{% block white %}{% endcomment %}\n{% endblock yolk %}\n"
        );
    }

    #[test]
    fn test_format_empty_block_tags_left() {
        let formatted = format(
            "{% %}{%%}{% extends 'egg.html' %}\n{%  %}\n{% load b a %}{% %}\n",
            Some((6, 1)),
        );
        assert_eq!(
            formatted,
            "{% %}{%%}{% extends 'egg.html' %}\n{%  %}\n{% load a b %}{% %}\n"
        );
    }

    #[test]
    fn test_format_empty_variable_tags_left() {
        let formatted = format("{{}}{{  }}{{a}}\n", None);
        assert_eq!(formatted, "{{}}{{  }}{{ a }}\n");
    }

    #[test]
    fn test_format_jinja_whitespace_control_left() {
        let formatted = format(
            "{%- if  a -%}{{- b -}}{%+ else +%}{#- c -#}{%-endif%}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{%- if  a -%}{{- b -}}{%+ else +%}{#- c -#}{%-endif%}\n"
        );
    }

    #[test]
    fn test_format_jinja_whitespace_control_one_side_left() {
        let formatted = format("{%- if  a %}{%  endif  -%}{{b}}\n", None);
        assert_eq!(formatted, "{%- if  a %}{%  endif  -%}{{ b }}\n");
    }

    #[test]
    fn test_format_negative_number_argument_formatted() {
        let formatted = format("{{ a|add:-1 }}{%  if  a == -1  %}{% endif %}\n", None);
        assert_eq!(formatted, "{{ a|add:-1 }}{% if a == -1 %}{% endif %}\n");
    }

    // format_variables

    #[test]
    fn test_format_variables_constant_int() {
        let formatted = format("{{ 1 }}\n", None);
        assert_eq!(formatted, "{{ 1 }}\n");
    }

    #[test]
    fn test_format_variables_constant_float() {
        let formatted = format("{{ 1.23 }}\n", None);
        assert_eq!(formatted, "{{ 1.23 }}\n");
    }

    #[test]
    fn test_format_variables_constant_float_negative() {
        let formatted = format("{{ -1.23 }}\n", None);
        assert_eq!(formatted, "{{ -1.23 }}\n");
    }

    #[test]
    fn test_format_variables_constant_str() {
        let formatted = format("{{ 'egg' }}\n", None);
        assert_eq!(formatted, "{{ 'egg' }}\n");
    }

    #[test]
    fn test_format_variables_constant_str_translated() {
        let formatted = format("{{ _('egg') }}\n", None);
        assert_eq!(formatted, "{{ _('egg') }}\n");
    }

    #[test]
    fn test_format_variables_var() {
        let formatted = format("{{ egg }}\n", None);
        assert_eq!(formatted, "{{ egg }}\n");
    }

    #[test]
    fn test_format_variables_var_attr() {
        let formatted = format("{{ egg.shell }}\n", None);
        assert_eq!(formatted, "{{ egg.shell }}\n");
    }

    #[test]
    fn test_format_variables_variable_filter_no_arg() {
        let formatted = format("{{ egg | crack }}\n", None);
        assert_eq!(formatted, "{{ egg|crack }}\n");
    }

    #[test]
    fn test_format_variables_variable_filter_constant_arg() {
        let formatted = format("{{ egg | crack:'fully' }}\n", None);
        assert_eq!(formatted, "{{ egg|crack:'fully' }}\n");
    }

    #[test]
    fn test_format_variables_variable_filter_variable_arg() {
        let formatted = format("{{ egg | crack:amount }}\n", None);
        assert_eq!(formatted, "{{ egg|crack:amount }}\n");
    }

    #[test]
    fn test_format_variables_syntax_error_start() {
        let formatted = format("{{ ?egg | crack }}\n", None);
        assert_eq!(formatted, "{{ ?egg | crack }}\n");
    }

    #[test]
    fn test_format_variables_syntax_error_end() {
        let formatted = format("{{ egg | crack? }}\n", None);
        assert_eq!(formatted, "{{ egg | crack? }}\n");
    }

    #[test]
    fn test_format_variables_syntax_error_middle() {
        let formatted = format("{{ engines[0].name | length }}\n", None);
        assert_eq!(formatted, "{{ engines[0].name | length }}\n");
    }

    #[test]
    fn test_format_variables_double_dot() {
        // Double dot syntax for "empty string lookup" deprecated:
        // https://code.djangoproject.com/ticket/35738
        let formatted = format("{{ engine..name }}\n", None);
        assert_eq!(formatted, "{{ engine..name }}\n");
    }

    #[test]
    fn test_format_block_bits() {
        let formatted = format("{%  if breakfast  ==  'egg'  %}\n", None);
        assert_eq!(formatted, "{% if breakfast == 'egg' %}\n");
    }

    #[test]
    fn test_format_block_bits_spaces_in_string() {
        let formatted = format("{% if breakfast == 'egg  mcmuffin' %}\n", None);
        assert_eq!(formatted, "{% if breakfast == 'egg  mcmuffin' %}\n");
    }

    #[test]
    fn test_format_block_bits_spaces_in_translated_string() {
        let formatted = format("{% if breakfast == _('egg  mcmuffin') %}\n", None);
        assert_eq!(formatted, "{% if breakfast == _('egg  mcmuffin') %}\n");
    }
}

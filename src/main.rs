mod cli;

use clap::Parser;
use regex::Regex;
use std::fs;
use std::sync::LazyLock;

fn main() {
    let args = cli::Args::parse();
    let exit_code = main_impl(&args, &mut std::io::stderr());
    std::process::exit(exit_code)
}

fn main_impl(args: &cli::Args, writer: &mut dyn std::io::Write) -> i32 {
    let target_version: Option<(u8, u8)> = {
        if args.target_version.is_none() {
            None
        } else {
            let version = args.target_version.as_ref().unwrap();
            let parts: Vec<&str> = version.split('.').collect();
            if parts.len() != 2 {
                panic!("Invalid target version format. Expected 'major.minor'");
            }
            Some((
                parts[0].parse().expect("Invalid major version number"),
                parts[1].parse().expect("Invalid minor version number"),
            ))
        }
    };

    let mut returncode = 0;
    let mut reformatted_count = 0;
    let mut already_formatted_count = 0;
    for filename in &args.filenames {
        match fs::read_to_string(filename) {
            Ok(content) => {
                let formatted = format(&content, target_version);
                if formatted != content {
                    if args.check {
                        writeln!(writer, "Would reformat: {}", filename).unwrap();
                        returncode = 1;
                        reformatted_count += 1;
                    } else {
                        fs::write(filename, formatted).expect("Could not write {filename}");
                        returncode = 1;
                        reformatted_count += 1;
                    }
                } else {
                    already_formatted_count += 1;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                writeln!(writer, "{} is non-UTF-8 (not supported)", filename).unwrap();
                returncode = 1;
            }
            Err(e) => {
                writeln!(writer, "Error reading {}: {}", filename, e).unwrap();
                returncode = 1;
            }
        }
    }

    let mut message = String::new();
    if reformatted_count > 0 {
        message.push_str(&reformatted_count.to_string());
        message.push_str(" file");
        if reformatted_count > 1 {
            message.push('s');
        }
        if args.check {
            message.push_str(" would be reformatted");
        } else {
            message.push_str(" reformatted");
        }
        if already_formatted_count > 0 {
            message.push_str(", ");
        }
    }
    if already_formatted_count > 0 {
        message.push_str(&already_formatted_count.to_string());
        message.push_str(" file");
        if already_formatted_count > 1 {
            message.push('s');
        }
        message.push_str(" already formatted");
    }
    if !message.is_empty() {
        writeln!(writer, "{}", message).unwrap();
    }

    returncode
}

// Lexer based on Django’s:
// https://github.com/django/django/blob/main/django/template/base.py

static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\{%.*?%\}|\{\{.*?\}\}|\{#.*?#\})").unwrap());

const BLOCK_TAG_START: &str = "{%";
const VARIABLE_TAG_START: &str = "{{";
const COMMENT_TAG_START: &str = "{#";

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Text {
        contents: String,
        lineno: usize,
    },
    Variable {
        filter_expression: FilterExpression,
        lineno: usize,
    },
    Block {
        bits: Vec<String>,
        lineno: usize,
    },
    Comment {
        contents: String,
        lineno: usize,
    },
}

fn lex(template_string: &str) -> Vec<Token> {
    let mut result = Vec::new();
    let mut verbatim = None;
    let mut lineno = 1;
    let mut last_end = 0;

    for cap in (&*TAG_RE).captures_iter(template_string) {
        let token_match = cap.get(0).unwrap();
        let (start, end) = (token_match.start(), token_match.end());

        if start > last_end {
            let text = &template_string[last_end..start];
            result.push(create_token(text, lineno, false, &mut verbatim));
            lineno += text.matches('\n').count();
        }

        let token_string = token_match.as_str();
        result.push(create_token(token_string, lineno, true, &mut verbatim));
        lineno += token_string.matches('\n').count();

        last_end = end;
    }

    if last_end < template_string.len() {
        let text = &template_string[last_end..];
        result.push(create_token(text, lineno, false, &mut verbatim));
    }

    result
}

fn create_token(
    token_string: &str,
    lineno: usize,
    in_tag: bool,
    verbatim: &mut Option<String>,
) -> Token {
    if in_tag {
        let content = token_string[2..token_string.len() - 2].trim();
        if token_string.starts_with(BLOCK_TAG_START) {
            if let Some(v) = &verbatim {
                if content != v {
                    return Token::Text {
                        contents: token_string.to_string(),
                        lineno,
                    };
                }
                *verbatim = None;
            } else if content.starts_with("verbatim") {
                *verbatim = Some(format!("end{}", content));
            }
            Token::Block {
                bits: split_contents(content),
                lineno,
            }
        } else if verbatim.is_none() {
            if token_string.starts_with(VARIABLE_TAG_START) {
                Token::Variable {
                    filter_expression: lex_filter_expression(content),
                    lineno,
                }
            } else {
                debug_assert!(token_string.starts_with(COMMENT_TAG_START));
                Token::Comment {
                    contents: content.to_string(),
                    lineno,
                }
            }
        } else {
            Token::Text {
                contents: token_string.to_string(),
                lineno,
            }
        }
    } else {
        Token::Text {
            contents: token_string.to_string(),
            lineno,
        }
    }
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
        ^(?P<var>[{var_chars}]+|{num})|
         (?:\s*{filter_sep}\s*
             (?P<filter_name>\w+)
                 (?:{arg_sep}
                     (?:
                      (?P<constant_arg>{constant})|
                      (?P<var_arg>[{var_chars}]+|{num})
                     )
                 )?
         )"#,
        constant = constant_string,
        num = r"[-+.]?\d[\d.e]*",
        var_chars = r"\w\.",
        filter_sep = regex::escape("|"),
        arg_sep = regex::escape(":"),
    ))
    .build()
    .unwrap()
});

#[derive(Debug, Clone, PartialEq)]
enum Expression {
    Constant(String),
    Variable(String),
    Unparsed(String),
}

#[derive(Debug, Clone, PartialEq)]
struct FilterExpression {
    var: Expression,
    filters: Vec<Filter>,
}

#[derive(Debug, Clone, PartialEq)]
struct Filter {
    name: String,
    arg: Option<Expression>,
}

fn lex_filter_expression(expr: &str) -> FilterExpression {
    let mut filter_expression = FilterExpression {
        var: Expression::Unparsed(expr.to_string()),
        filters: Vec::new(),
    };
    let mut upto = 0;
    let mut variable = false;
    for captures in (&*FILTER_RE).captures_iter(expr) {
        let start = captures.get(0).unwrap().start();
        if upto != start {
            // Syntax error - ignore it and return whole expression as constant
            return filter_expression;
        }

        if !variable {
            if let Some(constant) = captures.name("constant") {
                filter_expression.var = Expression::Constant(constant.as_str().to_string());
            } else if let Some(variable) = captures.name("var") {
                filter_expression.var = Expression::Variable(variable.as_str().to_string());
            }
            variable = true;
        } else {
            let filter_name = captures.name("filter_name").unwrap().as_str().to_string();
            if let Some(constant_arg) = captures.name("constant_arg") {
                filter_expression.filters.push(Filter {
                    name: filter_name,
                    arg: Some(Expression::Constant(constant_arg.as_str().to_string())),
                });
            } else if let Some(var_arg) = captures.name("var_arg") {
                filter_expression.filters.push(Filter {
                    name: filter_name,
                    arg: Some(Expression::Variable(var_arg.as_str().to_string())),
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
            var: Expression::Unparsed(expr.to_string()),
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

fn smart_split(text: &str) -> Vec<String> {
    SMART_SPLIT_RE
        .captures_iter(text)
        .map(|cap| cap[0].to_string())
        .collect()
}

fn split_contents(contents: &str) -> Vec<String> {
    let mut split = Vec::new();
    let mut bits = smart_split(contents).into_iter();

    while let Some(mut bit) = bits.next() {
        if bit.starts_with("_(\"") || bit.starts_with("_('") {
            let sentinel = format!("{})", &bit[2..3]);
            let mut trans_bit = vec![bit];
            while !trans_bit.last().unwrap().ends_with(&sentinel) {
                if let Some(next_bit) = bits.next() {
                    trans_bit.push(next_bit);
                } else {
                    break;
                }
            }
            bit = trans_bit.join(" ");
        }

        split.push(bit);
    }
    split
}

fn format(content: &str, target_version: Option<(u8, u8)>) -> String {
    // Lex
    let newline = detect_newline(content);
    let mut tokens = lex(content);

    // Fixers
    migrate_length_is(&mut tokens, target_version);
    migrate_empty_json_script(&mut tokens, target_version);
    migrate_translation_tags(&mut tokens, target_version);
    migrate_ifequal_tags(&mut tokens, target_version);
    migrate_static_load_tags(&mut tokens, target_version);

    // Formatters
    update_leading_trailing_whitespace(&mut tokens, newline);
    update_load_tags(&mut tokens);
    update_endblock_labels(&mut tokens);
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
    match content.split_once('\n') {
        Some((s, _)) if s.ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

#[inline(always)]
fn format_variable(filter_expression: FilterExpression, result: &mut String) {
    match filter_expression.var {
        Expression::Constant(value) => {
            result.push_str(&value);
        }
        Expression::Variable(value) => {
            result.push_str(&value);
        }
        Expression::Unparsed(value) => {
            result.push_str(&value);
        }
    }
    for filter in filter_expression.filters {
        result.push('|');
        result.push_str(&filter.name);
        if let Some(arg) = filter.arg {
            result.push(':');
            match arg {
                Expression::Constant(value) => {
                    result.push_str(&value);
                }
                Expression::Variable(value) => {
                    result.push_str(&value);
                }
                Expression::Unparsed(value) => {
                    result.push_str(&value);
                }
            }
        }
    }
}

// Fixers

static LENGTH_IS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([\w.]+)\|length_is:(\w+)").unwrap());

fn migrate_length_is(tokens: &mut Vec<Token>, target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (4, 2) {
        return;
    }

    for token in tokens.iter_mut() {
        if let Token::Block { bits, .. } = token {
            if bits.len() != 2 {
                continue;
            }
            if let Some(captures) = LENGTH_IS_RE.captures(&bits[1]) {
                let var1 = captures.get(1).unwrap().as_str().to_string();
                let var2 = captures.get(2).unwrap().as_str().to_string();
                bits[1] = format!("{}|length", var1);
                bits.push("==".to_string());
                bits.push(var2.to_string());
            }
        }
    }
}

fn migrate_empty_json_script(tokens: &mut Vec<Token>, target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (4, 1) {
        return;
    }

    for token in tokens.iter_mut() {
        if let Token::Variable {
            filter_expression, ..
        } = token
        {
            for filter in &mut filter_expression.filters {
                if filter.name == "json_script" {
                    if let Some(Expression::Constant(arg)) = &filter.arg {
                        if arg == "\"\"" || arg == "''" {
                            filter.arg = None;
                        }
                    }
                }
            }
        }
    }
}

fn migrate_translation_tags(tokens: &mut Vec<Token>, target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (3, 1) {
        return;
    }

    for token in tokens.iter_mut() {
        if let Token::Block { bits, .. } = token {
            match bits[0].as_str() {
                "trans" => {
                    bits[0] = "translate".to_string();
                }
                "blocktrans" => {
                    bits[0] = "blocktranslate".to_string();
                }
                "endblocktrans" => {
                    bits[0] = "endblocktranslate".to_string();
                }
                "load" => {
                    if bits.len() >= 4
                        && bits[bits.len() - 2] == "from"
                        && bits[bits.len() - 1] == "i18n"
                    {
                        for i in 1..bits.len() - 2 {
                            if bits[i] == "trans" {
                                bits[i] = "translate".to_string();
                            } else if bits[i] == "blocktrans" {
                                bits[i] = "blocktranslate".to_string();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn migrate_ifequal_tags(tokens: &mut Vec<Token>, target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (3, 1) {
        return;
    }

    // First pass: find matching pairs
    let mut stack = Vec::new();
    let mut pairs = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if let Token::Block { bits, .. } = token {
            match bits[0].as_str() {
                "ifequal" | "ifnotequal" => {
                    if bits.len() == 3 {
                        stack.push(i)
                    }
                }
                "endifequal" | "endifnotequal" => {
                    if let Some(start) = stack.pop() {
                        if bits.len() == 1 {
                            pairs.push((start, i));
                        }
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
        {
            if start_bits.len() >= 3 {
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
                    bits.push("if".to_string());
                    bits.push(var1);
                    bits.push(comparison.to_string());
                    bits.push(var2);
                }

                // Update end token
                if let Token::Block { bits, .. } = &mut tokens[end] {
                    bits.clear();
                    bits.push("endif".to_string());
                }
            }
        }
    }
}

fn migrate_static_load_tags(tokens: &mut Vec<Token>, target_version: Option<(u8, u8)>) {
    if target_version.is_none() || target_version.unwrap() < (2, 1) {
        return;
    }

    for token in tokens.iter_mut() {
        if let Token::Block { bits, .. } = token {
            if bits[0] == "load" {
                if bits.contains(&"from".to_string()) {
                    if bits.len() >= 4 && bits[bits.len() - 2] == "from" {
                        let last = bits.len() - 1;
                        let library = bits[last].as_str();
                        if library == "admin_static" || library == "staticfiles" {
                            bits[last] = "static".to_string();
                        }
                    }
                } else {
                    for i in 1..bits.len() {
                        if bits[i] == "admin_static" || bits[i] == "staticfiles" {
                            bits[i] = "static".to_string();
                        }
                    }
                }
            }
        }
    }
}

// Formatters

static LEADING_BLANK_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*\n)+").unwrap());

fn update_leading_trailing_whitespace(tokens: &mut Vec<Token>, newline: &str) {
    if let Some(mut token) = tokens.first_mut() {
        if let Token::Text { contents, .. } = &mut token {
            *contents = (&*LEADING_BLANK_LINES).replace(contents, "").to_string();
        }
    }

    if let Some(mut token) = tokens.last_mut() {
        if let Token::Text { contents, .. } = &mut token {
            *contents = format!("{}{}", contents.trim_end(), newline);
        } else {
            tokens.push(Token::Text {
                contents: newline.to_string(),
                lineno: 0,
            });
        }
    }
}

fn update_load_tags(tokens: &mut Vec<Token>) {
    let mut i = 0;
    while i < tokens.len() {
        if let Token::Block { ref bits, .. } = tokens[i] {
            if bits[0] == "load" {
                // load ... from ...
                if bits.contains(&"from".to_string()) {
                    if bits.len() >= 4 && bits[bits.len() - 2] == "from" {
                        let library = bits[bits.len() - 1].as_str();
                        let mut parts = bits[1..bits.len() - 2].to_vec();

                        parts.sort_unstable();
                        parts.dedup();
                        parts.insert(0, "load".to_string());
                        parts.push("from".to_string());
                        parts.push(library.to_string());

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
                                if bits.contains(&"from".to_string()) {
                                    break;
                                }
                                to_merge.push(j);
                                j += 1
                            }
                            _ => break,
                        }
                    }
                    if j > 0 {
                        if let Token::Text { .. } = tokens[j - 1] {
                            j -= 1;
                        }
                    }

                    let mut parts: Vec<String> = to_merge
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
                    parts.insert(0, "load".to_string());

                    if let Token::Block { bits, .. } = &mut tokens[i] {
                        bits.clear();
                        bits.extend(parts);
                    }

                    tokens.drain(i + 1..j);
                }
            }
        }
        i += 1;
    }
}

fn update_endblock_labels(tokens: &mut Vec<Token>) {
    let mut block_stack = Vec::new();
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
                    if bits.len() == 1 || (bits.len() == 2 && label == bits[1]) {
                        let same_line = start_lineno == *lineno;
                        Some(if same_line {
                            vec!["endblock".to_string()]
                        } else {
                            vec!["endblock".to_string(), label]
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(new_bits) = update {
            if let Token::Block { lineno, .. } = tokens[i] {
                tokens[i] = Token::Block {
                    bits: new_bits,
                    lineno,
                };
            }
        }
        i += 1;
    }
}

fn update_top_level_block_indentation(tokens: &mut Vec<Token>) {
    let mut after_extends = false;
    let mut block_depth = 0;

    for i in 0..tokens.len() {
        match &tokens[i] {
            Token::Block { bits, .. } => {
                if bits.len() >= 1 && bits[0] == "extends" {
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
                }
            }
            _ => continue,
        }
    }
}

fn unindent_token(tokens: &mut Vec<Token>, index: usize) {
    if index > 0 {
        if let Token::Text { contents, .. } = &mut tokens[index - 1] {
            *contents = contents.trim_end_matches(&[' ', '\t']).to_string();
        }
    }
}

fn update_top_level_block_spacing(tokens: &mut Vec<Token>, newline: &str) {
    let mut has_extends = false;
    let mut depth = 0;
    let mut last_top_level_tag = None;
    let mut i = 0;

    while i < tokens.len() {
        if let Token::Block { bits, .. } = &tokens[i] {
            match bits[0].as_str() {
                "extends" => {
                    has_extends = true;
                    last_top_level_tag = Some(i);
                }
                "block" => {
                    if has_extends && depth == 0 {
                        if let Some(last_end) = last_top_level_tag {
                            if last_end == i - 2 {
                                if let Token::Text { contents, .. } = &mut tokens[i - 1] {
                                    if contents.trim().is_empty() {
                                        *contents = format!("{}{}", newline, newline);
                                    }
                                }
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
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    // main

    #[test]
    fn test_main_impl_one_already_formatted() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("tank-engine.html");
        fs::write(&file_path, "{{ name }}\n").unwrap();

        // Capture stderr
        let mut buffer = Vec::new();
        let mut writer = std::io::Cursor::new(&mut buffer);

        // Run the main function with our non-UTF-8 file
        let args = cli::Args {
            filenames: vec![file_path.to_str().unwrap().to_string()],
            target_version: None,
            check: false,
        };

        let returncode = main_impl(&args, &mut writer);

        assert_eq!(returncode, 0);
        let output = String::from_utf8(buffer).unwrap();
        assert_eq!(output, "1 file already formatted\n");
    }

    #[test]
    fn test_main_impl_one_reformatted() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("tank-engine.html");
        fs::write(&file_path, "{{name}}").unwrap();

        // Capture stderr
        let mut buffer = Vec::new();
        let mut writer = std::io::Cursor::new(&mut buffer);

        // Run the main function with our non-UTF-8 file
        let args = cli::Args {
            filenames: vec![file_path.to_str().unwrap().to_string()],
            target_version: None,
            check: false,
        };

        let returncode = main_impl(&args, &mut writer);

        assert_eq!(returncode, 1);
        let output = String::from_utf8(buffer).unwrap();
        assert_eq!(output, "1 file reformatted\n");

        // Verify the file was changed
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "{{ name }}\n");
    }

    #[test]
    fn test_main_impl_one_non_utf_8_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("non_utf8.txt");

        // Create a file with non-UTF-8 content
        let mut file = File::create(&file_path).unwrap();
        file.write_all(&[0xFF, 0xFE, 0xFD]).unwrap();

        // Capture stderr
        let mut buffer = Vec::new();
        let mut writer = std::io::Cursor::new(&mut buffer);

        // Run the main function with our non-UTF-8 file
        let args = cli::Args {
            filenames: vec![file_path.to_str().unwrap().to_string()],
            target_version: None,
            check: false,
        };

        let returncode = main_impl(&args, &mut writer);

        assert_eq!(returncode, 1);

        let output = String::from_utf8(buffer).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("non_utf8.txt is non-UTF-8 (not supported)"));
    }

    #[test]
    fn test_main_impl_check_option() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("tank-engine.html");
        fs::write(&file_path, "{{name}}").unwrap();

        let mut buffer = Vec::new();
        let mut writer = std::io::Cursor::new(&mut buffer);

        let args = cli::Args {
            filenames: vec![file_path.to_str().unwrap().to_string()],
            target_version: None,
            check: true,
        };

        let returncode = main_impl(&args, &mut writer);

        assert_eq!(returncode, 1);
        let output = String::from_utf8(buffer).unwrap();
        // split into lines
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("Would reformat: "));
        assert!(lines[0].ends_with("tank-engine.html"));
        assert_eq!(lines[1], "1 file would be reformatted");

        // Verify the file wasn't actually changed
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "{{name}}");
    }

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
    fn test_length_is_not_migrated_with_other_conditions() {
        let formatted = format(
            "{% if eggs|length_is:1 and spam %}{% endif %}\n",
            Some((4, 2)),
        );
        assert_eq!(formatted, "{% if eggs|length_is:1 and spam %}{% endif %}\n");
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
        let formatted = format("{% extends 'egg.html' %}\n\n{% block yolk %}\n  {% block white %}\n    protein\n  {% endblock white %}\n{% endblock yolk %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n\n{% block yolk %}\n  {% block white %}\n    protein\n  {% endblock white %}\n{% endblock yolk %}\n");
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
        let formatted = format("{% extends 'egg.html' %}\n\n  {% block yolk %}\n  yellow\n  {% endblock yolk %}\n\n  {% block white %}\n    protein\n  {% endblock white %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n\n{% block yolk %}\n  yellow\n{% endblock yolk %}\n\n{% block white %}\n    protein\n{% endblock white %}\n");
    }

    // update_top_level_block_spacing

    #[test]
    fn test_update_top_level_block_spacing_no_change() {
        let formatted = format("{% extends 'egg.html' %}\n\n{% block yolk %}Sunny side up{% endblock %}\n\n{% block white %}Albumin{% endblock %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n\n{% block yolk %}Sunny side up{% endblock %}\n\n{% block white %}Albumin{% endblock %}\n");
    }

    #[test]
    fn test_update_top_level_block_spacing_add_line() {
        let formatted = format("{% extends 'egg.html' %}\n{% block yolk %}Sunny side up{% endblock %}\n{% block white %}Albumin{% endblock %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n\n{% block yolk %}Sunny side up{% endblock %}\n\n{% block white %}Albumin{% endblock %}\n");
    }

    #[test]
    fn test_update_top_level_block_spacing_add_line_with_crlf_first() {
        let formatted = format("{% extends 'egg.html' %}\r\n{% block yolk %}Sunny side up{% endblock %}\n{% block white %}Albumin{% endblock %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\r\n\r\n{% block yolk %}Sunny side up{% endblock %}\r\n\r\n{% block white %}Albumin{% endblock %}\r\n");
    }

    #[test]
    fn test_update_top_level_block_spacing_remove_extra_lines() {
        let formatted = format("{% extends 'egg.html' %}\n\n\n{% block yolk %}Sunny side up{% endblock %}\n\n\n{% block white %}Albumin{% endblock %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n\n{% block yolk %}Sunny side up{% endblock %}\n\n{% block white %}Albumin{% endblock %}\n");
    }

    #[test]
    fn test_update_top_level_block_spacing_remove_extra_line_with_crlf_first() {
        let formatted = format("{% extends 'egg.html' %}\r\n\r\n\r\n{% block yolk %}Sunny side up{% endblock %}\n\n\n{% block white %}Albumin{% endblock %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\r\n\r\n{% block yolk %}Sunny side up{% endblock %}\r\n\r\n{% block white %}Albumin{% endblock %}\r\n");
    }

    #[test]
    fn test_update_top_level_block_spacing_nested_blocks() {
        let formatted = format("{% extends 'egg.html' %}\n\n{% block yolk %}{% block inner_yolk %}Runny{% endblock %}{% endblock %}\n\n{% block white %}Firm{% endblock %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n\n{% block yolk %}{% block inner_yolk %}Runny{% endblock %}{% endblock %}\n\n{% block white %}Firm{% endblock %}\n");
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

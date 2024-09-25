mod cli;

use clap::Parser;
use regex::Regex;
use std::fs;
use std::sync::LazyLock;

fn main() {
    let args = cli::Args::parse();

    let target_version: Option<(u8, u8)> = {
        if args.target_version.is_none() {
            None
        } else {
            let version = args.target_version.unwrap();
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

    let mut changed = false;
    for filename in &args.filenames {
        let content = fs::read_to_string(filename).expect("Could not open {file}");
        let formatted = format(&content, target_version);
        if formatted != content {
            println!("Rewriting {}", filename);
            changed = true;
            fs::write(filename, formatted).expect("Could not write {filename}");
        }
    }
    std::process::exit(if changed { 1 } else { 0 });
}

// Lexer based on Django’s:
// https://github.com/django/django/blob/main/django/template/base.py

static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\{%.*?%\}|\{\{.*?\}\}|\{#.*?#\})").unwrap());

const BLOCK_TAG_START: &str = "{%";
const VARIABLE_TAG_START: &str = "{{";
const COMMENT_TAG_START: &str = "{#";

#[derive(Debug, Clone, PartialEq)]
enum TokenType {
    Text { contents: String },
    Variable { filter_expression: FilterExpression },
    Block { bits: Vec<String> },
    Comment { contents: String },
}

#[derive(Debug)]
struct Token {
    token_type: TokenType,
    lineno: usize,
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
                    return Token {
                        token_type: TokenType::Text {
                            contents: token_string.to_string(),
                        },
                        lineno,
                    };
                }
                *verbatim = None;
            } else if content.starts_with("verbatim") {
                *verbatim = Some(format!("end{}", content));
            }
            Token {
                token_type: TokenType::Block {
                    bits: split_contents(content),
                },
                lineno,
            }
        } else if verbatim.is_none() {
            if token_string.starts_with(VARIABLE_TAG_START) {
                Token {
                    token_type: TokenType::Variable {
                        filter_expression: lex_filter_expression(content),
                    },
                    lineno,
                }
            } else {
                debug_assert!(token_string.starts_with(COMMENT_TAG_START));
                Token {
                    token_type: TokenType::Comment {
                        contents: content.to_string(),
                    },
                    lineno,
                }
            }
        } else {
            Token {
                token_type: TokenType::Text {
                    contents: token_string.to_string(),
                },
                lineno,
            }
        }
    } else {
        Token {
            token_type: TokenType::Text {
                contents: token_string.to_string(),
            },
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
    let mut tokens = lex(content);

    // Token-fixing passes
    fix_template_whitespace(&mut tokens);
    update_load_tags(&mut tokens, target_version);
    fix_endblock_labels(&mut tokens);
    unindent_extends_and_blocks(&mut tokens);

    // Build result
    let mut result = String::new();
    for token in tokens {
        match token.token_type {
            TokenType::Text { contents } => result.push_str(&contents),
            TokenType::Variable { filter_expression } => {
                result.push_str("{{ ");
                format_variable(filter_expression, &mut result);
                result.push_str(" }}");
            }
            TokenType::Block { bits } => {
                result.push_str("{% ");
                result.push_str(&bits.join(" "));
                result.push_str(" %}");
            }
            TokenType::Comment { contents } => {
                result.push_str("{# ");
                result.push_str(&contents);
                result.push_str(" #}");
            }
        }
    }
    result
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

static LEADING_BLANK_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*\n)+").unwrap());
fn fix_template_whitespace(tokens: &mut Vec<Token>) {
    if let Some(token) = tokens.first_mut() {
        if let TokenType::Text { contents } = &mut token.token_type {
            *contents = (&*LEADING_BLANK_LINES).replace(contents, "").to_string();
        }
    }

    if let Some(token) = tokens.last_mut() {
        if let TokenType::Text { contents } = &mut token.token_type {
            *contents = contents.trim_end().to_string() + "\n";
        } else {
            tokens.push(Token {
                token_type: TokenType::Text {
                    contents: "\n".to_string(),
                },
                lineno: 0,
            });
        }
    }
}

fn update_load_tags(tokens: &mut Vec<Token>, target_version: Option<(u8, u8)>) {
    let mut i = 0;
    while i < tokens.len() {
        if let TokenType::Block { ref bits } = tokens[i].token_type {
            if bits[0] == "load" {
                let mut j = i + 1;
                let mut to_merge = vec![i];
                while j < tokens.len() {
                    match &tokens[j].token_type {
                        TokenType::Text { contents } if contents.trim().is_empty() => j += 1,
                        TokenType::Block { bits } if bits[0] == "load" => {
                            to_merge.push(j);
                            j += 1
                        }
                        _ => break,
                    }
                }
                if j > 0 {
                    if let TokenType::Text { .. } = tokens[j - 1].token_type {
                        j -= 1;
                    }
                }

                let mut parts: Vec<String> = to_merge
                    .iter()
                    .filter_map(|&idx| {
                        if let TokenType::Block { bits } = &tokens[idx].token_type {
                            Some(bits.iter().skip(1).cloned().collect::<Vec<_>>())
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .collect();

                if let Some(version) = target_version {
                    if version >= (2, 1) {
                        parts = parts
                            .into_iter()
                            .map(|part| match part.as_str() {
                                "admin_static" | "staticfiles" => "static".to_string(),
                                _ => part,
                            })
                            .collect();
                    }
                }

                parts.sort_unstable();
                parts.dedup();
                parts.insert(0, "load".to_string());

                if let TokenType::Block { bits } = &mut tokens[i].token_type {
                    bits.clear();
                    bits.extend(parts);
                }

                tokens.drain(i + 1..j);
            }
        }
        i += 1;
    }
}

fn fix_endblock_labels(tokens: &mut Vec<Token>) {
    let mut block_stack = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let update = match &tokens[i].token_type {
            TokenType::Block { bits } if bits[0] == "block" => {
                let label = bits.get(1).cloned();
                block_stack.push((i, label));
                None
            }
            TokenType::Block { bits } if bits[0] == "endblock" => {
                if let Some((start, label)) = block_stack.pop() {
                    if bits.len() == 1 || (bits.len() == 2 && label.as_ref() == bits.get(1)) {
                        let same_line = tokens[start].lineno == tokens[i].lineno;
                        Some(if same_line {
                            vec!["endblock".to_string()]
                        } else {
                            vec!["endblock".to_string(), label.unwrap()]
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
            tokens[i].token_type = TokenType::Block { bits: new_bits };
        }
        i += 1;
    }
}

fn unindent_extends_and_blocks(tokens: &mut Vec<Token>) {
    let mut after_extends = false;
    let mut block_depth = 0;

    for i in 0..tokens.len() {
        match &tokens[i].token_type {
            TokenType::Block { bits } => {
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
        if let TokenType::Text { contents } = &mut tokens[index - 1].token_type {
            *contents = contents.trim_end_matches(&[' ', '\t']).to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // fix_start_end_whitespace

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
    fn test_format_load_trailing_empty_lines_left() {
        let formatted = format("{% load albumen %}\n\n{% albu %}\n", None);
        assert_eq!(formatted, "{% load albumen %}\n\n{% albu %}\n");
    }

    #[test]
    fn test_format_load_admin_static_migrated() {
        let formatted = format("{% load admin_static %}\n", Some((2, 1)));
        assert_eq!(formatted, "{% load static %}\n");
    }

    #[test]
    fn test_format_load_admin_static_not_migrated() {
        let formatted = format("{% load admin_static %}\n", Some((2, 0)));
        assert_eq!(formatted, "{% load admin_static %}\n");
    }

    #[test]
    fn test_format_load_staticfiles_migrated() {
        let formatted = format("{% load staticfiles %}\n", Some((2, 1)));
        assert_eq!(formatted, "{% load static %}\n");
    }

    #[test]
    fn test_format_load_staticfiles_not_migrated() {
        let formatted = format("{% load staticfiles %}\n", Some((2, 0)));
        assert_eq!(formatted, "{% load staticfiles %}\n");
    }

    // fix_endblock_labels

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

    // unindent_extends_and_blocks

    #[test]
    fn test_format_extends_unindented() {
        let formatted = format("  {% extends 'egg.html' %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n");
    }

    #[test]
    fn test_format_top_level_blocks_unindented() {
        let formatted = format(
            "{% extends 'egg.html' %}\n  {% block yolk %}\n    yellow\n  {% endblock yolk %}\n",
            None,
        );
        assert_eq!(
            formatted,
            "{% extends 'egg.html' %}\n{% block yolk %}\n    yellow\n{% endblock yolk %}\n"
        );
    }

    #[test]
    fn test_format_second_level_blocks_indented() {
        let formatted = format("{% extends 'egg.html' %}\n{% block yolk %}\n  {% block white %}\n    protein\n  {% endblock white %}\n{% endblock yolk %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n{% block yolk %}\n  {% block white %}\n    protein\n  {% endblock white %}\n{% endblock yolk %}\n");
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
        let formatted = format("{% extends 'egg.html' %}\n  {% block yolk %}\n  yellow\n  {% endblock yolk %}\n  {% block white %}\n    protein\n  {% endblock white %}\n", None);
        assert_eq!(formatted, "{% extends 'egg.html' %}\n{% block yolk %}\n  yellow\n{% endblock yolk %}\n{% block white %}\n    protein\n{% endblock white %}\n");
    }

    // format output phase

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
}

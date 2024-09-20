mod cli;

use clap::Parser;
use regex::Regex;
use std::fs;
use std::sync::OnceLock;

fn main() {
    let args = cli::Args::parse();

    let mut changed = false;
    for filename in &args.filenames {
        let content = fs::read_to_string(filename).expect("Could not open {file}");
        let formatted = format(&content);
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

static TAG_RE: OnceLock<Regex> = OnceLock::new();

fn get_tag_re() -> &'static Regex {
    TAG_RE.get_or_init(|| Regex::new(r"(\{%.*?%\}|\{\{.*?\}\}|\{#.*?#\})").unwrap())
}

const BLOCK_TAG_START: &str = "{%";
const VARIABLE_TAG_START: &str = "{{";
const COMMENT_TAG_START: &str = "{#";

#[derive(Debug, Clone, Copy, PartialEq)]
enum TokenType {
    TEXT,
    VAR,
    BLOCK,
    COMMENT,
}

#[derive(Debug)]
struct Token {
    token_type: TokenType,
    contents: String,
    #[allow(dead_code)]
    position: (usize, usize),
    #[allow(dead_code)]
    lineno: usize,
}

fn lex(template_string: &str) -> Vec<Token> {
    let mut result = Vec::new();
    let mut verbatim = None;
    let mut lineno = 1;
    let mut last_end = 0;

    for cap in get_tag_re().captures_iter(template_string) {
        let token_match = cap.get(0).unwrap();
        let (start, end) = (token_match.start(), token_match.end());

        if start > last_end {
            let text = &template_string[last_end..start];
            result.push(create_token(
                text,
                (last_end, start),
                lineno,
                false,
                &mut verbatim,
            ));
            lineno += text.matches('\n').count();
        }

        let token_string = token_match.as_str();
        result.push(create_token(
            token_string,
            (start, end),
            lineno,
            true,
            &mut verbatim,
        ));
        lineno += token_string.matches('\n').count();

        last_end = end;
    }

    if last_end < template_string.len() {
        let text = &template_string[last_end..];
        result.push(create_token(
            text,
            (last_end, template_string.len()),
            lineno,
            false,
            &mut verbatim,
        ));
    }

    result
}

fn create_token(
    token_string: &str,
    position: (usize, usize),
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
                        token_type: TokenType::TEXT,
                        contents: token_string.to_string(),
                        position,
                        lineno,
                    };
                }
                *verbatim = None;
            } else if content.starts_with("verbatim") {
                *verbatim = Some(format!("end{}", content));
            }
            Token {
                token_type: TokenType::BLOCK,
                contents: content.to_string(),
                position,
                lineno,
            }
        } else if verbatim.is_none() {
            if token_string.starts_with(VARIABLE_TAG_START) {
                Token {
                    token_type: TokenType::VAR,
                    contents: content.to_string(),
                    position,
                    lineno,
                }
            } else {
                debug_assert!(token_string.starts_with(COMMENT_TAG_START));
                Token {
                    token_type: TokenType::COMMENT,
                    contents: content.to_string(),
                    position,
                    lineno,
                }
            }
        } else {
            Token {
                token_type: TokenType::TEXT,
                contents: token_string.to_string(),
                position,
                lineno,
            }
        }
    } else {
        Token {
            token_type: TokenType::TEXT,
            contents: token_string.to_string(),
            position,
            lineno,
        }
    }
}

fn format(content: &str) -> String {
    // Lex
    let mut tokens = lex(content);

    // Token-fixing passes
    merge_load_tags(&mut tokens);
    fix_endblock_labels(&mut tokens);

    // Build result
    let mut result = String::new();
    for token in tokens {
        match token.token_type {
            TokenType::TEXT => result.push_str(&token.contents),
            TokenType::VAR => {
                result.push_str("{{ ");
                result.push_str(&token.contents);
                result.push_str(" }}");
            }
            TokenType::BLOCK => {
                result.push_str("{% ");
                result.push_str(&token.contents);
                result.push_str(" %}");
            }
            TokenType::COMMENT => {
                result.push_str("{# ");
                result.push_str(&token.contents);
                result.push_str(" #}");
            }
        }
    }
    result
}

fn merge_load_tags(tokens: &mut Vec<Token>) {
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].token_type == TokenType::BLOCK && tokens[i].contents.starts_with("load ") {
            let mut j = i + 1;
            let mut to_merge = vec![i];
            while j < tokens.len() {
                match tokens[j].token_type {
                    TokenType::TEXT if tokens[j].contents.trim().is_empty() => j += 1,
                    TokenType::BLOCK if tokens[j].contents.starts_with("load ") => {
                        to_merge.push(j);
                        j += 1;
                    }
                    _ => break,
                }
            }
            let mut parts = Vec::new();
            for &idx in &to_merge {
                parts.extend(tokens[idx].contents.split_whitespace().skip(1));
            }
            parts.sort_unstable();
            parts.dedup();
            tokens[i].contents = format!("load {}", parts.join(" "));
            tokens.drain(i + 1..j);
        }
        i += 1;
    }
}

fn fix_endblock_labels(tokens: &mut Vec<Token>) {
    let mut block_stack = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].token_type == TokenType::BLOCK {
            if tokens[i].contents.starts_with("block ") {
                let label = tokens[i].contents.split_whitespace().nth(1).unwrap_or("");
                block_stack.push((i, label.to_string()));
            } else if tokens[i].contents.starts_with("endblock") {
                if let Some((start, label)) = block_stack.pop() {
                    let parts: Vec<&str> = tokens[i].contents.split_whitespace().collect();
                    if parts.len() == 1 || (parts.len() == 2 && parts[1] == label) {
                        let same_line = tokens[start].lineno == tokens[i].lineno;
                        tokens[i].contents = if same_line {
                            "endblock".to_string()
                        } else {
                            format!("endblock {}", label)
                        };
                    }
                }
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_spaces_added() {
        let formatted = format("a {{var}} {%tag%} {#comment#}");
        assert_eq!(formatted, "a {{ var }} {% tag %} {# comment #}");
    }

    #[test]
    fn test_format_spaces_removed() {
        let formatted = format("a {{  var  }} {%  tag  %} {#  comment  #}");
        assert_eq!(formatted, "a {{ var }} {% tag %} {# comment #}");
    }

    #[test]
    fn test_format_verbatim_left() {
        let formatted = format("a {% verbatim %} {{var}} {%tag%} {#comment#} {% endverbatim %}");
        assert_eq!(
            formatted,
            "a {% verbatim %} {{var}} {%tag%} {#comment#} {% endverbatim %}"
        );
    }

    #[test]
    fn test_format_load_sorted() {
        let formatted = format("{% load z y x %}");
        assert_eq!(formatted, "{% load x y z %}");
    }

    #[test]
    fn test_format_load_whitespace_cleaned() {
        let formatted = format("{% load   x  y %}");
        assert_eq!(formatted, "{% load x y %}");
    }

    #[test]
    fn test_format_load_consecutive_merged() {
        let formatted = format("{% load x %}{% load y %}");
        assert_eq!(formatted, "{% load x y %}");
    }

    #[test]
    fn test_format_load_consecutive_space_merged() {
        let formatted = format("{% load x %} {% load y %}");
        assert_eq!(formatted, "{% load x y %}");
    }

    #[test]
    fn test_format_load_consecutive_newline_merged() {
        let formatted = format("{% load x %}\n{% load y %}");
        assert_eq!(formatted, "{% load x y %}");
    }

    #[test]
    fn test_format_endblock_broken() {
        let formatted = format("{% endblock %}");
        assert_eq!(formatted, "{% endblock %}");
    }

    #[test]
    fn test_format_endblock_broken_nesting() {
        let formatted = format("{% block a %}\n{% endblock b %}");
        assert_eq!(formatted, "{% block a %}\n{% endblock b %}");
    }

    #[test]
    fn test_format_endblock_label_added() {
        let formatted = format("{% block h %}\n{% endblock %}");
        assert_eq!(formatted, "{% block h %}\n{% endblock h %}");
    }

    #[test]
    fn test_format_endblock_label_added_nested() {
        let formatted = format("{% block h %}\n{% block i %}\n{% endblock %}\n{% endblock %}");
        assert_eq!(
            formatted,
            "{% block h %}\n{% block i %}\n{% endblock i %}\n{% endblock h %}"
        );
    }

    #[test]
    fn test_format_endblock_label_removed() {
        let formatted = format("{% block h %}i{% endblock h %}");
        assert_eq!(formatted, "{% block h %}i{% endblock %}");
    }
}

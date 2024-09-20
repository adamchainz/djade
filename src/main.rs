mod cli;

use clap::Parser;
use regex::Regex;
use std::fs;
use std::io::{self, Read, Write};
use std::sync::OnceLock;

fn main() {
    let args = cli::Args::parse();

    if args.filenames.is_empty() {
        eprintln!("Error: No filenames provided");
        std::process::exit(1);
    }

    for filename in &args.filenames {
        let content = if filename == "-" {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .expect("Failed to read from stdin");
            buffer
        } else {
            fs::read_to_string(filename).expect("Could not read file")
        };

        let tokens = lex(&content);
        // call format and write result back into file
        let formatted = format(&tokens);

        if filename == "-" {
            io::stdout()
                .write_all(formatted.as_bytes())
                .expect("Failed to write to stdout");
        } else {
            fs::write(filename, formatted).expect("Could not write file");
        }
    }
}

// Lexer based on a Rust translation of Django’s lexer per:
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

fn format(tokens: &[Token]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_and_format() {
        let input =
            "Text {{ var }} {% block %} {# comment #} {% verbatim %} {{ raw }} {% endverbatim %}";
        let tokens = lex(input);
        let formatted = format(&tokens);
        assert_eq!(formatted, input);
    }

    #[test]
    fn test_create_token() {
        let mut verbatim = None;
        assert_eq!(
            create_token("text", (0, 4), 1, false, &mut verbatim).token_type,
            TokenType::TEXT
        );
        assert_eq!(
            create_token("{{ var }}", (0, 9), 1, true, &mut verbatim).token_type,
            TokenType::VAR
        );
        assert_eq!(
            create_token("{% block %}", (0, 11), 1, true, &mut verbatim).token_type,
            TokenType::BLOCK
        );
        assert_eq!(
            create_token("{# comment #}", (0, 13), 1, true, &mut verbatim).token_type,
            TokenType::COMMENT
        );

        verbatim = Some("endverbatim".to_string());
        assert_eq!(
            create_token("{% verbatim %}", (0, 14), 1, true, &mut verbatim).token_type,
            TokenType::TEXT
        );
        assert_eq!(
            create_token("{% endverbatim %}", (0, 16), 1, true, &mut verbatim).token_type,
            TokenType::BLOCK
        );
    }
}

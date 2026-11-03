//! Property-based tests that run djade::format() on generated template-like
//! input, checking invariants that should hold for any template.

use djade::format;
use proptest::prelude::*;
use regex::Regex;
use std::sync::LazyLock;

const TARGET_VERSIONS: [Option<(u8, u8)>; 4] = [None, Some((3, 1)), Some((4, 2)), Some((6, 1))];

/// Fragments of Django template syntax, plus some awkward text, to splice
/// together. Random strings would rarely form tags.
const FRAGMENTS: &[&str] = &[
    // Delimiters, including Jinja whitespace control
    "{%",
    "%}",
    "{{",
    "}}",
    "{#",
    "#}",
    "{%-",
    "-%}",
    "{{-",
    "-}}",
    "{%+",
    "{",
    "}",
    "%",
    "#",
    // Tag names
    "extends",
    "block",
    "endblock",
    "partialdef",
    "endpartialdef",
    "inline",
    "load",
    "from",
    "comment",
    "endcomment",
    "verbatim",
    "endverbatim",
    "if",
    "elif",
    "else",
    "endif",
    "and",
    "or",
    "not",
    "is",
    "in",
    "==",
    "!=",
    "with",
    "endwith",
    "as",
    "trans",
    "translate",
    "blocktrans",
    "blocktranslate",
    "endblocktrans",
    "count",
    "trimmed",
    "plural",
    "ifequal",
    "endifequal",
    "ifnotequal",
    "endifnotequal",
    "staticfiles",
    "admin_static",
    "static",
    "i18n",
    "only",
    // Filters and arguments
    "|",
    ":",
    "|length_is:1",
    "|length_is:n",
    "|length",
    "|json_script:\"\"",
    "|json_script:''",
    "|default:\"a b\"",
    "_(\"x\")",
    "-1",
    "1",
    "=",
    ".",
    ",",
    // Strings
    "\"",
    "'",
    "\"a  b\"",
    "'a  b'",
    // Whitespace
    " ",
    "  ",
    "\t",
    "\n",
    "\n\n",
    "\r\n",
    "\r",
    // Text
    "<p>",
    "</p>",
    "é",
    "日本",
    "🚂",
];

fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => prop::sample::select(FRAGMENTS).prop_map(str::to_string),
        1 => "[a-z]{1,3}",
    ]
}

/// Whole tags, so that well-formed syntax is common.
fn tag() -> impl Strategy<Value = String> {
    let inner = prop::collection::vec(fragment(), 0..6).prop_map(|parts| parts.join(" "));
    (
        prop::sample::select(&[("{%", "%}"), ("{{", "}}"), ("{#", "#}")][..]),
        prop::sample::select(&["", " ", "  "][..]),
        inner,
        prop::sample::select(&["", " ", "  "][..]),
    )
        .prop_map(|((open, close), pad1, inner, pad2)| format!("{open}{pad1}{inner}{pad2}{close}"))
}

fn template() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            3 => tag(),
            2 => fragment(),
        ],
        0..30,
    )
    .prop_map(|parts| parts.concat())
}

// Same as djade’s tag regex.
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\{%.*?%\}|\{\{.*?\}\}|\{#.*?#\})").unwrap());

/// Text outside of tags, without whitespace, which djade should not change.
fn non_tag_text(template: &str) -> String {
    TAG_RE
        .replace_all(template, "")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

proptest! {
    // Override with the PROPTEST_CASES environment variable for longer runs.
    #![proptest_config(ProptestConfig::with_cases(
        std::env::var("PROPTEST_CASES").ok().and_then(|c| c.parse().ok()).unwrap_or(2_000)
    ))]

    #[test]
    fn idempotent(input in template()) {
        for target_version in TARGET_VERSIONS {
            let once = format(&input, target_version);
            let twice = format(&once, target_version);
            prop_assert_eq!(&twice, &once, "target version {:?}", target_version);
        }
    }

    #[test]
    fn text_outside_tags_preserved(input in template()) {
        let expected = non_tag_text(&input);
        for target_version in TARGET_VERSIONS {
            let formatted = format(&input, target_version);
            prop_assert_eq!(
                &non_tag_text(&formatted),
                &expected,
                "target version {:?}, formatted: {:?}",
                target_version,
                formatted,
            );
        }
    }

    #[test]
    fn arbitrary_strings_do_not_panic(input in any::<String>()) {
        for target_version in TARGET_VERSIONS {
            format(&input, target_version);
        }
    }
}

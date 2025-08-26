use clap::Parser;
use regex::Regex;
use std::fs;
use std::sync::LazyLock;

#[derive(Parser, Debug)]
#[command(
    author,
    name = "djade",
    about = "A Django template formatter.",
    version
)]
pub struct Args {
    #[arg(required = true, help = "Filenames to format, or '-' for stdin.")]
    pub filenames: Vec<String>,

    #[arg(
        long,
        default_value = "auto",
        // Versions also need adding below
        value_parser = ["auto", "2.1", "2.2", "3.0", "3.1", "3.2", "4.1", "4.2", "5.0", "5.1", "5.2"],
        help = "The version of Django to target.",
    )]
    pub target_version: String,

    #[arg(
        long,
        help = "Avoid writing any formatted files back. Instead, exit with a non-zero status code if any files would have been modified, and zero otherwise."
    )]
    pub check: bool,
}

#[derive(Debug, PartialEq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}

impl Version {
    pub fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }

    pub fn as_tuple(&self) -> (u8, u8) {
        (self.major, self.minor)
    }
}

static DJANGO_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?xi)
        ^django
        \s*
        (?:
            \[[^\]]+\]
            \s*
        )?
        (?:==|~=|>=)
        \s*
        (?P<major>[0-9]+)
        \.
        (?P<minor>[0-9]+)
        (?:
            (?:a|b|rc)
            [0-9]+
        |
            \.
            [0-9]+
        )?
        (?:
            \s*,\s*
            (?:<|<=)
            \s*
            [0-9]+
            (?:
                \.
                [0-9]+
                (?:
                    \.
                    [0-9]+
                )?
            )?
        )?
        ",
    )
    .unwrap()
});

pub fn get_target_version(version_str: &str) -> Option<Version> {
    if version_str != "auto" {
        return parse_version_string(version_str);
    }

    detect_version_from_pyproject_toml("pyproject.toml")
}

fn parse_version_string(version_str: &str) -> Option<Version> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() != 2 {
        return None;
    }

    let major = parts[0].parse::<u8>().ok()?;
    let minor = parts[1].parse::<u8>().ok()?;

    Some(Version::new(major, minor))
}

const SUPPORTED_TARGET_VERSIONS: &[(u8, u8)] = &[
    (2, 1),
    (2, 2),
    (3, 0),
    (3, 1),
    (3, 2),
    (4, 1),
    (4, 2),
    (5, 0),
    (5, 1),
    (5, 2),
];

fn detect_version_from_pyproject_toml(path: &str) -> Option<Version> {
    let content = fs::read_to_string(path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;

    let dependencies = config.get("project")?.get("dependencies")?.as_array()?;

    for dep in dependencies {
        if let Some(dep_str) = dep.as_str() {
            if let Some(version) = parse_django_dependency(dep_str) {
                if SUPPORTED_TARGET_VERSIONS.contains(&version.as_tuple()) {
                    eprintln!(
                        "Detected Django version from pyproject.toml: {}.{}",
                        version.major, version.minor
                    );
                    return Some(version);
                }
            }
        }
    }

    None
}

fn parse_django_dependency(dep_str: &str) -> Option<Version> {
    let lowercase_dep = dep_str.to_lowercase();
    let captures = DJANGO_VERSION_RE.captures(&lowercase_dep)?;

    let major = captures.name("major")?.as_str().parse::<u8>().ok()?;
    let minor = captures.name("minor")?.as_str().parse::<u8>().ok()?;

    Some(Version::new(major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn verify_cli() {
        Args::command().debug_assert()
    }

    #[test]
    fn test_cli_with_args() {
        let args = Args::parse_from(["djade", "file1.html", "file2.html"]);
        assert_eq!(args.filenames, vec!["file1.html", "file2.html"]);
    }

    #[test]
    fn test_help_option() {
        let mut app = Args::command();
        let help_output = app.render_help().to_string();
        assert!(help_output.contains("A Django template formatter."));
        assert!(help_output.contains("Usage: djade [OPTIONS] <FILENAMES>..."));
    }

    #[test]
    fn test_target_version_default() {
        let args = Args::parse_from(["djade", "file1.html"]);
        assert_eq!(args.target_version, "auto");
    }

    #[test]
    fn test_target_version_set() {
        let args = Args::parse_from(["djade", "--target-version", "5.1", "file1.html"]);
        assert_eq!(args.target_version, "5.1");
    }

    #[test]
    fn test_target_version_auto() {
        let args = Args::parse_from(["djade", "--target-version", "auto", "file1.html"]);
        assert_eq!(args.target_version, "auto");
    }

    #[test]
    fn test_parse_version_string() {
        assert_eq!(parse_version_string("4.2"), Some(Version::new(4, 2)));
        assert_eq!(parse_version_string("5.1"), Some(Version::new(5, 1)));
        assert_eq!(parse_version_string("invalid"), None);
        assert_eq!(parse_version_string("4"), None);
        assert_eq!(parse_version_string("4.2.1"), None);
    }

    #[test]
    fn test_get_target_version_explicit() {
        assert_eq!(get_target_version("4.2"), Some(Version::new(4, 2)));
        assert_eq!(get_target_version("5.1"), Some(Version::new(5, 1)));
    }

    #[test]
    fn test_get_target_version_auto_fallback() {
        // Uses Djade’s own pyproject.toml, which doesn’t depend on Django
        let result = get_target_version("auto");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_django_dependency() {
        assert_eq!(
            parse_django_dependency("django>=4.2"),
            Some(Version::new(4, 2))
        );
        assert_eq!(
            parse_django_dependency("Django==5.1.0"),
            Some(Version::new(5, 1))
        );
        assert_eq!(
            parse_django_dependency("django~=4.1"),
            Some(Version::new(4, 1))
        );
        assert_eq!(
            parse_django_dependency("django[extra]>=4.2"),
            Some(Version::new(4, 2))
        );
        assert_eq!(
            parse_django_dependency("django >= 4.2.1"),
            Some(Version::new(4, 2))
        );
        assert_eq!(
            parse_django_dependency("django>=4.2,<5.0"),
            Some(Version::new(4, 2))
        );
        assert_eq!(parse_django_dependency("requests>=2.0"), None);
        assert_eq!(parse_django_dependency("invalid"), None);
    }

    #[test]
    fn test_detect_version_from_pyproject_toml() {
        let temp_dir = tempdir().unwrap();
        let pyproject_path = temp_dir.path().join("pyproject.toml");

        // Test with Django dependency
        let pyproject_content = r#"
[project]
dependencies = [
    "django>=4.2",
    "requests>=2.0",
]
"#;

        fs::write(&pyproject_path, pyproject_content).unwrap();

        let result = detect_version_from_pyproject_toml(pyproject_path.to_str().unwrap());
        assert_eq!(result, Some(Version::new(4, 2)));
    }

    #[test]
    fn test_detect_version_from_pyproject_no_django() {
        let temp_dir = tempdir().unwrap();
        let pyproject_path = temp_dir.path().join("pyproject.toml");

        // Test without Django dependency
        let pyproject_content = r#"
[project]
dependencies = [
    "requests>=2.0",
    "pytest>=6.0",
]
"#;

        fs::write(&pyproject_path, pyproject_content).unwrap();

        let result = detect_version_from_pyproject_toml(pyproject_path.to_str().unwrap());
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_version_from_pyproject_unsupported_version() {
        let temp_dir = tempdir().unwrap();
        let pyproject_path = temp_dir.path().join("pyproject.toml");

        // Test with unsupported Django version
        let pyproject_content = r#"
[project]
dependencies = [
    "django>=6.0",
    "requests>=2.0",
]
"#;

        fs::write(&pyproject_path, pyproject_content).unwrap();

        let result = detect_version_from_pyproject_toml(pyproject_path.to_str().unwrap());
        assert_eq!(result, None);
    }
}

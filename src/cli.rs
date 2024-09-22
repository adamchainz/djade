use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    author,
    name = "djade",
    about = "A Django template formatter.",
    version
)]
pub struct Args {
    #[arg(required = true)]
    pub filenames: Vec<String>,

    #[arg(
        long,
        default_value = "4.2",
        value_parser = parse_version,
        help = "The version of Django to target.",
    )]
    pub target_version: (u8, u8),
}

fn parse_version(s: &str) -> Result<(u8, u8), String> {
    match s {
        "2.1" => Ok((2, 1)),
        "2.2" => Ok((2, 2)),
        "3.0" => Ok((3, 0)),
        "3.1" => Ok((3, 1)),
        "3.2" => Ok((3, 2)),
        "4.1" => Ok((4, 1)),
        "4.2" => Ok((4, 2)),
        "5.0" => Ok((5, 0)),
        "5.1" => Ok((5, 1)),
        _ => Err(format!(
            "Invalid version: {}. Allowed versions are 4.2, 5.0, 5.1.",
            s
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

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
        assert_eq!(args.target_version, (4, 2));
    }

    #[test]
    fn test_target_version_set() {
        let args = Args::parse_from(["djade", "--target-version", "5.1", "file1.html"]);
        assert_eq!(args.target_version, (5, 1));
    }
}

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
        default_value = None,
        value_parser = ["2.1", "2.2", "3.0", "3.1", "3.2", "4.1", "4.2", "5.0", "5.1"],
        help = "The version of Django to target.",
    )]
    pub target_version: Option<String>,

    #[arg(
        long,
        help = "Avoid writing any formatted files back. Instead, exit with a non-zero status code if any files would have been modified, and zero otherwise."
    )]
    pub check: bool,
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
        assert_eq!(args.target_version, None);
    }

    #[test]
    fn test_target_version_set() {
        let args = Args::parse_from(["djade", "--target-version", "5.1", "file1.html"]);
        assert_eq!(args.target_version, Some(String::from("5.1")));
    }
}

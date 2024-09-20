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
        assert!(help_output.contains("Usage: djade <FILENAMES>..."));
    }
}

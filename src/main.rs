mod cli;

use clap::Parser;
use cli::get_target_version;
use djade::format;
use std::fs;
use std::io::{self, Read};

fn main() {
    let args = cli::Args::parse();
    let exit_code = main_impl(&args, &mut std::io::stderr());
    std::process::exit(exit_code)
}

fn main_impl(args: &cli::Args, writer: &mut dyn std::io::Write) -> i32 {
    let target_version: Option<(u8, u8)> =
        get_target_version(&args.target_version).map(|v| v.as_tuple());

    let mut returncode = 0;
    let mut reformatted_count = 0;
    let mut already_formatted_count = 0;
    for filename in &args.filenames {
        let (content, is_stdin) = if filename == "-" {
            let mut buffer = String::new();
            match io::stdin().read_to_string(&mut buffer) {
                Ok(_) => (buffer, true),
                Err(e) => {
                    writeln!(writer, "Error reading from stdin: {}", e).unwrap();
                    returncode = 1;
                    continue;
                }
            }
        } else {
            match fs::read_to_string(filename) {
                Ok(content) => (content, false),
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                    writeln!(writer, "{} is non-UTF-8 (not supported)", filename).unwrap();
                    returncode = 1;
                    continue;
                }
                Err(e) => {
                    writeln!(writer, "Error reading {}: {}", filename, e).unwrap();
                    returncode = 1;
                    continue;
                }
            }
        };

        let formatted = format(&content, target_version);
        if formatted != content {
            if args.check {
                let display_name = if is_stdin { "stdin" } else { filename };
                writeln!(writer, "Would reformat: {}", display_name).unwrap();
                returncode = 1;
                reformatted_count += 1;
            } else if is_stdin {
                print!("{}", formatted);
                reformatted_count += 1;
            } else {
                fs::write(filename, formatted).expect("Could not write {filename}");
                returncode = 1;
                reformatted_count += 1;
            }
        } else {
            if is_stdin && !args.check {
                print!("{}", content);
            }
            already_formatted_count += 1;
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
            target_version: "auto".to_string(),
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
            target_version: "auto".to_string(),
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
            target_version: "auto".to_string(),
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
            target_version: "auto".to_string(),
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

    #[test]
    fn test_main_impl_auto_version_with_pyproject() {
        let dir = tempdir().unwrap();
        let old_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        let pyproject_content = r#"
[project]
dependencies = [
    "django>=4.2,<5.0",
    "requests>=2.0",
]
"#;
        fs::write("pyproject.toml", pyproject_content).unwrap();

        // Create a test template file
        let template_content = "{% block content %}\nHello\n{% endblock %}";
        let template_path = dir.path().join("test.html");
        fs::write(&template_path, template_content).unwrap();

        let args = cli::Args {
            filenames: vec![template_path.to_str().unwrap().to_string()],
            target_version: "auto".to_string(),
            check: false,
        };

        let mut output = Vec::new();
        let exit_code = main_impl(&args, &mut output);

        assert_eq!(exit_code, 1);
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("1 file reformatted"));

        let reformatted_content = fs::read_to_string(&template_path).unwrap();
        assert!(reformatted_content.contains("{% endblock content %}"));

        std::env::set_current_dir(old_dir).unwrap();
    }

    #[test]
    fn test_main_impl_auto_version_without_pyproject() {
        let dir = tempdir().unwrap();
        let old_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        let template_content = "{% block content %}\nHello\n{% endblock %}";
        let template_path = dir.path().join("test.html");
        fs::write(&template_path, template_content).unwrap();

        let args = cli::Args {
            filenames: vec![template_path.to_str().unwrap().to_string()],
            target_version: "auto".to_string(),
            check: false,
        };

        let mut output = Vec::new();
        let exit_code = main_impl(&args, &mut output);

        assert_eq!(exit_code, 1);
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("1 file reformatted"));

        let reformatted_content = fs::read_to_string(&template_path).unwrap();
        assert!(reformatted_content.contains("{% endblock content %}"));

        std::env::set_current_dir(old_dir).unwrap();
    }

    #[test]
    fn test_main_impl_explicit_version() {
        let dir = tempdir().unwrap();

        let template_content = "{% block content %}\nHello\n{% endblock %}";
        let template_path = dir.path().join("test.html");
        fs::write(&template_path, template_content).unwrap();

        let args = cli::Args {
            filenames: vec![template_path.to_str().unwrap().to_string()],
            target_version: "4.2".to_string(),
            check: false,
        };

        let mut output = Vec::new();
        let exit_code = main_impl(&args, &mut output);

        assert_eq!(exit_code, 1);
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("1 file reformatted"));

        let reformatted_content = fs::read_to_string(&template_path).unwrap();
        assert!(reformatted_content.contains("{% endblock content %}"));
    }
}

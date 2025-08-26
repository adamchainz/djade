use std::fs;
use std::io::Write;
use std::process::{Child, Command, Output, Stdio};
use tempfile::tempdir;

fn run_djade(args: &[&str]) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_djade"));
    for arg in args {
        cmd.arg(arg);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start djade process")
}

fn write_to_stdin_and_wait(mut child: Child, input: &[u8]) -> Output {
    {
        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        stdin.write_all(input).expect("Failed to write to stdin");
    }
    child.wait_with_output().expect("Failed to read output")
}

#[test]
fn test_stdin_reformatted() {
    let child = run_djade(&["-"]);
    let output = write_to_stdin_and_wait(child, b"{{  engine  |  paint  }}");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{{ engine|paint }}\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "1 file reformatted\n"
    );
}

#[test]
fn test_stdin_already_formatted() {
    let child = run_djade(&["-"]);
    let output = write_to_stdin_and_wait(child, b"{{ engine|paint }}\n");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{{ engine|paint }}\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "1 file already formatted\n"
    );
}

#[test]
fn test_stdin_check_mode_needs_formatting() {
    let child = run_djade(&["--check", "-"]);
    let output = write_to_stdin_and_wait(child, b"{{  engine  |  paint  }}");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "Would reformat: stdin\n1 file would be reformatted\n"
    );
}

#[test]
fn test_stdin_check_mode_already_formatted() {
    let child = run_djade(&["--check", "-"]);
    let output = write_to_stdin_and_wait(child, b"{{ engine|paint }}\n");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "1 file already formatted\n"
    );
}

#[test]
fn test_stdin_empty_input() {
    let child = run_djade(&["-"]);
    let output = write_to_stdin_and_wait(child, b"");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "1 file already formatted\n"
    );
}

#[test]
fn test_stdin_mixed_with_files() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.html");
    fs::write(&file_path, "{{  thomas  }}").unwrap();

    let child = run_djade(&[file_path.to_str().unwrap(), "-"]);

    let output = write_to_stdin_and_wait(child, b"{{  percy  }}");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "{{ percy }}\n");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "2 files reformatted\n"
    );

    let file_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(file_content, "{{ thomas }}\n");
}

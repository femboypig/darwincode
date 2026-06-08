pub fn format_shell_output(
    stdout: &str,
    stderr: &str,
    error_field: &str,
    is_still_running_err: bool,
    is_aborted: bool,
    is_running: bool,
    running_suffix: Option<&str>,
) -> String {
    let mut output = String::new();
    if !stdout.is_empty() {
        output.push_str(stdout);
    }
    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(stderr);
    }
    if !error_field.is_empty() && error_field != "null" && !is_still_running_err {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(error_field);
    }

    if is_aborted {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("^C\n[Process terminated by user via Ctrl+C]");
    } else if is_running {
        if let Some(suffix) = running_suffix {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(suffix);
        }
    } else if output.is_empty() {
        output = "(empty output)".to_owned();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_shell_output_basic() {
        let output = format_shell_output("hello", "", "", false, false, false, None);
        assert_eq!(output, "hello");
    }

    #[test]
    fn test_format_shell_output_stderr() {
        let output = format_shell_output("out", "err", "", false, false, false, None);
        assert_eq!(output, "out\nerr");
    }

    #[test]
    fn test_format_shell_output_empty() {
        let output = format_shell_output("", "", "", false, false, false, None);
        assert_eq!(output, "(empty output)");
    }

    #[test]
    fn test_format_shell_output_aborted() {
        let output = format_shell_output("partial", "", "", false, true, false, None);
        assert!(output.contains("Ctrl+C"));
    }

    #[test]
    fn test_format_shell_output_running() {
        let output =
            format_shell_output("", "", "", false, false, true, Some("[still running]"));
        assert_eq!(output, "[still running]");
    }
}

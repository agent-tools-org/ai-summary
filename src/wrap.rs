// Wrap subcommand: run a command, compress large passing test output, preserve exit code.
// Exports: run_wrap().
// Deps: std::fs, std::io, std::path, std::process, std::time.

use std::fs::{create_dir_all, read_dir, remove_file, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const MIN_SIZE: usize = 3000;
const TEE_MIN_SIZE: usize = 500;
const TEE_DIR: &str = "/tmp/ai-summary-tee";
const TEE_MAX_SIZE_BYTES: usize = 1_048_576;
const TEE_KEEP_FILES: usize = 20;
const MAX_SLUG_LEN: usize = 40;

/// Run a command via `sh -c`, compress passing test output, preserve exit code.
pub fn run_wrap(command: &[String]) -> ! {
    if command.is_empty() {
        eprintln!("[ai-summary] wrap: no command provided");
        std::process::exit(1);
    }
    let cmd_str = command.join(" ");
    let output = Command::new("sh")
        .args(["-c", &cmd_str])
        .stdin(Stdio::inherit())
        .output()
        .unwrap_or_else(|e| {
            eprintln!("[ai-summary] wrap: failed to execute: {e}");
            std::process::exit(1);
        });
    let exit_code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Combine stdout+stderr (test runners mix both)
    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.is_empty() {
        stderr.to_string()
    } else {
        format!("{stdout}\n{stderr}")
    };

    if exit_code != 0 {
        if combined.len() >= TEE_MIN_SIZE {
            if let Some(path) = write_tee_file(&combined, &cmd_str) {
                exit_with_raw_output_and_message(&combined, &path, exit_code);
            }
        }
        exit_with_raw_output(&combined, exit_code);
    }

    if combined.len() < MIN_SIZE {
        exit_with_raw_output(&combined, exit_code);
    }

    // Parse test results from common formats
    let summary = summarize_test_output(&combined);

    if let Some(summary) = summary {
        print!("{summary}");
    } else {
        // Not recognized as test output, pass through
        print!("{combined}");
    }

    std::process::exit(exit_code);
}

/// Parse test result lines and return a compact summary.
/// Returns None if output doesn't look like test results.
fn summarize_test_output(output: &str) -> Option<String> {
    // Rust: "test result: ok. X passed; Y failed; Z ignored"
    // Node: "Tests: X passed, Y failed, Z total"
    // Python: "X passed, Y failed, Z error"
    // Go: "ok  ... (cached)" or "FAIL ..."

    let mut total_passed = 0u64;
    let mut total_failed = 0u64;
    let mut total_ignored = 0u64;
    let mut found_results = false;

    for line in output.lines() {
        // Rust test result lines
        if line.contains("passed") {
            for word in line.split_whitespace() {
                if let Some(rest) = word.strip_suffix("passed") {
                    // Handle "5passed" and standalone number before "passed"
                    if rest.is_empty() {
                        // Look for preceding number
                        continue;
                    }
                }
            }
            // Parse "N passed" patterns
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if (*part == "passed" || part.starts_with("passed")) && i > 0 {
                    if let Ok(n) = parts[i - 1].trim_end_matches(';').parse::<u64>() {
                        total_passed += n;
                        found_results = true;
                    }
                }
                if (*part == "failed" || part.starts_with("failed")) && i > 0 {
                    if let Ok(n) = parts[i - 1].trim_end_matches(';').parse::<u64>() {
                        total_failed += n;
                    }
                }
                if (*part == "ignored" || part.starts_with("ignored")) && i > 0 {
                    if let Ok(n) = parts[i - 1].trim_end_matches(';').parse::<u64>() {
                        total_ignored += n;
                    }
                }
            }
        }
    }

    if !found_results {
        return None;
    }

    // Collect failed test names for context
    let failed_tests: Vec<&str> = output
        .lines()
        .filter(|l| l.contains("FAILED") && l.starts_with("test "))
        .collect();

    let errors: Vec<&str> = output
        .lines()
        .filter(|l| l.contains("error[") || l.contains("panicked at"))
        .take(20)
        .collect();

    if total_failed > 0 || !failed_tests.is_empty() {
        let mut result = format!(
            "TESTS FAILED: {total_passed} passed, {total_failed} failed, {total_ignored} ignored."
        );
        if !failed_tests.is_empty() {
            result.push_str("\n\n");
            result.push_str(&failed_tests.join("\n"));
        }
        if !errors.is_empty() {
            result.push_str("\n\n");
            result.push_str(&errors.join("\n"));
        }
        Some(result)
    } else {
        Some(format!(
            "All tests passed: {total_passed} passed, {total_failed} failed, {total_ignored} ignored."
        ))
    }
}

fn exit_with_raw_output(output: &str, exit_code: i32) -> ! {
    print!("{output}");
    std::process::exit(exit_code);
}

fn exit_with_raw_output_and_message(output: &str, path: &str, exit_code: i32) -> ! {
    print!("{output}");
    eprintln!("[ai-summary] Full output saved to: {path}");
    std::process::exit(exit_code);
}

fn write_tee_file(raw: &str, command_slug: &str) -> Option<String> {
    let raw_bytes = raw.as_bytes();
    let dir = Path::new(TEE_DIR);
    create_dir_all(dir).ok()?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let slug = sanitize_command_slug(command_slug);
    let file_name = format!("{timestamp}_{slug}.log");
    let path = dir.join(file_name);
    let content = if raw_bytes.len() > TEE_MAX_SIZE_BYTES {
        &raw_bytes[..TEE_MAX_SIZE_BYTES]
    } else {
        raw_bytes
    };
    let mut file = File::create(&path).ok()?;
    file.write_all(content).ok()?;
    let _ = cleanup_tee_dir();
    Some(path.to_string_lossy().into_owned())
}

fn sanitize_command_slug(command_slug: &str) -> String {
    let mut slug = command_slug
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    if slug.len() > MAX_SLUG_LEN {
        slug.truncate(MAX_SLUG_LEN);
    }
    if slug.is_empty() {
        slug.push_str("command");
    }
    slug
}

fn cleanup_tee_dir() -> std::io::Result<()> {
    let mut entries: Vec<(String, PathBuf)> = read_dir(TEE_DIR)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                return None;
            }
            let name = entry.file_name().into_string().ok()?;
            if !name.ends_with(".log") {
                return None;
            }
            Some((name, entry.path()))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.len() <= TEE_KEEP_FILES {
        return Ok(());
    }
    let remove_count = entries.len() - TEE_KEEP_FILES;
    for (_, path) in entries.into_iter().take(remove_count) {
        let _ = remove_file(path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_rust_passing() {
        let output = "running 50 tests\n\
            test foo ... ok\n\
            test bar ... ok\n\
            \n\
            test result: ok. 50 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out\n";
        let result = summarize_test_output(output).unwrap();
        assert_eq!(result, "All tests passed: 50 passed, 0 failed, 3 ignored.");
    }

    #[test]
    fn summarize_rust_failing() {
        let output = "running 10 tests\n\
            test foo ... ok\n\
            test bar ... FAILED\n\
            \n\
            test result: FAILED. 9 passed; 1 failed; 0 ignored\n";
        let result = summarize_test_output(output).unwrap();
        assert!(result.starts_with("TESTS FAILED: 9 passed, 1 failed"));
        assert!(result.contains("test bar ... FAILED"));
    }

    #[test]
    fn summarize_multiple_suites() {
        let output = "test result: ok. 20 passed; 0 failed; 0 ignored\n\
            test result: ok. 30 passed; 0 failed; 2 ignored\n";
        let result = summarize_test_output(output).unwrap();
        assert_eq!(result, "All tests passed: 50 passed, 0 failed, 2 ignored.");
    }

    #[test]
    fn summarize_non_test_output() {
        let output = "Compiling foo v0.1.0\nFinished in 2.3s\n";
        assert!(summarize_test_output(output).is_none());
    }

    #[test]
    fn short_output_returns_none() {
        // Even valid test output that's short should be handled by run_wrap (MIN_SIZE check)
        // But summarize_test_output itself doesn't check size, just format
        let output = "test result: ok. 1 passed; 0 failed; 0 ignored\n";
        let result = summarize_test_output(output).unwrap();
        assert_eq!(result, "All tests passed: 1 passed, 0 failed, 0 ignored.");
    }

    #[test]
    fn sanitize_command_slug_limits_length_and_replaces() {
        let input = "cmd with spaces-and+more*chars$over_limit_1234567890";
        let sanitized = sanitize_command_slug(input);
        assert_eq!(sanitized, "cmd_with_spaces-and_more_chars_over_limi");
        assert_eq!(sanitized.len(), MAX_SLUG_LEN);
        assert!(sanitized.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    }
}

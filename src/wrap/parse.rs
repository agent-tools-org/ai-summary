//! Condense recognized test-runner totals.
//! Unknown output is passed through by the caller.

pub(super) fn summarize_test_output(output: &str) -> Option<String> {
    // Rust: "test result: ok. X passed; Y failed; Z ignored"
    // Node: "Tests: X passed, Y failed, Z total"
    // Python: "X passed, Y failed, Z error"
    // Go: "ok  ... (cached)" or "FAIL ..."

    let mut total_passed = 0u64;
    let mut total_failed = 0u64;
    let mut total_ignored = 0u64;
    let mut found_results = false;

    for line in output.lines() {
        let clean = line.trim().trim_matches('=').trim();
        let known = clean.starts_with("test result: ")
            || clean.starts_with("Tests:")
            || clean
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
                && (clean.ends_with("s") || clean.ends_with("passed"));
        if known && clean.contains("passed") {
            // Parse "N passed" patterns
            let parts: Vec<&str> = clean.split_whitespace().collect();
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

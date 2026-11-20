//! Check wrapper parsing and process argument preparation.
//! Exercises real runner result formats without launching runners.

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
fn exact_argv_preserves_spaces_and_quotes() {
    let args = ["runner", "two words", "it's", ""].map(String::from);
    let command = prepare(&args).unwrap();
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["two words", "it's", ""]
    );
}

#[test]
fn env_prefix_preserves_a_multiword_value() {
    let args = ["RUST_LOG=debug plus", "cargo", "test"].map(String::from);
    let command = prepare(&args).unwrap();
    assert_eq!(command.get_program(), "cargo");
    assert_eq!(command.get_envs().next().unwrap().1.unwrap(), "debug plus");
}

#[test]
fn assignment_recognizes_only_valid_shell_names() {
    assert_eq!(assignment("MODE=two words"), Some(("MODE", "two words")));
    assert!(assignment("bad-key=value").is_none());
}

#[test]
fn ignores_number_passed_in_unrecognized_output() {
    assert!(summarize_test_output("check: 42 passed across the wire").is_none());
}

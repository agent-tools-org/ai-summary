//! Run a test command with exact argv and preserve its status and streams.
//! Only recognized large successful runner output is compressed.

mod parse;
mod tee;
#[cfg(test)]
mod tests;

use parse::summarize_test_output;
use std::{
    io::{self, Write},
    process::{Command, ExitStatus, Stdio},
};

const MIN_SIZE: usize = 3000;
const TEE_MIN_SIZE: usize = 500;

pub fn run_wrap(args: &[String]) -> ! {
    let mut command =
        prepare(args).unwrap_or_else(|| crate::commands::fail(1, "wrap requires a command"));
    let output = command
        .stdin(Stdio::inherit())
        .output()
        .unwrap_or_else(|error| {
            let code = if error.kind() == io::ErrorKind::NotFound {
                127
            } else {
                126
            };
            crate::commands::fail(code, &format!("wrap: {error}"))
        });
    let status = exit_code(output.status);
    let size = output.stdout.len().saturating_add(output.stderr.len());
    if status == 0 && size >= MIN_SIZE {
        let combined = format!(
            "{}
{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if let Some(summary) = summarize_test_output(&combined) {
            println!("{summary}");
            std::process::exit(0);
        }
    }
    let _ = io::stdout().write_all(&output.stdout);
    let _ = io::stderr().write_all(&output.stderr);
    if status != 0 && size >= TEE_MIN_SIZE {
        if let Some(path) = tee::save(&output.stdout, &output.stderr) {
            eprintln!("[ai-summary] Failure output saved to: {}", path.display());
        }
    }
    std::process::exit(status);
}

fn prepare(args: &[String]) -> Option<Command> {
    let count = args
        .iter()
        .take_while(|arg| assignment(arg).is_some())
        .count();
    let mut command = Command::new(args.get(count)?);
    command.args(&args[count + 1..]);
    for (key, value) in args[..count].iter().filter_map(|arg| assignment(arg)) {
        command.env(key, value);
    }
    Some(command)
}

fn assignment(value: &str) -> Option<(&str, &str)> {
    let (name, value) = value.split_once('=')?;
    let mut bytes = name.bytes();
    if !bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    Some((name, value))
}

fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or_else(|| {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            128 + status.signal().unwrap_or(1)
        }
        #[cfg(not(unix))]
        {
            1
        }
    })
}

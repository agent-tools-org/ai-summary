//! Run provider tools with a deadline, bounded output, and optional stdin.
//! Uses private temporary files so child I/O cannot deadlock the parent.

use std::{
    fs::File,
    io::{Read, Seek, Write},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

#[cfg(unix)]
mod signals;
#[cfg(unix)]
use signals::ManagedChild;

pub const CONTENT_LIMIT: u64 = 8 * 1024 * 1024;
const STDERR_LIMIT: u64 = 64 * 1024;

pub fn capture(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
    limit: u64,
) -> Result<Output, String> {
    let mut stdout = tempfile::tempfile().map_err(io_error)?;
    let mut stderr = tempfile::tempfile().map_err(io_error)?;
    command
        .stdout(stdout.try_clone().map_err(io_error)?)
        .stderr(stderr.try_clone().map_err(io_error)?)
        .stdin(stdin_file(input).map_err(io_error)?);
    let mut child = ManagedChild::spawn(command).map_err(io_error)?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(io_error)? {
            break status;
        }
        if stdout.metadata().map_err(io_error)?.len() > limit {
            return Err(format!("tool output exceeds {limit} bytes"));
        }
        if stderr.metadata().map_err(io_error)?.len() > STDERR_LIMIT {
            return Err("tool diagnostics exceed byte limit".into());
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| "tool timed out".to_owned())?;
        std::thread::sleep(remaining.min(Duration::from_millis(15)));
    };
    let stdout = read_output(&mut stdout, limit)?;
    let stderr = read_output(&mut stderr, STDERR_LIMIT)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub fn available(command: &mut Command) -> bool {
    capture(command, None, Duration::from_secs(3), 4096).is_ok_and(|output| output.status.success())
}

fn stdin_file(input: Option<&[u8]>) -> std::io::Result<Stdio> {
    let Some(input) = input else {
        return Ok(Stdio::null());
    };
    let mut file = tempfile::tempfile()?;
    file.write_all(input)?;
    file.rewind()?;
    Ok(file.into())
}

fn read_output(file: &mut File, limit: u64) -> Result<Vec<u8>, String> {
    file.rewind().map_err(io_error)?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > limit {
        return Err(format!("tool output exceeds {limit} bytes"));
    }
    Ok(bytes)
}

fn io_error(error: std::io::Error) -> String {
    format!("tool I/O: {error}")
}

#[cfg(not(unix))]
struct ManagedChild(std::process::Child);

#[cfg(not(unix))]
impl ManagedChild {
    fn spawn(command: &mut Command) -> std::io::Result<Self> {
        command.spawn().map(Self)
    }
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.0.try_wait()
    }
}

#[cfg(not(unix))]
impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn terminates_on_deadline_instead_of_waiting_for_child() {
        let started = Instant::now();
        let result = capture(
            Command::new("/bin/sh").args(["-c", "/bin/sleep 10 & wait"]),
            None,
            Duration::from_millis(100),
            4096,
        );
        assert!(result.unwrap_err().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn rejects_output_over_budget_after_child_closes() {
        let result = capture(
            Command::new("/bin/echo").arg("above limit"),
            None,
            Duration::from_secs(3),
            3,
        );
        assert!(result.unwrap_err().contains("output exceeds 3 bytes"));
    }
}

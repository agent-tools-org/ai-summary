//! Terminal detection helpers for stdin/stdout.
//! Exports: is_stdin_terminal(), is_stdout_terminal().
//! Deps: std::io::IsTerminal.

use std::io::IsTerminal;

pub(crate) fn is_stdin_terminal() -> bool {
    std::io::stdin().is_terminal()
}

pub(crate) fn is_stdout_terminal() -> bool {
    std::io::stdout().is_terminal()
}

//! Paging for diff output, matching `git diff`.
//!
//! Diff output is only readable with a pager, and a user's pager choice already lives in
//! their git config (`core.pager`, commonly `delta`). Rather than inventing a second
//! configuration surface, we ask git what it would use: `git var GIT_PAGER` resolves
//! `$GIT_PAGER` -> `core.pager` -> `$PAGER` -> built-in default, exactly as git does.

use std::io::{self, IsTerminal, Write};
use std::process::{Child, Command, Stdio};

/// Decide the pager command from what `git var GIT_PAGER` reported.
///
/// `cat` is git's own idiom for "paging disabled", so it is treated as no pager rather
/// than spawning a pointless process.
pub fn resolve_pager(raw: Option<String>) -> Option<String> {
    let cmd = raw?.trim().to_string();
    if cmd.is_empty() || cmd == "cat" {
        return None;
    }
    Some(cmd)
}

/// Ask git which pager it would use. `None` when git is absent or fails.
fn git_pager() -> Option<String> {
    let out = Command::new("git")
        .args(["var", "GIT_PAGER"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// A sink for diff output: either a spawned pager or plain stdout.
pub struct Pager {
    child: Option<Child>,
}

impl Pager {
    /// Spawn the user's pager, or fall back to stdout.
    ///
    /// Paging is skipped when disabled explicitly, when stdout is not a terminal (so pipes
    /// and scripts keep their plain, parseable output), or when no pager is configured.
    pub fn new(enabled: bool) -> Self {
        if !enabled || !io::stdout().is_terminal() {
            return Self { child: None };
        }
        let Some(cmd) = resolve_pager(git_pager()) else {
            return Self { child: None };
        };

        // Run through a shell because the configured pager may carry arguments,
        // e.g. `less -FRX`. This is what git does too.
        let mut command = Command::new("sh");
        command.arg("-c").arg(&cmd).stdin(Stdio::piped());

        // Without these, plain `less` eats colors and refuses to quit on short output.
        // git sets the same defaults.
        if std::env::var_os("LESS").is_none() {
            command.env("LESS", "FRX");
        }

        match command.spawn() {
            Ok(child) => Self { child: Some(child) },
            // A broken pager must not cost the user their diff.
            Err(_) => Self { child: None },
        }
    }

    /// Close the pager's input and wait for it to exit.
    pub fn finish(mut self) {
        if let Some(mut child) = self.child.take() {
            drop(child.stdin.take());
            let _ = child.wait();
        }
    }
}

impl Write for Pager {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.child.as_mut().and_then(|c| c.stdin.as_mut()) {
            Some(stdin) => stdin.write(buf),
            None => io::stdout().write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.child.as_mut().and_then(|c| c.stdin.as_mut()) {
            Some(stdin) => stdin.flush(),
            None => io::stdout().flush(),
        }
    }
}

/// Quitting the pager early (`q` in less) closes the pipe mid-write. That is a normal
/// user action, not a failure, so `BrokenPipe` is swallowed and everything else surfaces.
pub fn ignore_broken_pipe(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_broken_pipe_when_ignored_then_ok() {
        let err = Err(io::Error::new(io::ErrorKind::BrokenPipe, "pager quit"));
        assert!(ignore_broken_pipe(err).is_ok());
    }

    #[test]
    fn given_other_io_error_when_ignored_then_still_error() {
        let err = Err(io::Error::new(io::ErrorKind::PermissionDenied, "nope"));
        assert!(ignore_broken_pipe(err).is_err());
    }
}

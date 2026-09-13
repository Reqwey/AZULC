//! Lifecycle of noninteractive background processes and their descendants.
//!
//! Windows uses a job; Linux/macOS use a new process group. Descendants are
//! terminated on cancellation, errors, and when the parent exits. Unix children
//! that explicitly leave the group (setsid/setpgid) are outside this contract.

use std::{
    io,
    process::{ExitStatus, Stdio},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
};

#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
#[cfg(unix)]
#[path = "unix.rs"]
mod platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputStream {
    Stdout,
    Stderr,
}

/// Runs without a console or stdin, forwarding both streams until fully drained.
/// Dropping the future terminates the managed group before dropping/reaping the
/// direct child. Callers never need to parse output to determine completion.
pub(crate) async fn run(
    command: &mut Command,
    on_line: impl Fn(OutputStream, String) + Sync,
) -> io::Result<ExitStatus> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    platform::configure(command);
    let mut child = command.spawn()?;
    // Declaration order matters: the guard drops before child, keeping Unix
    // process group IDs protected from reuse until group cleanup is finished.
    let mut group = platform::Guard::attach(&child)?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (status, (), ()) = tokio::try_join!(
        platform::wait(&mut child, &mut group),
        read_lines(stdout, OutputStream::Stdout, &on_line),
        read_lines(stderr, OutputStream::Stderr, &on_line),
    )?;
    Ok(status)
}

async fn read_lines<R: AsyncRead + Unpin>(
    reader: Option<R>,
    stream: OutputStream,
    on_line: &impl Fn(OutputStream, String),
) -> io::Result<()> {
    if let Some(reader) = reader {
        let mut reader = BufReader::new(reader);
        let mut bytes = Vec::new();
        while reader.read_until(b'\n', &mut bytes).await? != 0 {
            if bytes.last() == Some(&b'\n') {
                bytes.pop();
            }
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            on_line(stream, String::from_utf8_lossy(&bytes).into_owned());
            bytes.clear();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

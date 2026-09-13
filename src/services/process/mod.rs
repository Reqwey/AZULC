//! Lifecycle of noninteractive background processes and their descendants.
//!
//! Windows uses a job; Linux/macOS use a new process group. Descendants are
//! terminated on errors and when the parent exits. Cancellation can either
//! terminate or detach, allowing games to survive launcher closure. Unix children
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnDrop {
    Terminate,
    Detach,
}

/// An explicit stop kills the group, then waits for exit and drains its output.
/// The boolean reports whether a stop was handled before normal completion.
/// `Detach` preserves a running game when the monitor or launcher is closed.
pub(crate) async fn run_controlled(
    command: &mut Command,
    on_drop: OnDrop,
    on_started: impl FnOnce(u32),
    on_line: impl Fn(OutputStream, String) + Sync,
    stop: impl Future<Output = ()>,
) -> io::Result<(ExitStatus, bool)> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(on_drop == OnDrop::Terminate);
    platform::configure(command);
    let mut child = command.spawn()?;
    // Declaration order matters: the guard drops before child, keeping Unix
    // process group IDs protected from reuse until group cleanup is finished.
    // A failure while attaching must still kill the newly spawned child.
    let mut group = match platform::Guard::attach(&child, on_drop) {
        Ok(group) => group,
        Err(error) => {
            let _ = child.kill().await;
            return Err(error);
        }
    };
    on_started(
        child
            .id()
            .ok_or_else(|| io::Error::other("missing child PID"))?,
    );
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let result = tokio::try_join!(
        async {
            tokio::select! {
                biased;
                result = platform::wait(&mut child, &mut group) => result.map(|status| (status, false)),
                () = stop => {
                    group.terminate()?;
                    platform::wait(&mut child, &mut group).await.map(|status| (status, true))
                }
            }
        },
        read_lines(stdout, OutputStream::Stdout, &on_line),
        read_lines(stderr, OutputStream::Stderr, &on_line),
    );
    let (status, (), ()) = match result {
        Ok(result) => result,
        Err(error) => {
            let _ = group.terminate();
            let _ = child.kill().await;
            return Err(error);
        }
    };
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

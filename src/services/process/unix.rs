//! Safe Unix process-group management, shared by Linux and macOS.

use rustix::{
    io::Errno,
    process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid},
};
use std::{io, process::ExitStatus, time::Duration};
use tokio::process::{Child, Command};

pub(super) fn configure(command: &mut Command) {
    // setpgid happens in the child before exec, so even launcher shims inherit it.
    command.process_group(0);
}

pub(super) struct Guard(Option<Pid>);

impl Guard {
    pub(super) fn attach(child: &Child) -> io::Result<Self> {
        let pid = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(Pid::from_raw)
            .filter(|pid| *pid != Pid::INIT)
            .ok_or_else(|| io::Error::other("background process ID is unavailable"))?;
        Ok(Self(Some(pid)))
    }

    fn terminate(&mut self) -> io::Result<()> {
        if let Some(pid) = self.0 {
            match kill_process_group(pid, Signal::KILL) {
                Ok(()) | Err(Errno::SRCH) => self.0 = None,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

pub(super) async fn wait(child: &mut Child, group: &mut Guard) -> io::Result<ExitStatus> {
    let pid = group
        .0
        .ok_or_else(|| io::Error::other("background process group is unavailable"))?;
    loop {
        // WNOWAIT observes exit without reaping. Keeping the leader waitable
        // reserves its PID/PGID until we have killed remaining group members.
        // This also avoids hanging on pipes held open by an orphaned descendant.
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        ) {
            Ok(Some(_)) => break,
            Ok(None) | Err(Errno::INTR) => tokio::time::sleep(Duration::from_millis(20)).await,
            Err(error) => return Err(error.into()),
        }
    }
    group.terminate()?;
    child.wait().await
}

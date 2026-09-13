//! Keeps background processes and their children in one cancellable Windows job.

use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

pub(super) fn configure(command: &mut tokio::process::Command) {
    use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
    command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
}

pub(super) struct Guard(Option<OwnedHandle>);

impl Guard {
    pub(super) fn terminate(&mut self) -> io::Result<()> {
        if let Some(job) = &self.0 {
            // SAFETY: the owned job handle is live for this call.
            if unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(job.as_raw_handle(), 1)
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    pub(super) fn attach(
        child: &tokio::process::Child,
        on_drop: super::OnDrop,
    ) -> io::Result<Self> {
        attach_and_resume(child, on_drop).map(|handle| Self(Some(handle)))
    }
}

pub(super) async fn wait(
    child: &mut tokio::process::Child,
    group: &mut Guard,
) -> io::Result<std::process::ExitStatus> {
    let status = child.wait().await?;
    group.terminate()?;
    // Close the job before draining pipes, which descendants may still hold.
    drop(group.0.take());
    Ok(status)
}

fn attach_and_resume(
    child: &tokio::process::Child,
    on_drop: super::OnDrop,
) -> io::Result<OwnedHandle> {
    let process = child
        .raw_handle()
        .ok_or_else(|| io::Error::other("background process handle is unavailable"))?;
    // SAFETY: null pointers request an unnamed job with default security.
    let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the newly created handle is valid and ownership is transferred once.
    let job = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    if on_drop == super::OnDrop::Terminate {
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    }
    // SAFETY: handles and the correctly sized information structure are live for
    // both calls. The non-inheritable OwnedHandle closes on completion or cancel.
    unsafe {
        if SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
            || AssignProcessToJobObject(job.as_raw_handle(), process) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    resume_initial_thread(
        child
            .id()
            .ok_or_else(|| io::Error::other("missing background process PID"))?,
    )?;
    Ok(job)
}

/// The child was created suspended, so it cannot spawn descendants outside the job.
fn resume_initial_thread(pid: u32) -> io::Result<()> {
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };
    // SAFETY: snapshot flags request thread metadata, with no pointer arguments.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: snapshot is a valid newly owned handle.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    // SAFETY: entry has the expected size and lives through iteration. The child
    // is suspended, so only its initial thread exists and its PID cannot be reused.
    unsafe {
        let mut found = Thread32First(snapshot.as_raw_handle(), &mut entry);
        while found != 0 {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if thread.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let thread = OwnedHandle::from_raw_handle(thread);
                if ResumeThread(thread.as_raw_handle()) == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                return Ok(());
            }
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            found = Thread32Next(snapshot.as_raw_handle(), &mut entry);
        }
    }
    Err(io::Error::other(
        "suspended background process's initial thread was not found",
    ))
}

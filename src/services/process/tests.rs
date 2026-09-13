use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

async fn run(
    command: &mut Command,
    on_line: impl Fn(OutputStream, String) + Sync,
) -> io::Result<ExitStatus> {
    run_controlled(
        command,
        OnDrop::Terminate,
        |_| {},
        on_line,
        std::future::pending(),
    )
    .await
    .map(|(status, _)| status)
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("azulc process {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn child_command(mode: &str, directory: &Path) -> Command {
    let mut command = Command::from(fixture_command(mode, directory));
    command.current_dir(directory);
    command
}

fn fixture_command(mode: &str, directory: &Path) -> std::process::Command {
    let module = module_path!().split_once("::").unwrap().1;
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!("{module}::child_fixture"),
            "--ignored",
            "--nocapture",
        ])
        .env("AZULC_PROCESS_TEST_MODE", mode)
        .env("AZULC_PROCESS_TEST_DIR", directory);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    command
}

// Reusing the test binary avoids requiring a shell, Python, or Java on any OS.
#[test]
#[ignore = "subprocess fixture, launched by the process lifecycle tests"]
fn child_fixture() {
    use std::io::Write;
    let Ok(mode) = std::env::var("AZULC_PROCESS_TEST_MODE") else {
        return;
    };
    let directory = PathBuf::from(std::env::var_os("AZULC_PROCESS_TEST_DIR").unwrap());
    match mode.as_str() {
        "output" => {
            std::io::stdout()
                .write_all(b"stdout line\r\nnon-UTF8: \xff\nlast fragment")
                .unwrap();
            std::io::stdout().flush().unwrap();
            std::io::stderr().write_all(b"stderr line\n").unwrap();
            std::io::stderr().flush().unwrap();
            std::process::exit(7);
        }
        "leaf" => {
            std::fs::write(directory.join("leaf-ready"), std::process::id().to_string()).unwrap();
            std::thread::sleep(Duration::from_millis(1500));
            std::fs::write(directory.join("leaf-survived"), "alive").unwrap();
            // A finite fallback ensures a broken implementation cannot leak a
            // permanent test process, while inherited pipes still expose leaks.
            std::thread::sleep(Duration::from_secs(5));
        }
        "tree" | "parent-exits" => {
            let mut leaf = fixture_command("leaf", &directory).spawn().unwrap();
            let start = std::time::Instant::now();
            while !directory.join("leaf-ready").exists() {
                assert!(start.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(10));
            }
            if mode == "parent-exits" {
                // Deliberately leave an orphan holding both inherited pipes.
                std::process::exit(0);
            }
            std::fs::write(directory.join("parent-ready"), "ready").unwrap();
            let _ = leaf.wait();
            std::fs::write(directory.join("parent-exited"), "done").unwrap();
        }
        _ => panic!("unknown child mode"),
    }
    std::process::exit(0);
}

#[tokio::test]
async fn returns_exit_status_and_drains_both_streams() {
    let fixture = Fixture::new();
    let lines = Mutex::new(Vec::new());
    let status = run(&mut child_command("output", &fixture.0), |stream, line| {
        lines.lock().unwrap().push((stream, line));
    })
    .await
    .unwrap();
    assert_eq!(status.code(), Some(7));
    let lines = lines.into_inner().unwrap();
    for expected in [
        (OutputStream::Stdout, "stdout line"),
        (OutputStream::Stdout, "non-UTF8: \u{fffd}"),
        (OutputStream::Stdout, "last fragment"),
        (OutputStream::Stderr, "stderr line"),
    ] {
        assert!(
            lines
                .iter()
                .any(|(stream, line)| *stream == expected.0 && line == expected.1),
            "missing {expected:?}: {lines:?}"
        );
    }
}

async fn ready(path: &Path) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child did not become ready");
}

#[tokio::test]
async fn cancellation_terminates_descendants() {
    let fixture = Fixture::new();
    let mut command = child_command("tree", &fixture.0);
    let task = tokio::spawn(async move { run(&mut command, |_, _| {}).await });
    ready(&fixture.0.join("parent-ready")).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        !fixture.0.join("leaf-survived").exists(),
        "descendant outlived cancellation"
    );
}

#[tokio::test]
async fn parent_exit_cleans_descendants_before_draining_pipes() {
    let fixture = Fixture::new();
    let mut command = child_command("parent-exits", &fixture.0);
    let task = tokio::spawn(async move { run(&mut command, |_, _| {}).await });
    ready(&fixture.0.join("leaf-ready")).await;
    let status = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("orphan held output pipes open")
        .unwrap()
        .unwrap();
    assert!(status.success());
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!fixture.0.join("leaf-survived").exists());
}

#[tokio::test]
async fn one_cancelled_group_does_not_kill_another() {
    let first = Fixture::new();
    let second = Fixture::new();
    let mut one = child_command("tree", &first.0);
    let mut two = child_command("tree", &second.0);
    let one_task = tokio::spawn(async move { run(&mut one, |_, _| {}).await });
    let two_task = tokio::spawn(async move { run(&mut two, |_, _| {}).await });
    ready(&first.0.join("parent-ready")).await;
    ready(&second.0.join("parent-ready")).await;
    one_task.abort();
    assert!(one_task.await.unwrap_err().is_cancelled());
    ready(&second.0.join("leaf-survived")).await;
    assert!(!first.0.join("leaf-survived").exists());
    two_task.abort();
    assert!(two_task.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn spawn_failure_is_reported() {
    let fixture = Fixture::new();
    let error = run(
        &mut Command::new(fixture.0.join("missing-program")),
        |_, _| {},
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[tokio::test]
async fn explicit_stop_waits_for_exit_and_terminates_descendants() {
    let fixture = Fixture::new();
    let mut command = child_command("tree", &fixture.0);
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        run_controlled(&mut command, OnDrop::Detach, |_| {}, |_, _| {}, async {
            stopped.await.unwrap()
        })
        .await
    });
    ready(&fixture.0.join("parent-ready")).await;
    stop.send(()).unwrap();
    let (_, terminated) = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(terminated);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!fixture.0.join("leaf-survived").exists());
}

#[tokio::test]
async fn detached_game_survives_monitor_drop() {
    let fixture = Fixture::new();
    let mut command = child_command("tree", &fixture.0);
    let task = tokio::spawn(async move {
        run_controlled(
            &mut command,
            OnDrop::Detach,
            |_| {},
            |_, _| {},
            std::future::pending(),
        )
        .await
    });
    ready(&fixture.0.join("parent-ready")).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    ready(&fixture.0.join("leaf-survived")).await;
    ready(&fixture.0.join("parent-exited")).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
}

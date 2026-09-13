use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("forge processor test {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn processor(root: &Path, args: &[&str], outputs: Vec<(PathBuf, String)>) -> PlannedProcessor {
    PlannedProcessor {
        label: "fixture".into(),
        jar: root.join("processor.jar"),
        classpath: vec![],
        args: args.iter().map(|arg| (*arg).into()).collect(),
        outputs,
        mappings_prepared: false,
    }
}

#[test]
fn reads_folded_main_class_from_main_manifest_section_only() {
    assert_eq!(
        main_class("Manifest-Version: 1.0\r\nMain-Class: example.\r\n Main\r\n\r\n"),
        Some("example.Main".into())
    );
    assert_eq!(
        main_class("Manifest-Version: 1.0\n\nName: file\nMain-Class: Wrong\n"),
        None
    );
}

#[tokio::test]
async fn cached_outputs_advance_without_starting_java() {
    let root = Fixture::new();
    let output = root.0.join("valid.jar");
    std::fs::write(&output, b"valid").unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    execute(
        Path::new("nonexistent-java"),
        &root.0,
        &[processor(
            &root.0,
            &[],
            vec![(output, integrity::sha1_hex(b"valid"))],
        )],
        &tx,
    )
    .await
    .unwrap();
    let mut last = None;
    while let Ok(event) = rx.try_recv() {
        if let PipelineEvent::Progress(p) = event {
            last = Some(p);
        }
    }
    assert_eq!(last.unwrap().fraction(), 1.0);
}

#[tokio::test]
async fn invalid_or_missing_outputs_are_not_reused() {
    let root = Fixture::new();
    let path = root.0.join("output.jar");
    let outputs = vec![(path.clone(), integrity::sha1_hex(b"correct"))];
    assert!(!outputs_valid(&outputs).await.unwrap());
    std::fs::write(path, b"wrong").unwrap();
    assert!(!outputs_valid(&outputs).await.unwrap());
    assert!(!outputs_valid(&[]).await.unwrap());
}

fn compile_processor(root: &Path) {
    use std::io::Write;
    let source = r#"import java.nio.file.*;
public class ProcessorFixture {
  public static void main(String[] args) throws Exception {
    System.out.println("arbitrary logs with no Forge progress markers");
    if (args[0].equals("fail")) System.exit(7);
    if (args[0].equals("bad")) return;
    if (args[0].equals("repair") && Files.exists(Paths.get(args[2]))) return;
    if (args[0].equals("sleep")) {
      Files.write(Paths.get(args[1]), "started".getBytes("UTF-8"));
      Thread.sleep(1500);
      Files.write(Paths.get(args[2]), "alive".getBytes("UTF-8"));
      return;
    }
    if (args[0].equals("second") && !new String(Files.readAllBytes(Paths.get(args[1])), "UTF-8").equals("first")) System.exit(9);
    Files.write(Paths.get(args[2]), args[0].getBytes("UTF-8"));
  }
}"#;
    std::fs::write(root.join("ProcessorFixture.java"), source).unwrap();
    let mut command = std::process::Command::new("javac");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::services::java::CREATE_NO_WINDOW);
    }
    let output = command
        .arg("-d")
        .arg(root)
        .arg(root.join("ProcessorFixture.java"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut jar = zip::ZipWriter::new(std::fs::File::create(root.join("processor.jar")).unwrap());
    jar.start_file(
        "META-INF/MANIFEST.MF",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    jar.write_all(b"Manifest-Version: 1.0\r\nMain-Class: ProcessorFixture\r\n\r\n")
        .unwrap();
    jar.start_file(
        "ProcessorFixture.class",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    jar.write_all(&std::fs::read(root.join("ProcessorFixture.class")).unwrap())
        .unwrap();
    jar.finish().unwrap();
}

#[tokio::test]
#[ignore = "requires java and javac on PATH; exercises real Java processes"]
async fn java_processors_are_sequential_validated_and_cancellable() {
    let root = Fixture::new();
    compile_processor(&root.0);
    let first = root.0.join("first.txt");
    let second = root.0.join("second.txt");
    let first_text = first.to_str().unwrap();
    let second_text = second.to_str().unwrap();
    let processors = vec![
        processor(
            &root.0,
            &["first", "unused", first_text],
            vec![(first.clone(), integrity::sha1_hex(b"first"))],
        ),
        processor(
            &root.0,
            &["second", first_text, second_text],
            vec![(second.clone(), integrity::sha1_hex(b"second"))],
        ),
    ];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    execute(Path::new("java"), &root.0, &processors, &tx)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&second).unwrap(), b"second");
    let mut fractions = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let PipelineEvent::Progress(p) = event {
            fractions.push(p.fraction());
        }
    }
    assert!(fractions.contains(&0.5));
    assert_eq!(fractions.last(), Some(&1.0));
    std::fs::write(&first, b"corrupt").unwrap();
    let repair = processor(
        &root.0,
        &["repair", "unused", first_text],
        vec![(first.clone(), integrity::sha1_hex(b"repair"))],
    );
    execute(Path::new("java"), &root.0, &[repair], &tx)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&first).unwrap(), b"repair");
    while rx.try_recv().is_ok() {}
    for mode in ["fail", "bad"] {
        let steps = vec![processor(
            &root.0,
            &[mode],
            vec![(root.0.join("missing"), integrity::sha1_hex(b"expected"))],
        )];
        assert!(
            execute(Path::new("java"), &root.0, &steps, &tx)
                .await
                .is_err()
        );
        while let Ok(event) = rx.try_recv() {
            if let PipelineEvent::Progress(p) = event {
                assert!(p.fraction() < 1.0);
            }
        }
    }
    let started = root.0.join("started");
    let alive = root.0.join("alive");
    let steps = vec![processor(
        &root.0,
        &["sleep", started.to_str().unwrap(), alive.to_str().unwrap()],
        vec![],
    )];
    let run_root = root.0.clone();
    let task =
        tokio::spawn(async move { execute(Path::new("java"), &run_root, &steps, &tx).await });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !started.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!alive.exists(), "cancelled processor continued executing");
}

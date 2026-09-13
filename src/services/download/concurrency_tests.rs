use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::AtomicBool,
};

struct Server {
    url: String,
    requests: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    root: PathBuf,
}

impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, max, done) = (requests.clone(), peak.clone(), stop.clone());
        let worker = std::thread::spawn(move || {
            let active = Arc::new(AtomicUsize::new(0));
            let mut clients = Vec::new();
            while !done.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        let (count, max, active) = (count.clone(), max.clone(), active.clone());
                        clients.push(std::thread::spawn(move || {
                            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                            let _ = socket.read(&mut [0; 4096]);
                            count.fetch_add(1, Ordering::SeqCst);
                            max.fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(200));
                            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc");
                            active.fetch_sub(1, Ordering::SeqCst);
                        }));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("{error}"),
                }
            }
            for client in clients {
                client.join().unwrap();
            }
        });
        let root = std::env::temp_dir().join(format!("azulc-parallel-{}", uuid::Uuid::new_v4()));
        Self {
            url,
            requests,
            peak,
            stop,
            worker: Some(worker),
            root,
        }
    }
    fn spec(&self, name: &str) -> DownloadSpec {
        DownloadSpec {
            urls: vec![self.url.clone()],
            destination: self.root.join(name),
            size: 3,
            sha1: Some("a9993e364706816aba3e25717850c26c9cd0d89d".into()),
            sha512: None,
            label: name.into(),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn concurrent_batches_reuse_one_verified_shared_file() {
    let server = Server::new();
    let spec = server.spec("shared.jar");
    let (one, two) = tokio::join!(
        download_batch(Client::new(), vec![spec.clone()], 2, |_| {}),
        download_batch(Client::new(), vec![spec.clone()], 2, |_| {}),
    );
    one.unwrap();
    two.unwrap();
    assert_eq!(server.requests.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(spec.destination).unwrap(), b"abc");
}

#[tokio::test]
async fn separate_instances_download_at_the_same_time() {
    if cpu_thread_count() < 2 {
        return;
    }
    let server = Server::new();
    let (one, two) = tokio::join!(
        download_batch(
            Client::new(),
            vec![server.spec("instance-a/mod.jar")],
            1,
            |_| {}
        ),
        download_batch(
            Client::new(),
            vec![server.spec("instance-b/mod.jar")],
            1,
            |_| {}
        ),
    );
    one.unwrap();
    two.unwrap();
    assert_eq!(server.peak.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cancelling_one_download_releases_its_target_for_the_waiting_task() {
    let server = Server::new();
    let spec = server.spec("shared.jar");
    let first_spec = spec.clone();
    let first =
        tokio::spawn(
            async move { download_batch(Client::new(), vec![first_spec], 1, |_| {}).await },
        );
    tokio::time::timeout(Duration::from_secs(3), async {
        while server.requests.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    tokio::time::timeout(
        Duration::from_secs(3),
        download_batch(Client::new(), vec![spec.clone()], 1, |_| {}),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(std::fs::read(spec.destination).unwrap(), b"abc");
    assert_eq!(server.requests.load(Ordering::SeqCst), 2);
}

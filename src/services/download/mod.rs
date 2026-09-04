//! Download orchestration and its file-integrity support.

pub(crate) mod file_ops;
pub(crate) mod integrity;
pub mod source;

use crate::domain::cpu_thread_count;
use futures::{StreamExt, stream};
use reqwest::{Client, StatusCode};
use sha1::{Digest as _, Sha1};
use sha2::Sha512;
use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
};

const REPORT_INTERVAL: Duration = Duration::from_millis(250);
// SJMCL retries transient request failures before abandoning a published URL.
// This matters for large mrpack batches: one short-lived CDN/TLS failure must
// not fail the whole installation after the other files have completed.
const MAX_ATTEMPTS_PER_URL: usize = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSpec {
    /// Candidate URLs in priority order. The next URL is tried whenever an
    /// earlier candidate fails, including on a checksum mismatch.
    pub urls: Vec<String>,
    pub destination: PathBuf,
    /// Expected size, or zero when the provider does not publish one.
    pub size: u64,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DownloadSnapshot {
    pub current: u64,
    /// Zero means at least one file has an unknown size.
    pub total: u64,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_per_second: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("{label}: at least one non-empty download URL is required")]
    EmptyUrl { label: String },
    #[error("{label}: destination does not name a file: {destination}")]
    InvalidDestination { label: String, destination: PathBuf },
    #[error("multiple downloads target the same destination: {0}")]
    DuplicateDestination(PathBuf),
    #[error("{label}: invalid {algorithm} digest `{value}`")]
    InvalidDigest {
        label: String,
        algorithm: &'static str,
        value: String,
    },
    #[error("file operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{label}: every download URL failed: {attempts:?}")]
    AllUrlsFailed {
        label: String,
        attempts: Vec<String>,
    },
    #[error("download worker failed: {0}")]
    Worker(String),
}

/// Downloads a batch with bounded concurrency and ordered URL fallback.
///
/// Progress callbacks are emitted at most about four times per second, plus an
/// initial and final snapshot. The callback deliberately has no knowledge of
/// launcher stages, so an installer can translate each snapshot into its own
/// pipeline event.
pub async fn download_batch<F>(
    client: Client,
    specs: Vec<DownloadSpec>,
    concurrency: usize,
    on_progress: F,
) -> Result<(), DownloadError>
where
    F: Fn(DownloadSnapshot) + Send + Sync + 'static,
{
    validate_specs(&specs)?;

    let files_total = specs.len();
    let total = if specs.iter().all(|spec| spec.size > 0) {
        specs.iter().map(|spec| spec.size).sum()
    } else {
        0
    };
    let tracker = Arc::new(ProgressTracker::new(total, files_total));
    let report = Arc::new(on_progress);
    report(tracker.snapshot(0.0));

    if specs.is_empty() {
        return Ok(());
    }

    let (finished_tx, mut finished_rx) = watch::channel(false);
    let report_tracker = Arc::clone(&tracker);
    let interval_report = Arc::clone(&report);
    let reporter = tokio::spawn(async move {
        let mut interval = tokio::time::interval(REPORT_INTERVAL);
        // Tokio intervals tick immediately. Consume that tick because the
        // caller already received the initial snapshot above.
        interval.tick().await;
        let mut previous = report_tracker.current.load(Ordering::Relaxed);
        let mut previous_at = Instant::now();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let now = Instant::now();
                    let current = report_tracker.current.load(Ordering::Relaxed);
                    let elapsed = now.duration_since(previous_at).as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        current.saturating_sub(previous) as f64 / elapsed
                    } else {
                        0.0
                    };
                    interval_report(report_tracker.snapshot(speed));
                    previous = current;
                    previous_at = now;
                }
                changed = finished_rx.changed() => {
                    if changed.is_err() || *finished_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });

    let limit = concurrency.clamp(1, cpu_thread_count());
    let results = stream::iter(specs.into_iter().map(|spec| {
        let client = client.clone();
        let tracker = Arc::clone(&tracker);
        async move { download_spec(&client, &spec, &tracker).await }
    }))
    .buffer_unordered(limit)
    .collect::<Vec<_>>()
    .await;

    let _ = finished_tx.send(true);
    reporter
        .await
        .map_err(|error| DownloadError::Worker(error.to_string()))?;
    report(tracker.snapshot(0.0));

    for result in results {
        result?;
    }
    Ok(())
}

#[derive(Debug)]
struct ProgressTracker {
    current: AtomicU64,
    total: u64,
    files_done: AtomicUsize,
    files_total: usize,
}

impl ProgressTracker {
    fn new(total: u64, files_total: usize) -> Self {
        Self {
            current: AtomicU64::new(0),
            total,
            files_done: AtomicUsize::new(0),
            files_total,
        }
    }

    fn snapshot(&self, bytes_per_second: f64) -> DownloadSnapshot {
        DownloadSnapshot {
            current: self.current.load(Ordering::Relaxed),
            total: self.total,
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total,
            bytes_per_second,
        }
    }
}

fn validate_specs(specs: &[DownloadSpec]) -> Result<(), DownloadError> {
    let mut destinations = HashSet::with_capacity(specs.len());
    for spec in specs {
        if spec.urls.is_empty() || spec.urls.iter().any(|url| url.trim().is_empty()) {
            return Err(DownloadError::EmptyUrl {
                label: spec.label.clone(),
            });
        }
        if spec.destination.file_name().is_none() {
            return Err(DownloadError::InvalidDestination {
                label: spec.label.clone(),
                destination: spec.destination.clone(),
            });
        }
        validate_digest::<20>(spec, "SHA-1", spec.sha1.as_deref())?;
        validate_digest::<64>(spec, "SHA-512", spec.sha512.as_deref())?;
        if !destinations.insert(destination_key(&spec.destination)) {
            return Err(DownloadError::DuplicateDestination(
                spec.destination.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_digest<const BYTES: usize>(
    spec: &DownloadSpec,
    algorithm: &'static str,
    value: Option<&str>,
) -> Result<(), DownloadError> {
    let Some(value) = value else {
        return Ok(());
    };
    if integrity::normalized_hex::<BYTES>(value).is_none() {
        return Err(DownloadError::InvalidDigest {
            label: spec.label.clone(),
            algorithm,
            value: value.to_string(),
        });
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn destination_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

#[cfg(not(target_os = "windows"))]
fn destination_key(path: &Path) -> PathBuf {
    path.to_path_buf()
}

async fn download_spec(
    client: &Client,
    spec: &DownloadSpec,
    tracker: &ProgressTracker,
) -> Result<(), DownloadError> {
    if let Some(size) = reusable_file(spec).await? {
        tracker.current.fetch_add(size, Ordering::Relaxed);
        tracker.files_done.fetch_add(1, Ordering::Relaxed);
        return Ok(());
    }

    if let Some(parent) = spec.destination.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|source| io_error(parent, source))?;
    }
    let temporary = file_ops::staging_path(&spec.destination)
        .map_err(|source| io_error(&spec.destination, source))?;
    let mut attempts = Vec::with_capacity(spec.urls.len());

    for url in &spec.urls {
        for attempt in 1..=MAX_ATTEMPTS_PER_URL {
            let attempt_bytes = AtomicU64::new(0);
            match download_from_url(client, url, spec, &temporary, tracker, &attempt_bytes).await {
                Ok(()) => {
                    tracker.files_done.fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
                Err(error) => {
                    // Failed attempts must not inflate progress when the same
                    // URL is retried or a fallback mirror downloads the bytes.
                    tracker
                        .current
                        .fetch_sub(attempt_bytes.load(Ordering::Relaxed), Ordering::Relaxed);
                    let _ = fs::remove_file(&temporary).await;

                    if error.retryable && attempt < MAX_ATTEMPTS_PER_URL {
                        tokio::time::sleep(retry_delay(attempt)).await;
                        continue;
                    }

                    let suffix = if attempt > 1 {
                        format!(" after {attempt} attempts")
                    } else {
                        String::new()
                    };
                    attempts.push(format!("{url}{suffix}: {}", error.message));
                    break;
                }
            }
        }
    }

    Err(DownloadError::AllUrlsFailed {
        label: spec.label.clone(),
        attempts,
    })
}

async fn reusable_file(spec: &DownloadSpec) -> Result<Option<u64>, DownloadError> {
    let metadata = match fs::metadata(&spec.destination).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(io_error(&spec.destination, source)),
    };
    if !metadata.is_file() || (spec.size > 0 && metadata.len() != spec.size) {
        return Ok(None);
    }

    let has_digest = spec.sha1.is_some() || spec.sha512.is_some();
    if !has_digest {
        // Equal length alone is not enough to prove that an existing file is
        // reusable. Providers without a digest are downloaded again.
        return Ok(None);
    }

    let actual = digest_file(
        &spec.destination,
        spec.sha1.is_some(),
        spec.sha512.is_some(),
    )
    .await?;
    if !digest_matches(spec.sha1.as_deref(), actual.sha1.as_deref())
        || !digest_matches(spec.sha512.as_deref(), actual.sha512.as_deref())
    {
        return Ok(None);
    }
    Ok(Some(metadata.len()))
}

async fn download_from_url(
    client: &Client,
    url: &str,
    spec: &DownloadSpec,
    temporary: &Path,
    tracker: &ProgressTracker,
    attempt_bytes: &AtomicU64,
) -> Result<(), AttemptFailure> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(AttemptFailure::request)?;
    let status = response.status();
    if !status.is_success() {
        return Err(AttemptFailure::new(
            format!("HTTP status {status}"),
            transient_status(status),
        ));
    }
    let mut body = response.bytes_stream();
    let mut file = fs::File::create(temporary).await.map_err(|error| {
        AttemptFailure::permanent(format!("could not create {}: {error}", temporary.display()))
    })?;
    let mut digests = Digests::new(spec.sha1.is_some(), spec.sha512.is_some());

    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(AttemptFailure::request)?;
        file.write_all(&chunk).await.map_err(|error| {
            AttemptFailure::permanent(format!("could not write {}: {error}", temporary.display()))
        })?;
        digests.update(&chunk);
        let count = chunk.len() as u64;
        attempt_bytes.fetch_add(count, Ordering::Relaxed);
        tracker.current.fetch_add(count, Ordering::Relaxed);
    }
    file.flush().await.map_err(|error| {
        AttemptFailure::permanent(format!("could not flush {}: {error}", temporary.display()))
    })?;
    file.sync_all().await.map_err(|error| {
        AttemptFailure::permanent(format!("could not sync {}: {error}", temporary.display()))
    })?;
    drop(file);

    let downloaded = attempt_bytes.load(Ordering::Relaxed);
    if spec.size > 0 && downloaded != spec.size {
        return Err(AttemptFailure::transient(format!(
            "size mismatch (expected {}, received {downloaded})",
            spec.size
        )));
    }
    let actual = digests.finish();
    verify_digest("SHA-1", spec.sha1.as_deref(), actual.sha1.as_deref())
        .map_err(AttemptFailure::transient)?;
    verify_digest("SHA-512", spec.sha512.as_deref(), actual.sha512.as_deref())
        .map_err(AttemptFailure::transient)?;

    file_ops::replace_file(temporary, &spec.destination)
        .await
        .map_err(|error| {
            AttemptFailure::permanent(format!(
                "could not replace {}: {error}",
                spec.destination.display()
            ))
        })
}

#[derive(Debug)]
struct AttemptFailure {
    message: String,
    retryable: bool,
}

impl AttemptFailure {
    fn new(message: String, retryable: bool) -> Self {
        Self { message, retryable }
    }

    fn transient(message: String) -> Self {
        Self::new(message, true)
    }

    fn permanent(message: String) -> Self {
        Self::new(message, false)
    }

    fn request(error: reqwest::Error) -> Self {
        let retryable = error.is_connect()
            || error.is_timeout()
            || error.is_body()
            || error.is_request()
            || error.status().is_some_and(transient_status);
        Self::new(error.to_string(), retryable)
    }
}

fn transient_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::FORBIDDEN
            | StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_EARLY
            | StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
}

fn retry_delay(completed_attempt: usize) -> Duration {
    RETRY_BASE_DELAY.saturating_mul(completed_attempt as u32)
}

fn verify_digest(
    algorithm: &str,
    expected: Option<&str>,
    actual: Option<&str>,
) -> Result<(), String> {
    if digest_matches(expected, actual) {
        Ok(())
    } else {
        Err(format!(
            "{algorithm} mismatch (expected {}, received {})",
            expected.unwrap_or("none"),
            actual.unwrap_or("none")
        ))
    }
}

fn digest_matches(expected: Option<&str>, actual: Option<&str>) -> bool {
    match (expected, actual) {
        (None, _) => true,
        (Some(expected), Some(actual)) => expected.trim().eq_ignore_ascii_case(actual.trim()),
        (Some(_), None) => false,
    }
}

fn io_error(path: impl AsRef<Path>, source: io::Error) -> DownloadError {
    DownloadError::Io {
        path: path.as_ref().to_path_buf(),
        source,
    }
}

#[derive(Default)]
struct Digests {
    sha1: Option<Sha1>,
    sha512: Option<Sha512>,
}

impl Digests {
    fn new(want_sha1: bool, want_sha512: bool) -> Self {
        Self {
            sha1: want_sha1.then(Sha1::new),
            sha512: want_sha512.then(Sha512::new),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        if let Some(hash) = &mut self.sha1 {
            hash.update(bytes);
        }
        if let Some(hash) = &mut self.sha512 {
            hash.update(bytes);
        }
    }

    fn finish(self) -> DigestResult {
        DigestResult {
            sha1: self.sha1.map(|hash| hex::encode(hash.finalize())),
            sha512: self.sha512.map(|hash| hex::encode(hash.finalize())),
        }
    }
}

struct DigestResult {
    sha1: Option<String>,
    sha512: Option<String>,
}

async fn digest_file(
    path: &Path,
    want_sha1: bool,
    want_sha512: bool,
) -> Result<DigestResult, DownloadError> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|source| io_error(path, source))?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut digests = Digests::new(want_sha1, want_sha512);
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|source| io_error(path, source))?;
        if read == 0 {
            break;
        }
        digests.update(&buffer[..read]);
    }
    Ok(digests.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(urls: Vec<&str>) -> DownloadSpec {
        DownloadSpec {
            urls: urls.into_iter().map(str::to_owned).collect(),
            destination: PathBuf::from("mods/example.jar"),
            size: 3,
            sha1: None,
            sha512: None,
            label: "example".into(),
        }
    }

    #[test]
    fn streaming_hashes_match_known_vectors() {
        let mut hashes = Digests::new(true, true);
        hashes.update(b"a");
        hashes.update(b"bc");
        let hashes = hashes.finish();
        assert_eq!(
            hashes.sha1.as_deref(),
            Some("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
        assert_eq!(
            hashes.sha512.as_deref(),
            Some(concat!(
                "ddaf35a193617abacc417349ae204131",
                "12e6fa4e89a97ea20a9eeee64b55d39a",
                "2192992a274fc1a836ba3c23a3feebbd",
                "454d4423643ce80e2a9ac94fa54ca49f"
            ))
        );
    }

    #[test]
    fn validation_rejects_missing_or_blank_urls() {
        assert!(matches!(
            validate_specs(&[spec(Vec::new())]),
            Err(DownloadError::EmptyUrl { .. })
        ));
        assert!(matches!(
            validate_specs(&[spec(vec!["https://example.invalid/file", "  "])]),
            Err(DownloadError::EmptyUrl { .. })
        ));
    }

    #[test]
    fn validation_rejects_malformed_hashes_and_duplicate_destinations() {
        let mut malformed = spec(vec!["https://example.invalid/file"]);
        malformed.sha512 = Some("not-a-digest".into());
        assert!(matches!(
            validate_specs(&[malformed]),
            Err(DownloadError::InvalidDigest {
                algorithm: "SHA-512",
                ..
            })
        ));

        let duplicate = spec(vec!["https://example.invalid/file"]);
        assert!(matches!(
            validate_specs(&[duplicate.clone(), duplicate]),
            Err(DownloadError::DuplicateDestination(_))
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn validation_rejects_case_insensitive_destination_aliases() {
        let first = spec(vec!["https://example.invalid/first"]);
        let mut second = spec(vec!["https://example.invalid/second"]);
        second.destination = PathBuf::from("MODS/EXAMPLE.JAR");
        assert!(matches!(
            validate_specs(&[first, second]),
            Err(DownloadError::DuplicateDestination(_))
        ));
    }

    #[test]
    fn retries_only_transient_http_statuses_with_bounded_backoff() {
        assert!(transient_status(StatusCode::FORBIDDEN));
        assert!(transient_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(transient_status(StatusCode::BAD_GATEWAY));
        assert!(!transient_status(StatusCode::NOT_FOUND));
        assert!(!transient_status(StatusCode::UNAUTHORIZED));

        assert_eq!(retry_delay(1), Duration::from_millis(250));
        assert_eq!(retry_delay(2), Duration::from_millis(500));
        assert_eq!(MAX_ATTEMPTS_PER_URL, 3);
    }
}

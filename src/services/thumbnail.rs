use crate::services::content::ContentKind;
use futures::{StreamExt, stream};
use image::imageops::FilterType;
use reqwest::{Client, Url, redirect};
use serde_json::Value;
use std::{
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
    time::Duration,
};
use zip::ZipArchive;

pub const SIZE: u32 = 32;
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 512 * 1024;
const REMOTE_WORKERS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub rgba: Vec<u8>,
}

pub async fn fetch_remote_batch(urls: Vec<String>) -> Vec<(String, Option<Thumbnail>)> {
    let client = match Client::builder()
        .user_agent("AZULC/0.1.0")
        .timeout(Duration::from_secs(12))
        .redirect(redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !trusted_remote_url(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
    {
        Ok(client) => client,
        Err(_) => return urls.into_iter().map(|url| (url, None)).collect(),
    };

    stream::iter(urls.into_iter().map(|url| {
        let client = client.clone();
        async move {
            let thumbnail = fetch_remote(&client, &url).await;
            (url, thumbnail)
        }
    }))
    .buffer_unordered(REMOTE_WORKERS)
    .collect()
    .await
}

async fn fetch_remote(client: &Client, value: &str) -> Option<Thumbnail> {
    let url = Url::parse(value).ok()?;
    if !trusted_remote_url(&url) {
        return None;
    }
    let response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_IMAGE_BYTES as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.ok()?;
        if bytes.len().saturating_add(chunk.len()) > MAX_IMAGE_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    normalize(&bytes)
}

pub async fn load_local(kind: ContentKind, path: PathBuf, is_directory: bool) -> Option<Thumbnail> {
    tokio::task::spawn_blocking(move || load_local_blocking(kind, &path, is_directory))
        .await
        .ok()
        .flatten()
}

fn load_local_blocking(kind: ContentKind, path: &Path, is_directory: bool) -> Option<Thumbnail> {
    if kind == ContentKind::Screenshots {
        return normalize(&read_file_limited(path, MAX_IMAGE_BYTES)?);
    }
    if is_directory {
        return load_directory_thumbnail(kind, path);
    }
    load_archive_thumbnail(kind, path)
}

fn load_directory_thumbnail(kind: ContentKind, root: &Path) -> Option<Thumbnail> {
    let declared = (kind == ContentKind::Mods)
        .then(|| declared_icon_from_directory(root))
        .flatten();
    let candidates = match kind {
        ContentKind::Worlds => &["icon.png"][..],
        ContentKind::ResourcePacks | ContentKind::DataPacks | ContentKind::ShaderPacks => {
            &["pack.png", "icon.png"][..]
        }
        ContentKind::Mods => &["icon.png", "logo.png", "pack.png"][..],
        ContentKind::Screenshots => &[][..],
    };
    declared
        .into_iter()
        .chain(candidates.iter().map(|value| value.to_string()))
        .filter_map(|relative| safe_child(root, &relative))
        .find_map(|candidate| {
            read_file_limited(&candidate, MAX_IMAGE_BYTES).and_then(|data| normalize(&data))
        })
}

fn load_archive_thumbnail(kind: ContentKind, path: &Path) -> Option<Thumbnail> {
    let file = File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let declared = (kind == ContentKind::Mods)
        .then(|| declared_icon_from_archive(&mut archive))
        .flatten();
    let candidates = match kind {
        ContentKind::ResourcePacks | ContentKind::DataPacks | ContentKind::ShaderPacks => {
            &["pack.png", "icon.png"][..]
        }
        ContentKind::Mods => &["icon.png", "logo.png", "pack.png"][..],
        ContentKind::Worlds | ContentKind::Screenshots => &[][..],
    };
    declared
        .into_iter()
        .chain(candidates.iter().map(|value| value.to_string()))
        .filter_map(|name| safe_archive_name(&name))
        .find_map(|name| {
            read_zip_entry(&mut archive, &name, MAX_IMAGE_BYTES).and_then(|data| normalize(&data))
        })
}

fn declared_icon_from_directory(root: &Path) -> Option<String> {
    for (metadata, parser) in [
        (
            "fabric.mod.json",
            parse_fabric_icon as fn(&[u8]) -> Option<String>,
        ),
        ("quilt.mod.json", parse_quilt_icon),
        ("mcmod.info", parse_legacy_forge_icon),
    ] {
        if let Some(icon) = read_file_limited(&root.join(metadata), MAX_METADATA_BYTES)
            .and_then(|data| parser(&data))
        {
            return Some(icon);
        }
    }
    for metadata in ["META-INF/mods.toml", "META-INF/neoforge.mods.toml"] {
        if let Some(icon) = read_file_limited(&root.join(metadata), MAX_METADATA_BYTES)
            .and_then(|data| parse_toml_logo(&data))
        {
            return Some(icon);
        }
    }
    None
}

fn declared_icon_from_archive(archive: &mut ZipArchive<File>) -> Option<String> {
    for (metadata, parser) in [
        (
            "fabric.mod.json",
            parse_fabric_icon as fn(&[u8]) -> Option<String>,
        ),
        ("quilt.mod.json", parse_quilt_icon),
        ("mcmod.info", parse_legacy_forge_icon),
    ] {
        if let Some(icon) =
            read_zip_entry(archive, metadata, MAX_METADATA_BYTES).and_then(|data| parser(&data))
        {
            return Some(icon);
        }
    }
    for metadata in ["META-INF/mods.toml", "META-INF/neoforge.mods.toml"] {
        if let Some(icon) = read_zip_entry(archive, metadata, MAX_METADATA_BYTES)
            .and_then(|data| parse_toml_logo(&data))
        {
            return Some(icon);
        }
    }
    None
}

fn parse_fabric_icon(data: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(data).ok()?;
    match value.get("icon")? {
        Value::String(path) => Some(path.clone()),
        Value::Object(paths) => paths
            .iter()
            .filter_map(|(size, path)| Some((size.parse::<u32>().ok()?, path.as_str()?)))
            .max_by_key(|(size, _)| *size)
            .map(|(_, path)| path.to_string()),
        _ => None,
    }
}

fn parse_quilt_icon(data: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(data)
        .ok()?
        .pointer("/quilt_loader/metadata/icon")?
        .as_str()
        .map(str::to_string)
}

fn parse_legacy_forge_icon(data: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(data).ok()?;
    let item = value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(&value);
    item.get("logoFile")
        .or_else(|| item.get("logo_file"))?
        .as_str()
        .map(str::to_string)
}

fn parse_toml_logo(data: &[u8]) -> Option<String> {
    let source = std::str::from_utf8(data).ok()?;
    source.lines().find_map(|line| {
        let line = line.split('#').next()?.trim();
        let (key, value) = line.split_once('=')?;
        matches!(key.trim(), "logoFile" | "logo_file")
            .then(|| value.trim().trim_matches(['\'', '"']).trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn read_file_limited(path: &Path, limit: usize) -> Option<Vec<u8>> {
    if path.metadata().ok()?.len() > limit as u64 {
        return None;
    }
    std::fs::read(path).ok().filter(|data| data.len() <= limit)
}

fn read_zip_entry(archive: &mut ZipArchive<File>, name: &str, limit: usize) -> Option<Vec<u8>> {
    let mut entry = archive.by_name(name).ok()?;
    if entry.size() > limit as u64 {
        return None;
    }
    let mut data = Vec::with_capacity(entry.size() as usize);
    entry
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut data)
        .ok()?;
    (data.len() <= limit).then_some(data)
}

fn safe_child(root: &Path, value: &str) -> Option<PathBuf> {
    let relative = Path::new(value.trim().trim_start_matches(['/', '\\']));
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    Some(root.join(relative))
}

fn safe_archive_name(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/");
    let path = Path::new(&normalized);
    (!normalized.is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir)))
    .then_some(normalized)
}

fn normalize(data: &[u8]) -> Option<Thumbnail> {
    let image = image::load_from_memory(data).ok()?;
    let resized = image
        .resize_to_fill(SIZE, SIZE, FilterType::Lanczos3)
        .into_rgba8();
    Some(Thumbnail {
        rgba: resized.into_raw(),
    })
}

fn trusted_remote_url(url: &Url) -> bool {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    ["forgecdn.net", "modrinth.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_thumbnail_hosts_are_restricted() {
        assert!(trusted_remote_url(
            &Url::parse("https://media.forgecdn.net/avatars/1/2.png").unwrap()
        ));
        assert!(trusted_remote_url(
            &Url::parse("https://cdn.modrinth.com/data/project/icon.png").unwrap()
        ));
        assert!(!trusted_remote_url(
            &Url::parse("https://example.com/icon.png").unwrap()
        ));
    }

    #[test]
    fn metadata_icon_paths_cannot_escape_the_resource() {
        assert!(safe_archive_name("assets/example/icon.png").is_some());
        assert!(safe_archive_name("../secret.png").is_none());
        assert!(safe_child(Path::new("pack"), "../../secret.png").is_none());
    }

    #[test]
    fn reads_supported_mod_icon_metadata() {
        assert_eq!(
            parse_fabric_icon(br#"{"icon":{"16":"small.png","128":"large.png"}}"#).as_deref(),
            Some("large.png")
        );
        assert_eq!(
            parse_quilt_icon(br#"{"quilt_loader":{"metadata":{"icon":"quilt.png"}}}"#).as_deref(),
            Some("quilt.png")
        );
        assert_eq!(
            parse_toml_logo(b"logoFile = \"forge.png\"").as_deref(),
            Some("forge.png")
        );
    }
}

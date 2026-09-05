use crate::domain::{LoaderKind, LoaderSpec};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_UNCOMPRESSED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModpackFormat {
    CurseForge,
    Modrinth,
    MultiMc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackMetadata {
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub minecraft_version: String,
    pub loader: LoaderSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModpackFile {
    CurseForge {
        project_id: u64,
        file_id: u64,
        required: bool,
    },
    Direct {
        path: PathBuf,
        urls: Vec<String>,
        sha1: String,
        sha512: String,
        size: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackPlan {
    pub format: ModpackFormat,
    pub metadata: ModpackMetadata,
    pub files: Vec<ModpackFile>,
    /// A normalized ZIP path without leading or trailing separators.
    pub overrides_prefix: String,
}

/// Reads and validates a supported modpack without blocking Iced's async runtime.
pub async fn inspect_archive(archive: PathBuf) -> Result<ModpackPlan, String> {
    tokio::task::spawn_blocking(move || {
        let fallback_name = archive
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Imported modpack")
            .to_owned();
        let file = File::open(&archive)
            .map_err(|error| format!("Failed to open modpack {}: {error}", archive.display()))?;
        inspect_reader(file, &fallback_name)
    })
    .await
    .map_err(|error| format!("Modpack inspection task failed: {error}"))?
}

/// Copies the selected override tree into an instance directory.
///
/// `prefix` may contain leading/trailing `/` or `\\`; it is normalized before
/// matching so callers cannot accidentally produce the historical `//` mismatch.
pub async fn apply_overrides(
    archive: PathBuf,
    destination: PathBuf,
    prefix: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let file = File::open(&archive)
            .map_err(|error| format!("Failed to open modpack {}: {error}", archive.display()))?;
        apply_overrides_from_reader(file, &destination, &prefix)
    })
    .await
    .map_err(|error| format!("Modpack extraction task failed: {error}"))?
}

#[derive(Debug, Clone)]
struct ArchiveEntry {
    index: usize,
    path: PathBuf,
    size: u64,
    is_dir: bool,
    unix_mode: Option<u32>,
}

#[derive(Debug, Clone)]
enum ManifestLocation {
    CurseForge(usize),
    Modrinth(usize),
    MultiMc {
        manifest_index: usize,
        config_index: usize,
        base: PathBuf,
    },
}

fn inspect_reader<R: Read + Seek>(reader: R, fallback_name: &str) -> Result<ModpackPlan, String> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    let entries = validate_archive(&mut archive)?;
    let manifest = find_manifest(&entries)?;

    match manifest {
        ManifestLocation::CurseForge(index) => {
            let manifest: CurseForgeManifest =
                read_json_entry(&mut archive, index, "manifest.json")?;
            plan_curseforge(manifest)
        }
        ManifestLocation::Modrinth(index) => {
            let manifest: ModrinthManifest =
                read_json_entry(&mut archive, index, "modrinth.index.json")?;
            plan_modrinth(manifest)
        }
        ManifestLocation::MultiMc {
            manifest_index,
            config_index,
            base,
        } => {
            let manifest: MultiMcManifest =
                read_json_entry(&mut archive, manifest_index, "mmc-pack.json")?;
            let config = read_text_entry(&mut archive, config_index, "instance.cfg")?;
            plan_multimc(manifest, &config, &base, fallback_name)
        }
    }
}

fn validate_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<Vec<ArchiveEntry>, String> {
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "Archive has {} entries; the limit is {MAX_ARCHIVE_ENTRIES}",
            archive.len()
        ));
    }

    let mut total_size = 0_u64;
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| format!("Failed to inspect ZIP entry {index}: {error}"))?;
        let path = normalize_relative_path(file.name(), "ZIP entry")?;
        total_size = total_size
            .checked_add(file.size())
            .ok_or_else(|| "Archive uncompressed size overflowed u64".to_owned())?;
        if total_size > MAX_UNCOMPRESSED_BYTES {
            return Err(format!(
                "Archive expands beyond the {} GiB safety limit",
                MAX_UNCOMPRESSED_BYTES / 1024 / 1024 / 1024
            ));
        }
        entries.push(ArchiveEntry {
            index,
            path,
            size: file.size(),
            is_dir: file.is_dir(),
            unix_mode: file.unix_mode(),
        });
    }
    Ok(entries)
}

fn find_manifest(entries: &[ArchiveEntry]) -> Result<ManifestLocation, String> {
    let root_manifest = Path::new("manifest.json");
    let root_modrinth = Path::new("modrinth.index.json");
    let mut candidates = Vec::new();
    let mut incomplete_multimc = Vec::new();

    for entry in entries.iter().filter(|entry| !entry.is_dir) {
        if entry.path == root_manifest {
            candidates.push(ManifestLocation::CurseForge(entry.index));
            continue;
        }
        if entry.path == root_modrinth {
            candidates.push(ManifestLocation::Modrinth(entry.index));
            continue;
        }
        if entry.path.file_name().and_then(|name| name.to_str()) != Some("mmc-pack.json") {
            continue;
        }

        let base = entry.path.parent().unwrap_or_else(|| Path::new(""));
        // MultiMC/Prism exports are normally rooted directly or in one wrapping
        // directory. Keep SJMCL compatibility with at most two wrapping levels.
        if base.components().count() > 2 {
            continue;
        }
        let config_path = base.join("instance.cfg");
        let config_matches = entries
            .iter()
            .filter(|candidate| !candidate.is_dir && candidate.path == config_path)
            .collect::<Vec<_>>();
        if config_matches.len() != 1 {
            incomplete_multimc.push(entry.path.clone());
            continue;
        }
        candidates.push(ManifestLocation::MultiMc {
            manifest_index: entry.index,
            config_index: config_matches[0].index,
            base: base.to_path_buf(),
        });
    }

    match candidates.len() {
        0 if !incomplete_multimc.is_empty() => Err(format!(
            "MultiMC/Prism manifest {} requires exactly one sibling instance.cfg",
            incomplete_multimc[0].display()
        )),
        0 => Err(
            "Unknown modpack format: expected manifest.json, modrinth.index.json, or mmc-pack.json with instance.cfg"
                .to_owned(),
        ),
        1 => Ok(candidates.remove(0)),
        count => Err(format!(
            "Archive contains {count} recognized modpack manifests; format is ambiguous"
        )),
    }
}

fn read_json_entry<T: for<'de> Deserialize<'de>, R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    label: &str,
) -> Result<T, String> {
    let bytes = read_entry(archive, index, label, MAX_MANIFEST_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|error| format!("Invalid {label}: {error}"))
}

fn read_text_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    label: &str,
) -> Result<String, String> {
    let bytes = read_entry(archive, index, label, MAX_MANIFEST_BYTES)?;
    String::from_utf8(bytes).map_err(|error| format!("Invalid UTF-8 in {label}: {error}"))
}

fn read_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    label: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let mut file = archive
        .by_index(index)
        .map_err(|error| format!("Failed to read {label}: {error}"))?;
    if file.size() > limit {
        return Err(format!("{label} exceeds the {limit}-byte safety limit"));
    }
    let mut bytes = Vec::with_capacity(file.size().min(limit) as usize);
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read {label}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{label} exceeds the {limit}-byte safety limit"));
    }
    Ok(bytes)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeManifest {
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    manifest_type: Option<String>,
    minecraft: CurseForgeMinecraft,
    #[serde(default)]
    files: Vec<CurseForgeFile>,
    #[serde(default = "default_overrides_prefix")]
    overrides: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeMinecraft {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<CurseForgeLoader>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Deserialize)]
struct CurseForgeFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "default_true")]
    required: bool,
}

fn default_overrides_prefix() -> String {
    "overrides".to_owned()
}

fn default_true() -> bool {
    true
}

fn plan_curseforge(manifest: CurseForgeManifest) -> Result<ModpackPlan, String> {
    if let Some(manifest_type) = manifest.manifest_type.as_deref()
        && manifest_type != "minecraftModpack"
    {
        return Err(format!(
            "Unsupported CurseForge manifest type: {manifest_type}"
        ));
    }
    ensure_file_count(manifest.files.len())?;
    let loader = parse_curseforge_loader(&manifest.minecraft.mod_loaders)?;
    let overrides_prefix = normalize_prefix(&manifest.overrides)?;

    Ok(ModpackPlan {
        format: ModpackFormat::CurseForge,
        metadata: ModpackMetadata {
            name: required_text(manifest.name, "CurseForge modpack name")?,
            version: optional_text(manifest.version),
            author: optional_text(manifest.author),
            minecraft_version: required_text(
                manifest.minecraft.version,
                "CurseForge Minecraft version",
            )?,
            loader,
        },
        files: manifest
            .files
            .into_iter()
            .map(|file| ModpackFile::CurseForge {
                project_id: file.project_id,
                file_id: file.file_id,
                required: file.required,
            })
            .collect(),
        overrides_prefix,
    })
}

fn parse_curseforge_loader(entries: &[CurseForgeLoader]) -> Result<LoaderSpec, String> {
    if entries.is_empty() {
        return Ok(vanilla_loader());
    }
    if entries.len() > 1 {
        return Err(format!(
            "CurseForge pack declares {} mod loaders; only one is supported",
            entries.len()
        ));
    }
    let entry = &entries[0];
    if !entry.primary {
        return Err("CurseForge mod loader is not marked as primary".to_owned());
    }
    loader_from_prefixed_id(&entry.id)
}

fn loader_from_prefixed_id(id: &str) -> Result<LoaderSpec, String> {
    let id = id.trim();
    let lower = id.to_ascii_lowercase();
    let prefixes = [
        ("fabric-loader-", LoaderKind::Fabric),
        ("neoforge-", LoaderKind::NeoForge),
        ("forge-", LoaderKind::Forge),
        ("fabric-", LoaderKind::Fabric),
    ];
    for (prefix, kind) in prefixes {
        if lower.starts_with(prefix) {
            let version = id[prefix.len()..].trim();
            if version.is_empty() {
                return Err(format!("Mod loader {id:?} has no version"));
            }
            return Ok(LoaderSpec {
                kind,
                version: Some(version.to_owned()),
            });
        }
    }
    Err(format!("Unsupported mod loader: {id}"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthManifest {
    #[serde(default)]
    game: Option<String>,
    version_id: String,
    name: String,
    #[serde(default)]
    files: Vec<ModrinthFile>,
    dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthFile {
    path: String,
    hashes: ModrinthHashes,
    #[serde(default)]
    env: Option<ModrinthEnvironment>,
    downloads: Vec<String>,
    file_size: u64,
}

#[derive(Debug, Deserialize)]
struct ModrinthHashes {
    sha1: String,
    sha512: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthEnvironment {
    client: String,
    #[allow(dead_code)]
    server: String,
}

fn plan_modrinth(manifest: ModrinthManifest) -> Result<ModpackPlan, String> {
    if let Some(game) = manifest.game.as_deref()
        && !game.eq_ignore_ascii_case("minecraft")
    {
        return Err(format!("Unsupported Modrinth game: {game}"));
    }
    ensure_file_count(manifest.files.len())?;
    let minecraft_version = required_text(
        manifest
            .dependencies
            .get("minecraft")
            .cloned()
            .ok_or_else(|| "Modrinth pack has no Minecraft dependency".to_owned())?,
        "Modrinth Minecraft version",
    )?;
    let loader = parse_modrinth_loader(&manifest.dependencies)?;

    let mut files = Vec::with_capacity(manifest.files.len());
    for file in manifest.files {
        if file
            .env
            .as_ref()
            .is_some_and(|environment| environment.client == "unsupported")
        {
            continue;
        }
        let path = normalize_relative_path(&file.path, "Modrinth file path")?;
        let urls = normalize_modrinth_download_urls(file.downloads, &path)?;
        if file.hashes.sha1.trim().is_empty() || file.hashes.sha512.trim().is_empty() {
            return Err(format!(
                "Modrinth file {} is missing SHA-1 or SHA-512",
                path.display()
            ));
        }
        files.push(ModpackFile::Direct {
            path,
            urls,
            sha1: file.hashes.sha1,
            sha512: file.hashes.sha512,
            size: file.file_size,
        });
    }

    Ok(ModpackPlan {
        format: ModpackFormat::Modrinth,
        metadata: ModpackMetadata {
            name: required_text(manifest.name, "Modrinth modpack name")?,
            version: optional_text(Some(manifest.version_id)),
            author: None,
            minecraft_version,
            loader,
        },
        files,
        overrides_prefix: "overrides".to_owned(),
    })
}

/// Validates every URL from an mrpack manifest and stores the URL parser's
/// canonical representation in the install plan.
///
/// In particular, this removes URL-parser-ignored ASCII tabs/newlines instead
/// of carrying those control characters into request construction and error
/// output. It never joins a URL with the manifest path or file name: mrpack
/// `downloads` entries are already complete download URLs. Candidate order is
/// retained so the generic downloader can try every published fallback.
fn normalize_modrinth_download_urls(
    downloads: Vec<String>,
    path: &Path,
) -> Result<Vec<String>, String> {
    if downloads.is_empty() {
        return Err(format!(
            "Modrinth file {} has no download URL",
            path.display()
        ));
    }

    downloads
        .into_iter()
        .map(|url| {
            let parsed = reqwest::Url::parse(url.trim())
                .map_err(|error| format!("Invalid download URL for {}: {error}", path.display()))?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(format!(
                    "Unsupported download URL scheme for {}: {}",
                    path.display(),
                    parsed.scheme()
                ));
            }
            Ok(parsed.to_string())
        })
        .collect()
}

fn parse_modrinth_loader(dependencies: &BTreeMap<String, String>) -> Result<LoaderSpec, String> {
    let mut loaders = Vec::new();
    for (key, version) in dependencies {
        let kind = match key.as_str() {
            "minecraft" => continue,
            "forge" => LoaderKind::Forge,
            "fabric-loader" => LoaderKind::Fabric,
            "neoforge" => LoaderKind::NeoForge,
            other => {
                return Err(format!(
                    "Unsupported Modrinth dependency/mod loader: {other}"
                ));
            }
        };
        loaders.push(LoaderSpec {
            kind,
            version: Some(required_text(
                version.clone(),
                &format!("Modrinth {key} version"),
            )?),
        });
    }
    match loaders.len() {
        0 => Ok(vanilla_loader()),
        1 => Ok(loaders.remove(0)),
        count => Err(format!(
            "Modrinth pack declares {count} mod loaders; only one is supported"
        )),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MultiMcManifest {
    #[serde(default)]
    components: Vec<MultiMcComponent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MultiMcComponent {
    uid: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    cached_version: Option<String>,
}

fn plan_multimc(
    manifest: MultiMcManifest,
    config: &str,
    base: &Path,
    fallback_name: &str,
) -> Result<ModpackPlan, String> {
    let config = parse_instance_config(config);
    let minecraft_components = manifest
        .components
        .iter()
        .filter(|component| component.uid == "net.minecraft")
        .collect::<Vec<_>>();
    if minecraft_components.len() != 1 {
        return Err(format!(
            "MultiMC/Prism pack must declare exactly one net.minecraft component, found {}",
            minecraft_components.len()
        ));
    }
    let minecraft_version = component_version(minecraft_components[0])?;
    let loader = parse_multimc_loader(&manifest.components)?;
    let override_path = base.join(".minecraft");

    Ok(ModpackPlan {
        format: ModpackFormat::MultiMc,
        metadata: ModpackMetadata {
            name: config_value(&config, &["name"])
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(fallback_name)
                .to_owned(),
            version: config_value(
                &config,
                &["ManagedPackVersionName", "ManagedPackVersionID", "version"],
            )
            .map(str::to_owned),
            author: config_value(&config, &["author"]).map(str::to_owned),
            minecraft_version,
            loader,
        },
        files: Vec::new(),
        overrides_prefix: archive_path_string(&override_path),
    })
}

fn parse_multimc_loader(components: &[MultiMcComponent]) -> Result<LoaderSpec, String> {
    let mut loaders = Vec::new();
    for component in components {
        let kind = match component.uid.as_str() {
            "net.minecraftforge" => Some(LoaderKind::Forge),
            "net.fabricmc.fabric-loader" => Some(LoaderKind::Fabric),
            "net.neoforged" => Some(LoaderKind::NeoForge),
            "org.quiltmc.quilt-loader" | "com.mumfrey.liteloader" => {
                return Err(format!(
                    "Unsupported MultiMC/Prism mod loader: {}",
                    component.uid
                ));
            }
            _ => None,
        };
        if let Some(kind) = kind {
            loaders.push(LoaderSpec {
                kind,
                version: Some(component_version(component)?),
            });
        }
    }
    match loaders.len() {
        0 => Ok(vanilla_loader()),
        1 => Ok(loaders.remove(0)),
        count => Err(format!(
            "MultiMC/Prism pack declares {count} mod loaders; only one is supported"
        )),
    }
}

fn component_version(component: &MultiMcComponent) -> Result<String, String> {
    component
        .version
        .as_ref()
        .or(component.cached_version.as_ref())
        .cloned()
        .and_then(|version| optional_text(Some(version)))
        .ok_or_else(|| format!("Component {} has no version", component.uid))
}

fn parse_instance_config(config: &str) -> BTreeMap<String, String> {
    config
        .trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with('#')
                || line.starts_with(';')
                || (line.starts_with('[') && line.ends_with(']'))
            {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(value);
            Some((key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn config_value<'a>(config: &'a BTreeMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|wanted| {
        config
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value.as_str())
    })
}

fn vanilla_loader() -> LoaderSpec {
    LoaderSpec {
        kind: LoaderKind::Vanilla,
        version: None,
    }
}

fn ensure_file_count(count: usize) -> Result<(), String> {
    if count > MAX_ARCHIVE_ENTRIES {
        Err(format!(
            "Modpack declares {count} dependency files; the limit is {MAX_ARCHIVE_ENTRIES}"
        ))
    } else {
        Ok(())
    }
}

fn required_text(value: String, label: &str) -> Result<String, String> {
    optional_text(Some(value)).ok_or_else(|| format!("{label} is empty"))
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn normalize_prefix(prefix: &str) -> Result<String, String> {
    let prefix = prefix
        .trim()
        .trim_matches(|character| matches!(character, '/' | '\\'));
    if prefix.is_empty() {
        return Err("Overrides prefix is empty".to_owned());
    }
    normalize_relative_path(prefix, "overrides prefix").map(|path| archive_path_string(&path))
}

fn normalize_relative_path(value: &str, label: &str) -> Result<PathBuf, String> {
    if value.contains('\0') {
        return Err(format!("Unsafe {label}: path contains NUL"));
    }
    let portable = value.replace('\\', "/");
    if portable.starts_with('/') {
        return Err(format!(
            "Unsafe {label} {value:?}: absolute paths are forbidden"
        ));
    }

    let mut normalized = PathBuf::new();
    for component in portable.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                return Err(format!(
                    "Unsafe {label} {value:?}: parent traversal is forbidden"
                ));
            }
            _ if component.contains(':') => {
                return Err(format!(
                    "Unsafe {label} {value:?}: drive-qualified paths and alternate streams are forbidden"
                ));
            }
            _ => normalized.push(component),
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(format!("Unsafe {label} {value:?}: path is empty"));
    }
    Ok(normalized)
}

fn archive_path_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn apply_overrides_from_reader<R: Read + Seek>(
    reader: R,
    destination: &Path,
    prefix: &str,
) -> Result<(), String> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    let entries = validate_archive(&mut archive)?;
    let prefix = normalize_relative_path(
        prefix
            .trim()
            .trim_matches(|character| matches!(character, '/' | '\\')),
        "overrides prefix",
    )?;
    std::fs::create_dir_all(destination).map_err(|error| {
        format!(
            "Failed to create instance directory {}: {error}",
            destination.display()
        )
    })?;

    let mut extracted_paths = HashSet::new();
    let mut extracted_bytes = 0_u64;
    for entry in entries {
        let Ok(relative) = entry.path.strip_prefix(&prefix) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        if !extracted_paths.insert(relative.to_path_buf()) {
            return Err(format!(
                "Overrides contain a duplicate path: {}",
                relative.display()
            ));
        }
        if entry
            .unix_mode
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!(
                "Overrides contain a symbolic link, which is not allowed: {}",
                entry.path.display()
            ));
        }

        let output = destination.join(relative);
        if entry.is_dir {
            std::fs::create_dir_all(&output).map_err(|error| {
                format!(
                    "Failed to create override directory {}: {error}",
                    output.display()
                )
            })?;
            continue;
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size)
            .ok_or_else(|| "Override size overflowed u64".to_owned())?;
        if extracted_bytes > MAX_UNCOMPRESSED_BYTES {
            return Err(format!(
                "Overrides expand beyond the {} GiB safety limit",
                MAX_UNCOMPRESSED_BYTES / 1024 / 1024 / 1024
            ));
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create override directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let mut input = archive.by_index(entry.index).map_err(|error| {
            format!("Failed to open override {}: {error}", entry.path.display())
        })?;
        let remaining = MAX_UNCOMPRESSED_BYTES - (extracted_bytes - entry.size);
        let mut output_file = File::create(&output)
            .map_err(|error| format!("Failed to create override {}: {error}", output.display()))?;
        let copied = std::io::copy(&mut (&mut input).take(remaining + 1), &mut output_file)
            .map_err(|error| format!("Failed to extract override {}: {error}", output.display()))?;
        drop(output_file);
        drop(input);
        if copied > remaining {
            let _ = std::fs::remove_file(&output);
            return Err(format!(
                "Overrides expand beyond the {} GiB safety limit",
                MAX_UNCOMPRESSED_BYTES / 1024 / 1024 / 1024
            ));
        }
        if copied != entry.size {
            let _ = std::fs::remove_file(&output);
            return Err(format!(
                "Override {} size mismatch: ZIP declared {}, extracted {copied}",
                entry.path.display(),
                entry.size
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn zip_fixture(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, contents) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .expect("fixture entry should start");
            writer
                .write_all(contents)
                .expect("fixture entry should be written");
        }
        writer
            .finish()
            .expect("fixture ZIP should finish")
            .into_inner()
    }

    #[test]
    fn inspects_curseforge_pack() {
        let manifest = br#"{
            "manifestType":"minecraftModpack",
            "name":"Old Forge Pack",
            "version":"1.0",
            "author":"Azulc",
            "minecraft":{"version":"1.7.10","modLoaders":[{"id":"forge-10.13.4.1614","primary":true}]},
            "files":[{"projectID":12,"fileID":34,"required":true}],
            "overrides":"overrides/"
        }"#;
        let bytes = zip_fixture(&[("manifest.json", manifest)]);
        let plan = inspect_reader(Cursor::new(bytes), "fallback").expect("pack should parse");

        assert_eq!(plan.format, ModpackFormat::CurseForge);
        assert_eq!(plan.metadata.minecraft_version, "1.7.10");
        assert_eq!(plan.metadata.loader.kind, LoaderKind::Forge);
        assert_eq!(
            plan.metadata.loader.version.as_deref(),
            Some("10.13.4.1614")
        );
        assert_eq!(plan.overrides_prefix, "overrides");
        assert!(matches!(
            plan.files.as_slice(),
            [ModpackFile::CurseForge {
                project_id: 12,
                file_id: 34,
                required: true
            }]
        ));
    }

    #[test]
    fn inspects_modrinth_pack() {
        let manifest = br#"{
            "formatVersion":1,
            "game":"minecraft",
            "versionId":"2.0",
            "name":"Fabric Pack",
            "files":[{
                "path":"mods/example.jar",
                "hashes":{"sha1":"0123456789abcdef","sha512":"fedcba9876543210"},
                "downloads":["https://cdn.modrinth.com/example.jar"],
                "fileSize":42
            }],
            "dependencies":{"minecraft":"1.20.1","fabric-loader":"0.16.10"}
        }"#;
        let bytes = zip_fixture(&[("modrinth.index.json", manifest)]);
        let plan = inspect_reader(Cursor::new(bytes), "fallback").expect("pack should parse");

        assert_eq!(plan.format, ModpackFormat::Modrinth);
        assert_eq!(plan.metadata.loader.kind, LoaderKind::Fabric);
        assert!(matches!(
            plan.files.as_slice(),
            [ModpackFile::Direct { path, size: 42, .. }] if path == Path::new("mods/example.jar")
        ));
    }

    #[test]
    fn preserves_complete_modrinth_urls_and_fallback_order() {
        const PRIMARY: &str = "https://cdn.modrinth.com/data/qANg5Jrr/versions/baNcxaPZ/e4mc_minecraft-fabric-5.4.1.jar";
        const FALLBACK: &str =
            "https://example.invalid/modrinth/e4mc_minecraft-fabric-5.4.1.jar?mirror=2";
        let manifest = format!(
            r#"{{
                "formatVersion":1,
                "game":"minecraft",
                "versionId":"1.0",
                "name":"URL fixture",
                "files":[{{
                    "path":"mods/e4mc_minecraft-fabric-5.4.1.jar",
                    "hashes":{{"sha1":"0123456789abcdef","sha512":"fedcba9876543210"}},
                    "downloads":["{PRIMARY}","{FALLBACK}"],
                    "fileSize":42
                }}],
                "dependencies":{{"minecraft":"1.20.1","fabric-loader":"0.16.10"}}
            }}"#
        );
        let bytes = zip_fixture(&[("modrinth.index.json", manifest.as_bytes())]);
        let plan = inspect_reader(Cursor::new(bytes), "fallback").expect("pack should parse");

        let [ModpackFile::Direct { path, urls, .. }] = plan.files.as_slice() else {
            panic!("expected one direct Modrinth file");
        };
        assert_eq!(path, Path::new("mods/e4mc_minecraft-fabric-5.4.1.jar"));
        assert_eq!(urls, &[PRIMARY.to_owned(), FALLBACK.to_owned()]);
    }

    #[test]
    fn canonicalizes_url_control_characters_without_splitting_the_file_name() {
        let path = Path::new("mods/e4mc_minecraft-fabric-5.4.1.jar");
        let urls = normalize_modrinth_download_urls(
            vec!["https://cdn.modrinth.com/data/qANg5Jrr/versions/baNcxaPZ/e4mc_minecraft-\nfabric-5.4.1.jar".to_owned()],
            path,
        )
        .expect("WHATWG URL parsing should remove an embedded newline");

        assert_eq!(
            urls,
            vec![
                "https://cdn.modrinth.com/data/qANg5Jrr/versions/baNcxaPZ/e4mc_minecraft-fabric-5.4.1.jar"
            ]
        );
    }

    #[test]
    fn rejects_unsafe_modrinth_content_path() {
        let manifest = br#"{
            "formatVersion":1,
            "game":"minecraft",
            "versionId":"1.0",
            "name":"Unsafe mrpack",
            "files":[{
                "path":"mods/../../outside.jar",
                "hashes":{"sha1":"0123456789abcdef","sha512":"fedcba9876543210"},
                "downloads":["https://cdn.modrinth.com/outside.jar"],
                "fileSize":42
            }],
            "dependencies":{"minecraft":"1.20.1","fabric-loader":"0.16.10"}
        }"#;
        let bytes = zip_fixture(&[("modrinth.index.json", manifest)]);
        let error = inspect_reader(Cursor::new(bytes), "fallback")
            .expect_err("content path traversal must be rejected");

        assert!(error.contains("parent traversal"), "{error}");
    }

    #[test]
    fn inspects_nested_multimc_pack() {
        let manifest = br#"{
            "formatVersion":1,
            "components":[
                {"uid":"net.minecraft","version":"1.20.1"},
                {"uid":"net.neoforged","cachedVersion":"47.1.106"}
            ]
        }"#;
        let config = b"name=Prism Pack\nManagedPackVersionName=3.0\nauthor=Azulc\n";
        let bytes = zip_fixture(&[
            ("Prism Pack/mmc-pack.json", manifest),
            ("Prism Pack/instance.cfg", config),
        ]);
        let plan = inspect_reader(Cursor::new(bytes), "fallback").expect("pack should parse");

        assert_eq!(plan.format, ModpackFormat::MultiMc);
        assert_eq!(plan.metadata.name, "Prism Pack");
        assert_eq!(plan.metadata.loader.kind, LoaderKind::NeoForge);
        assert_eq!(plan.overrides_prefix, "Prism Pack/.minecraft");
    }

    #[test]
    fn rejects_archive_path_traversal() {
        let manifest = br#"{
            "name":"Unsafe Pack",
            "minecraft":{"version":"1.20.1","modLoaders":[]},
            "files":[],
            "overrides":"overrides"
        }"#;
        let bytes = zip_fixture(&[
            ("manifest.json", manifest),
            ("../outside.txt", b"must not escape"),
        ]);
        let error =
            inspect_reader(Cursor::new(bytes), "fallback").expect_err("traversal must be rejected");
        assert!(error.contains("parent traversal"), "{error}");
    }

    #[test]
    fn rejects_absolute_archive_path() {
        let manifest = br#"{
            "name":"Unsafe Pack",
            "minecraft":{"version":"1.20.1","modLoaders":[]},
            "files":[],
            "overrides":"overrides"
        }"#;
        let bytes = zip_fixture(&[
            ("manifest.json", manifest),
            ("/absolute.txt", b"must not escape"),
        ]);
        let error = inspect_reader(Cursor::new(bytes), "fallback")
            .expect_err("absolute path must be rejected");
        assert!(error.contains("absolute paths"), "{error}");
    }

    #[test]
    fn extracts_only_normalized_overrides_prefix() {
        let bytes = zip_fixture(&[
            ("overrides/config/settings.txt", b"inside"),
            ("not-overrides/ignored.txt", b"outside"),
        ]);
        let destination = std::env::temp_dir().join(format!(
            "azulc-modpack-test-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));

        apply_overrides_from_reader(Cursor::new(bytes), &destination, "/overrides/\\")
            .expect("overrides should extract");
        assert_eq!(
            std::fs::read(destination.join("config/settings.txt")).expect("override should exist"),
            b"inside"
        );
        assert!(!destination.join("ignored.txt").exists());
        std::fs::remove_dir_all(destination).expect("temporary fixture should be removed");
    }

    #[test]
    fn rejects_multiple_mod_loaders() {
        let manifest = br#"{
            "versionId":"bad",
            "name":"Ambiguous Pack",
            "files":[],
            "dependencies":{
                "minecraft":"1.20.1",
                "forge":"47.3.0",
                "fabric-loader":"0.16.10"
            }
        }"#;
        let bytes = zip_fixture(&[("modrinth.index.json", manifest)]);
        let error = inspect_reader(Cursor::new(bytes), "fallback")
            .expect_err("multiple loaders must be rejected");
        assert!(error.contains("2 mod loaders"), "{error}");
    }

    #[test]
    fn rejects_unknown_loader_instead_of_treating_it_as_vanilla() {
        let manifest = br#"{
            "name":"Unsupported Pack",
            "minecraft":{
                "version":"1.20.1",
                "modLoaders":[{"id":"quilt-0.27.1","primary":true}]
            },
            "files":[],
            "overrides":"overrides"
        }"#;
        let bytes = zip_fixture(&[("manifest.json", manifest)]);
        let error = inspect_reader(Cursor::new(bytes), "fallback")
            .expect_err("unknown loader must be rejected");
        assert!(
            error.contains("Unsupported mod loader: quilt-0.27.1"),
            "{error}"
        );
    }
}

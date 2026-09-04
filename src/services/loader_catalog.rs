use crate::domain::{DownloadSource, LoaderKind};
use serde::Deserialize;
use serde_json::Value;
use std::cmp::Ordering;

const BMCL_ROOT: &str = "https://bmclapi2.bangbang93.com";
const FABRIC_ROOT: &str = "https://meta.fabricmc.net";
const NEOFORGE_ROOT: &str = "https://maven.neoforged.net/api/maven/versions/releases";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoaderCatalogKey {
    pub minecraft_version: String,
    pub loader: LoaderKind,
    pub source: DownloadSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderVersionEntry {
    /// Compact version shown in the catalog.
    pub version: String,
    /// Exact coordinate handed to the installer. Forge branches are included.
    pub install_version: String,
    pub description: String,
    pub stable: bool,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderCatalog {
    pub entries: Vec<LoaderVersionEntry>,
    pub provider: &'static str,
}

impl LoaderCatalog {
    pub fn latest_stable_install_version(&self) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.stable)
            .map(|entry| entry.install_version.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct LoaderCatalogState {
    pub request_id: u64,
    pub key: Option<LoaderCatalogKey>,
    pub entries: Vec<LoaderVersionEntry>,
    pub provider: Option<&'static str>,
    pub loading: bool,
    pub error: Option<String>,
}

impl LoaderCatalogState {
    pub fn begin(&mut self, key: LoaderCatalogKey) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.key = Some(key);
        self.entries.clear();
        self.provider = None;
        self.loading = true;
        self.error = None;
        self.request_id
    }

    pub fn clear(&mut self) {
        let request_id = self.request_id.wrapping_add(1);
        *self = Self::default();
        self.request_id = request_id;
    }
}

pub async fn fetch(key: LoaderCatalogKey) -> Result<LoaderCatalog, String> {
    if key.loader == LoaderKind::Vanilla {
        return Ok(LoaderCatalog {
            entries: Vec::new(),
            provider: "LOCAL",
        });
    }

    let client = reqwest::Client::builder()
        .user_agent("AZULC/0.1.0")
        .build()
        .map_err(|error| error.to_string())?;

    match key.loader {
        LoaderKind::Vanilla => unreachable!(),
        LoaderKind::Forge => fetch_forge(&client, &key.minecraft_version).await,
        LoaderKind::Fabric => fetch_fabric(&client, &key).await,
        LoaderKind::NeoForge => fetch_neoforge(&client, &key).await,
    }
}

async fn fetch_forge(client: &reqwest::Client, minecraft: &str) -> Result<LoaderCatalog, String> {
    // Forge does not expose an equivalent official per-Minecraft-version
    // catalog. SJMCL uses this BMCLAPI endpoint for both source priorities.
    let url = format!("{BMCL_ROOT}/forge/minecraft/{minecraft}");
    let body = get(client, &url).await?;
    Ok(LoaderCatalog {
        entries: parse_forge(&body, minecraft)?,
        provider: "BMCLAPI / FORGE",
    })
}

async fn fetch_fabric(
    client: &reqwest::Client,
    key: &LoaderCatalogKey,
) -> Result<LoaderCatalog, String> {
    let official = format!("{FABRIC_ROOT}/v2/versions/loader/{}", key.minecraft_version);
    let bmcl = format!(
        "{BMCL_ROOT}/fabric-meta/v2/versions/loader/{}",
        key.minecraft_version
    );
    let candidates = match key.source {
        DownloadSource::Official => [
            (official.as_str(), "FABRIC META"),
            (bmcl.as_str(), "BMCLAPI / FABRIC"),
        ],
        DownloadSource::Bmcl => [
            (bmcl.as_str(), "BMCLAPI / FABRIC"),
            (official.as_str(), "FABRIC META"),
        ],
    };

    let mut errors = Vec::new();
    for (url, provider) in candidates {
        match get(client, url).await.and_then(|body| parse_fabric(&body)) {
            Ok(entries) => return Ok(LoaderCatalog { entries, provider }),
            Err(error) => errors.push(error),
        }
    }
    Err(format!(
        "Could not retrieve the Fabric loader catalog: {}",
        errors.join("; ")
    ))
}

async fn fetch_neoforge(
    client: &reqwest::Client,
    key: &LoaderCatalogKey,
) -> Result<LoaderCatalog, String> {
    let coordinate = if key.minecraft_version == "1.20.1" {
        "forge"
    } else {
        "neoforge"
    };
    let official = format!("{NEOFORGE_ROOT}/net/neoforged/{coordinate}/");
    let bmcl = format!("{BMCL_ROOT}/neoforge/list/{}", key.minecraft_version);
    let official_candidate = (official.as_str(), "NEOFORGE MAVEN", true);
    let bmcl_candidate = (bmcl.as_str(), "BMCLAPI / NEOFORGE", false);
    let candidates = match key.source {
        DownloadSource::Official => [official_candidate, bmcl_candidate],
        DownloadSource::Bmcl => [bmcl_candidate, official_candidate],
    };

    let mut errors = Vec::new();
    for (url, provider, is_official) in candidates {
        let result = get(client, url).await.and_then(|body| {
            if is_official {
                parse_neoforge_official(&body, &key.minecraft_version)
            } else {
                parse_neoforge_bmcl(&body, &key.minecraft_version)
            }
        });
        match result {
            Ok(entries) => return Ok(LoaderCatalog { entries, provider }),
            Err(error) => errors.push(error),
        }
    }
    Err(format!(
        "Could not retrieve the NeoForge loader catalog: {}",
        errors.join("; ")
    ))
}

async fn get(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("{url}: {error}"))?
        .error_for_status()
        .map_err(|error| format!("{url}: {error}"))?
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("{url}: {error}"))
}

#[derive(Debug, Deserialize)]
struct ForgeItem {
    #[serde(default)]
    branch: Option<Value>,
    build: i64,
    #[serde(default)]
    modified: String,
    version: String,
    #[serde(default)]
    files: Vec<ForgeFile>,
}

#[derive(Debug, Deserialize)]
struct ForgeFile {
    category: String,
    format: String,
}

fn parse_forge(body: &[u8], minecraft: &str) -> Result<Vec<LoaderVersionEntry>, String> {
    let mut items: Vec<ForgeItem> =
        serde_json::from_slice(body).map_err(|error| format!("Invalid Forge catalog: {error}"))?;
    items.sort_by(|a, b| b.build.cmp(&a.build));
    Ok(items
        .into_iter()
        .filter(|item| {
            item.files
                .iter()
                .any(|file| file.category == "installer" && file.format == "jar")
        })
        .map(|item| {
            let branch = item
                .branch
                .and_then(|value| value.as_str().map(str::to_owned))
                .filter(|value| !value.is_empty());
            let install_version =
                forge_install_version(minecraft, &item.version, branch.as_deref());
            LoaderVersionEntry {
                version: item.version,
                install_version,
                description: item.modified,
                stable: true,
                branch,
            }
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct FabricItem {
    loader: FabricLoader,
}

#[derive(Debug, Deserialize)]
struct FabricLoader {
    #[serde(default)]
    build: i64,
    version: String,
    stable: bool,
}

fn parse_fabric(body: &[u8]) -> Result<Vec<LoaderVersionEntry>, String> {
    let items: Vec<FabricItem> =
        serde_json::from_slice(body).map_err(|error| format!("Invalid Fabric catalog: {error}"))?;
    Ok(items
        .into_iter()
        .map(|item| LoaderVersionEntry {
            install_version: item.loader.version.clone(),
            version: item.loader.version,
            description: format!("build {}", item.loader.build),
            stable: item.loader.stable,
            branch: None,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct NeoForgeBmclItem {
    #[serde(rename = "rawVersion")]
    raw_version: String,
    version: String,
    mcversion: String,
}

fn parse_neoforge_bmcl(body: &[u8], minecraft: &str) -> Result<Vec<LoaderVersionEntry>, String> {
    let items: Vec<NeoForgeBmclItem> = serde_json::from_slice(body)
        .map_err(|error| format!("Invalid NeoForge BMCLAPI catalog: {error}"))?;
    Ok(neoforge_entries(items.into_iter().filter_map(|item| {
        if item.mcversion != minecraft {
            return None;
        }
        let install_version = if minecraft == "1.20.1" {
            if item.raw_version.starts_with("1.20.1-") {
                item.raw_version
            } else if item.version.starts_with("1.20.1-") {
                item.version.clone()
            } else {
                format!("1.20.1-{}", item.version)
            }
        } else {
            item.version.clone()
        };
        Some((item.version, install_version))
    })))
}

#[derive(Debug, Deserialize)]
struct NeoForgeOfficial {
    versions: Vec<String>,
}

fn parse_neoforge_official(
    body: &[u8],
    minecraft: &str,
) -> Result<Vec<LoaderVersionEntry>, String> {
    let response: NeoForgeOfficial = serde_json::from_slice(body)
        .map_err(|error| format!("Invalid NeoForge Maven catalog: {error}"))?;
    Ok(neoforge_entries(
        response
            .versions
            .into_iter()
            .filter(|version| neoforge_matches_minecraft(version, minecraft))
            .map(|version| (version.clone(), version)),
    ))
}

fn neoforge_entries(versions: impl Iterator<Item = (String, String)>) -> Vec<LoaderVersionEntry> {
    let mut entries: Vec<_> = versions
        .map(|(version, install_version)| {
            let lower = version.to_ascii_lowercase();
            let stable = !["alpha", "beta", "-rc", "+rc"]
                .iter()
                .any(|marker| lower.contains(marker));
            LoaderVersionEntry {
                install_version,
                version,
                description: String::new(),
                stable,
                branch: None,
            }
        })
        .collect();
    entries.sort_by(|a, b| natural_version_cmp(&b.version, &a.version));
    entries.dedup_by(|a, b| a.install_version == b.install_version);
    entries
}

pub(crate) fn neoforge_matches_minecraft(version: &str, minecraft: &str) -> bool {
    if minecraft == "1.20.1" {
        return version.starts_with("1.20.1-");
    }
    if let Some(april_fools) = version.strip_prefix("0.") {
        let Some((game, build)) = april_fools.split_once('.') else {
            return false;
        };
        return game == minecraft
            && build
                .strip_suffix("-beta")
                .unwrap_or(build)
                .parse::<u64>()
                .is_ok();
    }

    let Some(parsed) = parse_regular_neoforge(version) else {
        return false;
    };
    if minecraft.starts_with("1.") {
        return minecraft == format!("1.{}.{}", parsed.major, parsed.minor);
    }

    let mut derived = if parsed.patch == 0 {
        format!("{}.{}", parsed.major, parsed.minor)
    } else {
        format!("{}.{}.{}", parsed.major, parsed.minor, parsed.patch)
    };
    if let Some((kind, number)) = parsed.game_preview {
        derived.push('-');
        derived.push_str(kind);
        derived.push('-');
        derived.push_str(number);
    }
    minecraft == derived
}

struct ParsedNeoForge<'a> {
    major: u64,
    minor: u64,
    patch: u64,
    game_preview: Option<(&'a str, &'a str)>,
}

fn parse_regular_neoforge(version: &str) -> Option<ParsedNeoForge<'_>> {
    let (loader_part, game_preview) = match version.split_once('+') {
        Some((loader, preview)) => {
            let (kind, number) = preview.split_once('-')?;
            if !matches!(kind, "snapshot" | "pre" | "rc") || number.parse::<u64>().is_err() {
                return None;
            }
            (loader, Some((kind, number)))
        }
        None => (version, None),
    };
    let (numeric, loader_preview) = loader_part
        .split_once('-')
        .map_or((loader_part, None), |(numeric, preview)| {
            (numeric, Some(preview))
        });
    if let Some(preview) = loader_preview {
        let mut parts = preview.split('.');
        if !matches!(parts.next(), Some("alpha" | "beta" | "rc"))
            || parts
                .next()
                .is_some_and(|value| value.parse::<u64>().is_err())
            || parts.next().is_some()
        {
            return None;
        }
    }
    let numbers: Vec<u64> = numeric
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if !(3..=4).contains(&numbers.len()) {
        return None;
    }
    Some(ParsedNeoForge {
        major: numbers[0],
        minor: numbers[1],
        patch: numbers[2],
        game_preview,
    })
}

fn forge_install_version(minecraft: &str, version: &str, branch: Option<&str>) -> String {
    [Some(minecraft), Some(version), branch]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn natural_version_cmp(a: &str, b: &str) -> Ordering {
    numeric_parts(a)
        .cmp(&numeric_parts(b))
        .then_with(|| a.cmp(b))
}

fn numeric_parts(value: &str) -> Vec<u64> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap_or_default())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_stable_build_skips_newer_test_versions() {
        let catalog = LoaderCatalog {
            entries: vec![
                LoaderVersionEntry {
                    version: "0.17.0-beta".into(),
                    install_version: "0.17.0-beta".into(),
                    description: String::new(),
                    stable: false,
                    branch: None,
                },
                LoaderVersionEntry {
                    version: "0.16.14".into(),
                    install_version: "0.16.14".into(),
                    description: String::new(),
                    stable: true,
                    branch: None,
                },
            ],
            provider: "TEST",
        };

        assert_eq!(catalog.latest_stable_install_version(), Some("0.16.14"));
    }

    #[test]
    fn forge_catalog_sorts_builds_and_preserves_legacy_branch() {
        let body = br#"[
            {"build": 1151, "version": "10.13.0.1151", "modified": "old", "branch": null,
             "files":[{"category":"installer","format":"jar"}]},
            {"build": 1614, "version": "10.13.4.1614", "modified": "new", "branch": "1.7.10",
             "files":[{"category":"installer","format":"jar"}]}
        ]"#;
        let entries = parse_forge(body, "1.7.10").expect("valid Forge fixture");
        assert_eq!(entries[0].version, "10.13.4.1614");
        assert_eq!(entries[0].install_version, "1.7.10-10.13.4.1614-1.7.10");
        assert_eq!(entries[0].branch.as_deref(), Some("1.7.10"));
    }

    #[test]
    fn forge_catalog_hides_builds_without_an_installer_jar() {
        let body = br#"[
            {"build": 963, "version": "9.11.1.963", "files":[{"category":"userdev","format":"jar"}]},
            {"build": 965, "version": "9.11.1.965", "files":[{"category":"installer","format":"jar"}]}
        ]"#;
        let entries = parse_forge(body, "1.6.4").expect("valid Forge fixture");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "9.11.1.965");
    }

    #[test]
    fn fabric_catalog_preserves_meta_api_order_across_build_number_resets() {
        let body = br#"[
            {"loader":{"build":5,"version":"0.19.5","stable":true}},
            {"loader":{"build":214,"version":"0.10.6+build.214","stable":false}}
        ]"#;
        let entries = parse_fabric(body).expect("valid Fabric fixture");
        assert_eq!(entries[0].version, "0.19.5");
        assert!(entries[0].stable);
    }

    #[test]
    fn bmcl_neoforge_catalog_uses_numeric_descending_order() {
        let body = br#"[
            {"rawVersion":"neoforge-21.1.9","version":"21.1.9","mcversion":"1.21.1"},
            {"rawVersion":"neoforge-21.1.100","version":"21.1.100","mcversion":"1.21.1"},
            {"rawVersion":"neoforge-21.1.10-beta","version":"21.1.10-beta","mcversion":"1.21.1"}
        ]"#;
        let entries = parse_neoforge_bmcl(body, "1.21.1").expect("valid NeoForge fixture");
        assert_eq!(entries[0].version, "21.1.100");
        assert_eq!(entries[1].version, "21.1.10-beta");
        assert!(!entries[1].stable);
    }

    #[test]
    fn bmcl_neoforge_1_20_1_uses_raw_maven_coordinate() {
        let body = br#"[
            {"rawVersion":"1.20.1-47.1.82","version":"47.1.82","mcversion":"1.20.1"}
        ]"#;
        let entries = parse_neoforge_bmcl(body, "1.20.1").expect("valid NeoForge fixture");
        assert_eq!(entries[0].version, "47.1.82");
        assert_eq!(entries[0].install_version, "1.20.1-47.1.82");
    }

    #[test]
    fn official_neoforge_catalog_is_filtered_by_minecraft_version() {
        let body = br#"{
            "isSnapshot": false,
            "versions": ["20.6.120", "21.1.7", "21.1.9-beta", "21.2.1"]
        }"#;
        let entries = parse_neoforge_official(body, "1.21.1").expect("valid NeoForge fixture");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].version, "21.1.9-beta");
        assert!(!entries[0].stable);
    }

    #[test]
    fn official_neoforge_legacy_catalog_keeps_only_full_1_20_1_coordinates() {
        let body = br#"{
            "isSnapshot": false,
            "versions": ["47.1.82", "1.20.1-47.1.82", "1.20.1-47.1.83-beta"]
        }"#;
        let entries = parse_neoforge_official(body, "1.20.1").expect("valid NeoForge fixture");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].install_version, "1.20.1-47.1.83-beta");
        assert_eq!(entries[1].install_version, "1.20.1-47.1.82");
    }

    #[test]
    fn official_neoforge_supports_april_fools_versions() {
        let body = br#"{
            "versions": ["0.25w14craftmine.3-beta", "0.25w14craftmine.5", "21.1.1"]
        }"#;
        let entries =
            parse_neoforge_official(body, "25w14craftmine").expect("valid April Fools fixture");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].version, "0.25w14craftmine.5");
    }

    #[test]
    fn official_neoforge_matches_post_1_x_versions_exactly() {
        let body = br#"{
            "versions": [
                "26.1.0.8", "26.1.0.9+pre-1", "26.1.1.3", "26.1.2.4",
                "26.1.1.5+rc-2"
            ]
        }"#;
        let release = parse_neoforge_official(body, "26.1").expect("valid release fixture");
        assert_eq!(release.len(), 1);
        assert_eq!(release[0].version, "26.1.0.8");

        let patch = parse_neoforge_official(body, "26.1.1").expect("valid patch fixture");
        assert_eq!(patch.len(), 1);
        assert_eq!(patch[0].version, "26.1.1.3");

        let preview = parse_neoforge_official(body, "26.1-pre-1").expect("valid preview fixture");
        assert_eq!(preview.len(), 1);
        assert_eq!(preview[0].version, "26.1.0.9+pre-1");
    }

    #[test]
    fn catalog_state_invalidates_same_key_requests_and_clears() {
        let key = LoaderCatalogKey {
            minecraft_version: "1.21.1".into(),
            loader: LoaderKind::Fabric,
            source: DownloadSource::Bmcl,
        };
        let mut state = LoaderCatalogState::default();
        let first = state.begin(key.clone());
        let retry = state.begin(key);
        assert_ne!(first, retry);
        state.clear();
        assert_ne!(state.request_id, retry);
    }
}

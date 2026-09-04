//! Routing between official artifact endpoints and configured mirrors.

use crate::domain::{DownloadPolicy, DownloadSource};

const BMCL_API_ROOT: &str = "https://bmclapi2.bangbang93.com/";
const BMCL_MAVEN_ROOT: &str = "https://bmclapi2.bangbang93.com/maven/";

/// Pure URL routing for download requests.
///
/// The router deliberately leaves unknown URLs untouched. This is important for
/// loader metadata that points at third-party Maven repositories which BMCLAPI2
/// does not mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRouter {
    source: DownloadSource,
}

impl SourceRouter {
    pub const fn new(source: DownloadSource) -> Self {
        Self { source }
    }

    pub const fn from_policy(policy: &DownloadPolicy) -> Self {
        Self::new(policy.source)
    }

    #[cfg(test)]
    pub const fn source(self) -> DownloadSource {
        self.source
    }

    pub fn rewrite(self, url: &str) -> String {
        rewrite_url(self.source, url)
    }
}

impl From<DownloadSource> for SourceRouter {
    fn from(source: DownloadSource) -> Self {
        Self::new(source)
    }
}

impl From<&DownloadPolicy> for SourceRouter {
    fn from(policy: &DownloadPolicy) -> Self {
        Self::from_policy(policy)
    }
}

/// Rewrites a known official Minecraft ecosystem URL for the selected source.
/// Official and unsupported URLs are returned byte-for-byte unchanged.
pub fn rewrite_url(source: DownloadSource, url: &str) -> String {
    if source == DownloadSource::Official {
        return url.to_owned();
    }

    if let Some(url) = rewrite_neoforge_installer(url) {
        return url;
    }

    // Keep more specific prefixes before their general Maven hosts.
    const BMCL_RULES: &[(&str, &str)] = &[
        // Mojang manifests and per-version metadata/client objects.
        ("https://launchermeta.mojang.com/", BMCL_API_ROOT),
        ("https://piston-meta.mojang.com/", BMCL_API_ROOT),
        ("https://launcher.mojang.com/", BMCL_API_ROOT),
        ("https://piston-data.mojang.com/", BMCL_API_ROOT),
        // Vanilla libraries and hashed assets.
        ("https://libraries.minecraft.net/", BMCL_MAVEN_ROOT),
        (
            "https://resources.download.minecraft.net/",
            "https://bmclapi2.bangbang93.com/assets/",
        ),
        // Forge metadata and Maven repositories.
        (
            "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
            "https://bmclapi2.bangbang93.com/forge/promotions_slim.json",
        ),
        ("https://files.minecraftforge.net/maven/", BMCL_MAVEN_ROOT),
        ("https://maven.minecraftforge.net/", BMCL_MAVEN_ROOT),
        // Fabric loader metadata and artifacts.
        (
            "https://meta.fabricmc.net/",
            "https://bmclapi2.bangbang93.com/fabric-meta/",
        ),
        ("https://maven.fabricmc.net/", BMCL_MAVEN_ROOT),
        // NeoForge version APIs. SJMCL routes both coordinates to this list.
        (
            "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge/",
            "https://bmclapi2.bangbang93.com/neoforge/",
        ),
        (
            "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge/",
            "https://bmclapi2.bangbang93.com/neoforge/",
        ),
        ("https://maven.neoforged.net/releases/", BMCL_MAVEN_ROOT),
    ];

    BMCL_RULES
        .iter()
        .find_map(|(official, mirror)| {
            url.strip_prefix(official)
                .map(|suffix| format!("{mirror}{suffix}"))
        })
        .unwrap_or_else(|| url.to_owned())
}

/// BMCLAPI2 exposes modern NeoForge installers through a version endpoint
/// rather than its generic Maven mirror. Preserve a query/fragment if one is
/// present, although installer URLs normally contain neither.
fn rewrite_neoforge_installer(url: &str) -> Option<String> {
    const PREFIX: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge/";

    let path = url.strip_prefix(PREFIX)?;
    let (version, rest) = path.split_once('/')?;
    let tail_at = rest.find(['?', '#']).unwrap_or(rest.len());
    let (file_name, tail) = rest.split_at(tail_at);
    if file_name != format!("neoforge-{version}-installer.jar") {
        return None;
    }

    Some(format!(
        "https://bmclapi2.bangbang93.com/neoforge/version/{version}/download/installer{tail}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmcl(url: &str) -> String {
        rewrite_url(DownloadSource::Bmcl, url)
    }

    #[test]
    fn official_source_never_rewrites() {
        let url = "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar";
        assert_eq!(rewrite_url(DownloadSource::Official, url), url);
    }

    #[test]
    fn rewrites_mojang_manifests_and_version_metadata() {
        assert_eq!(
            bmcl("https://launchermeta.mojang.com/mc/game/version_manifest_v2.json"),
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
        );
        assert_eq!(
            bmcl("https://piston-meta.mojang.com/v1/packages/abc/1.21.1.json"),
            "https://bmclapi2.bangbang93.com/v1/packages/abc/1.21.1.json"
        );
    }

    #[test]
    fn rewrites_vanilla_libraries_and_assets() {
        assert_eq!(
            bmcl("https://libraries.minecraft.net/com/mojang/authlib/6.0.54/authlib-6.0.54.jar"),
            "https://bmclapi2.bangbang93.com/maven/com/mojang/authlib/6.0.54/authlib-6.0.54.jar"
        );
        assert_eq!(
            bmcl("https://resources.download.minecraft.net/ab/abcdef"),
            "https://bmclapi2.bangbang93.com/assets/ab/abcdef"
        );
    }

    #[test]
    fn rewrites_forge_metadata_and_artifacts() {
        assert_eq!(
            bmcl("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json"),
            "https://bmclapi2.bangbang93.com/forge/promotions_slim.json"
        );
        assert_eq!(
            bmcl(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.10/forge-1.20.1-47.4.10-installer.jar"
            ),
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/1.20.1-47.4.10/forge-1.20.1-47.4.10-installer.jar"
        );
        assert_eq!(
            bmcl(
                "https://files.minecraftforge.net/maven/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-universal.jar"
            ),
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-universal.jar"
        );
    }

    #[test]
    fn rewrites_fabric_meta_and_maven() {
        assert_eq!(
            bmcl("https://meta.fabricmc.net/v2/versions/loader/1.21.1"),
            "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/1.21.1"
        );
        assert_eq!(
            bmcl(
                "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar"
            ),
            "https://bmclapi2.bangbang93.com/maven/net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar"
        );
    }

    #[test]
    fn rewrites_neoforge_meta_maven_and_installer() {
        assert_eq!(
            bmcl("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge/"),
            "https://bmclapi2.bangbang93.com/neoforge/"
        );
        assert_eq!(
            bmcl(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.172/neoforge-21.1.172-universal.jar"
            ),
            "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/21.1.172/neoforge-21.1.172-universal.jar"
        );
        assert_eq!(
            bmcl(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.172/neoforge-21.1.172-installer.jar"
            ),
            "https://bmclapi2.bangbang93.com/neoforge/version/21.1.172/download/installer"
        );
    }

    #[test]
    fn router_uses_policy_and_keeps_unknown_hosts() {
        let policy = DownloadPolicy {
            source: DownloadSource::Bmcl,
            concurrency: 32,
        };
        let router = SourceRouter::from_policy(&policy);
        assert_eq!(router.source(), DownloadSource::Bmcl);
        assert_eq!(
            router.rewrite("https://repo.example.org/example.jar"),
            "https://repo.example.org/example.jar"
        );
    }
}

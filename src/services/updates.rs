//! GitHub release discovery and exact platform asset selection.

use serde::Deserialize;
use std::time::Duration;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub body: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

pub fn version(tag: &str) -> Result<semver::Version, semver::Error> {
    semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag))
}

impl Release {
    pub fn is_newer_than(&self, current: &str) -> bool {
        !self.draft
            && !self.prerelease
            && matches!((version(&self.tag_name), version(current)), (Ok(latest), Ok(current)) if latest.pre.is_empty() && latest.cmp_precedence(&current).is_gt())
    }

    pub fn download_url(&self, os: &str, arch: &str) -> Option<&str> {
        let arch = match arch {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            _ => return None,
        };
        let extension = match os {
            "windows" | "macos" => "zip",
            "linux" => "tar.gz",
            _ => return None,
        };
        let version = version(&self.tag_name).ok()?;
        let name = format!("AZULC-{version}-{os}-{arch}.{extension}");
        self.assets
            .iter()
            .find(|asset| {
                asset.name == name
                    && asset
                        .browser_download_url
                        .starts_with("https://github.com/Reqwey/AZULC/releases/download/")
            })
            .map(|asset| asset.browser_download_url.as_str())
    }

    pub fn notes(&self) -> crate::domain::ReleaseNotes {
        crate::domain::ReleaseNotes {
            version: self.tag_name.clone(),
            body: self
                .body
                .clone()
                .filter(|body| !body.trim().is_empty())
                .unwrap_or_else(|| "No release notes were provided for this version.".into()),
        }
    }
}

pub async fn fetch(tag: Option<&str>) -> Result<Release, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("AZULC/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let endpoint = match tag {
        Some(tag) => format!("https://api.github.com/repos/Reqwey/AZULC/releases/tags/v{tag}"),
        None => "https://api.github.com/repos/Reqwey/AZULC/releases/latest".into(),
    };
    client
        .get(endpoint)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.into(),
            body: None,
            draft: false,
            prerelease: false,
            assets: vec![],
        }
    }

    #[test]
    fn compares_versions_numerically_and_rejects_non_stable_releases() {
        assert!(release("v0.10.0").is_newer_than("0.9.0"));
        for tag in [
            "v0.9.0",
            "v0.8.0",
            "invalid",
            "v1.0.0-beta.1",
            "v0.9.0+build.2",
        ] {
            assert!(!release(tag).is_newer_than("0.9.0"));
        }
        let mut draft = release("v1.0.0");
        draft.draft = true;
        assert!(!draft.is_newer_than("0.9.0"));
    }

    #[test]
    fn selects_exact_os_and_architecture_without_fallback() {
        let mut release = release("v1.0.0");
        for platform in [
            "windows-x64.zip",
            "macos-x64.zip",
            "macos-arm64.zip",
            "linux-x64.tar.gz",
        ] {
            let name = format!("AZULC-1.0.0-{platform}");
            release.assets.push(Asset {
                browser_download_url: format!(
                    "https://github.com/Reqwey/AZULC/releases/download/v1.0.0/{name}"
                ),
                name,
            });
        }
        for (os, arch, suffix) in [
            ("windows", "x86_64", "windows-x64.zip"),
            ("macos", "aarch64", "macos-arm64.zip"),
            ("macos", "x86_64", "macos-x64.zip"),
            ("linux", "x86_64", "linux-x64.tar.gz"),
        ] {
            assert!(release.download_url(os, arch).unwrap().ends_with(suffix));
        }
        assert!(release.download_url("linux", "aarch64").is_none());
    }
}

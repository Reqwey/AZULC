//! Provider-neutral values returned by CurseForge and Modrinth catalogs.

use crate::services::providers::{
    curseforge::{File as CurseForgeFile, Project as CurseForgeProject, ResourceClass},
    modrinth::{
        Project as ModrinthProject, Version as ModrinthVersion,
        VersionStatus as ModrinthVersionStatus, VersionType as ModrinthVersionType,
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum CatalogProvider {
    #[default]
    CurseForge,
    Modrinth,
}

impl CatalogProvider {
    pub(crate) const ALL: [Self; 2] = [Self::CurseForge, Self::Modrinth];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::CurseForge => "CurseForge",
            Self::Modrinth => "Modrinth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CatalogProjectKey {
    CurseForge(u64),
    Modrinth(String),
}

impl CatalogProjectKey {
    pub(crate) fn provider(&self) -> CatalogProvider {
        match self {
            Self::CurseForge(_) => CatalogProvider::CurseForge,
            Self::Modrinth(_) => CatalogProvider::Modrinth,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CatalogProject {
    pub(crate) key: CatalogProjectKey,
    pub(crate) name: String,
    pub(crate) summary: String,
    pub(crate) author: String,
    pub(crate) categories: Vec<String>,
    pub(crate) download_count: u64,
    pub(crate) date_modified: String,
    pub(crate) available: bool,
    pub(crate) icon_url: Option<String>,
}

impl CatalogProject {
    pub(super) fn from_curseforge(project: CurseForgeProject) -> Self {
        let available = curseforge_project_available(
            project.is_available,
            project.resource_class(),
            project.allow_mod_distribution,
        );
        let icon_url = project.logo.and_then(|logo| {
            [logo.thumbnail_url, logo.url]
                .into_iter()
                .find(|value| !value.trim().is_empty())
        });
        Self {
            key: CatalogProjectKey::CurseForge(project.id),
            name: project.name,
            summary: project.summary,
            author: project
                .authors
                .first()
                .map_or_else(|| "unknown author".into(), |author| author.name.clone()),
            categories: project
                .categories
                .into_iter()
                .map(|category| category.name)
                .collect(),
            download_count: project.download_count,
            date_modified: project.date_modified,
            available,
            icon_url,
        }
    }

    pub(super) fn from_modrinth(project: ModrinthProject) -> Self {
        Self {
            key: CatalogProjectKey::Modrinth(project.id),
            name: project.title,
            summary: project.description,
            author: project.author.unwrap_or_else(|| "unknown author".into()),
            categories: project.categories,
            download_count: project.downloads,
            date_modified: project.updated,
            available: true,
            icon_url: project.icon_url,
        }
    }
}

fn curseforge_project_available(
    is_available: bool,
    class: Option<ResourceClass>,
    allows_distribution: Option<bool>,
) -> bool {
    is_available && (class == Some(ResourceClass::Modpack) || allows_distribution != Some(false))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CatalogReleaseKey {
    CurseForge {
        project_id: u64,
        file_id: u64,
    },
    Modrinth {
        project_id: String,
        version_id: String,
    },
}

impl CatalogReleaseKey {
    pub(crate) fn provider(&self) -> CatalogProvider {
        match self {
            Self::CurseForge { .. } => CatalogProvider::CurseForge,
            Self::Modrinth { .. } => CatalogProvider::Modrinth,
        }
    }

    pub(crate) fn belongs_to(&self, project: &CatalogProjectKey) -> bool {
        match (project, self) {
            (
                CatalogProjectKey::CurseForge(project_id),
                Self::CurseForge {
                    project_id: release_project_id,
                    ..
                },
            ) => project_id == release_project_id,
            (
                CatalogProjectKey::Modrinth(project_id),
                Self::Modrinth {
                    project_id: release_project_id,
                    ..
                },
            ) => project_id == release_project_id,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CatalogRelease {
    pub(crate) key: CatalogReleaseKey,
    pub(crate) display_name: String,
    pub(crate) file_name: String,
    pub(crate) release_type: u8,
    pub(crate) file_date: String,
    pub(crate) file_length: u64,
    pub(crate) download_count: u64,
    pub(crate) game_versions: Vec<String>,
    pub(crate) available: bool,
}

impl CatalogRelease {
    pub(crate) fn belongs_to(&self, project: &CatalogProject) -> bool {
        self.key.provider() == project.key.provider() && self.key.belongs_to(&project.key)
    }

    pub(super) fn from_curseforge(file: CurseForgeFile) -> Self {
        Self {
            key: CatalogReleaseKey::CurseForge {
                project_id: file.mod_id,
                file_id: file.id,
            },
            display_name: file.display_name,
            file_name: file.file_name,
            release_type: file.release_type,
            file_date: file.file_date,
            file_length: file.file_length,
            download_count: file.download_count,
            game_versions: file.game_versions,
            available: file.is_available && file.is_server_pack != Some(true),
        }
    }

    pub(super) fn from_modrinth(version: ModrinthVersion) -> Result<Self, String> {
        let install = version.install_plan().map_err(|error| error.to_string())?;
        let release_type = match version.version_type {
            ModrinthVersionType::Release => 1,
            ModrinthVersionType::Beta => 2,
            ModrinthVersionType::Alpha => 3,
            ModrinthVersionType::Unknown => 0,
        };
        let available = matches!(
            version.status,
            ModrinthVersionStatus::Listed
                | ModrinthVersionStatus::Archived
                | ModrinthVersionStatus::Unlisted
        );
        Ok(Self {
            key: CatalogReleaseKey::Modrinth {
                project_id: version.project_id.clone(),
                version_id: version.id,
            },
            display_name: version.name,
            file_name: install.file_name,
            release_type,
            file_date: version.date_published,
            file_length: install.size,
            download_count: version.downloads,
            game_versions: version.game_versions,
            available,
        })
    }

    pub(super) fn from_modrinth_versions(versions: Vec<ModrinthVersion>) -> Vec<Self> {
        versions
            .into_iter()
            .filter_map(|version| Self::from_modrinth(version).ok())
            .collect()
    }
}

pub(crate) fn thumbnail_urls(projects: &[CatalogProject]) -> Vec<String> {
    projects
        .iter()
        .filter_map(|project| project.icon_url.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::providers::modrinth::{FileHashes, VersionFile};

    #[test]
    fn project_keys_report_their_provider() {
        assert_eq!(
            CatalogProjectKey::Modrinth("project".into()).provider(),
            CatalogProvider::Modrinth
        );
    }

    #[test]
    fn release_keys_report_their_provider() {
        assert_eq!(
            CatalogReleaseKey::CurseForge {
                project_id: 1,
                file_id: 2,
            }
            .provider(),
            CatalogProvider::CurseForge
        );
    }

    #[test]
    fn releases_belong_only_to_their_declared_project() {
        let release = CatalogReleaseKey::Modrinth {
            project_id: "expected".into(),
            version_id: "version".into(),
        };

        assert!(release.belongs_to(&CatalogProjectKey::Modrinth("expected".into())));
        assert!(!release.belongs_to(&CatalogProjectKey::Modrinth("other".into())));
        assert!(!release.belongs_to(&CatalogProjectKey::CurseForge(1)));
    }

    #[test]
    fn restricted_curseforge_distribution_keeps_modpack_fallback_available() {
        assert!(curseforge_project_available(
            true,
            Some(ResourceClass::Modpack),
            Some(false)
        ));
        assert!(!curseforge_project_available(
            true,
            Some(ResourceClass::Mod),
            Some(false)
        ));
        assert!(!curseforge_project_available(
            false,
            Some(ResourceClass::Modpack),
            Some(false)
        ));
    }

    #[test]
    fn malformed_modrinth_release_does_not_hide_valid_releases() {
        let version = |id: &str, files: Vec<VersionFile>| ModrinthVersion {
            id: id.into(),
            project_id: "project".into(),
            author_id: "author".into(),
            name: id.into(),
            version_number: id.into(),
            changelog: None,
            dependencies: Vec::new(),
            game_versions: vec!["1.21.1".into()],
            version_type: ModrinthVersionType::Release,
            loaders: vec!["fabric".into()],
            featured: false,
            status: ModrinthVersionStatus::Listed,
            date_published: "2026-01-01T00:00:00Z".into(),
            downloads: 1,
            environment: None,
            files,
        };
        let valid_file = VersionFile {
            id: None,
            hashes: FileHashes {
                sha1: Some("a9993e364706816aba3e25717850c26c9cd0d89d".into()),
                sha512: None,
            },
            url: "https://cdn.modrinth.com/data/project/versions/valid/file.jar".into(),
            filename: "file.jar".into(),
            primary: true,
            size: 3,
            file_type: None,
        };

        let releases = CatalogRelease::from_modrinth_versions(vec![
            version("invalid", Vec::new()),
            version("valid", vec![valid_file]),
        ]);

        assert_eq!(releases.len(), 1);
        assert!(matches!(
            &releases[0].key,
            CatalogReleaseKey::Modrinth { version_id, .. } if version_id == "valid"
        ));
    }
}

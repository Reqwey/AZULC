//! Route, tab, and wizard state shared by the application and views.

use crate::{
    domain::InstanceColor,
    services::{content::ContentKind, minecraft},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum Route {
    #[default]
    Home,
    Instance {
        id: Uuid,
        tab: InstanceTab,
    },
    Installation(Uuid),
    NewInstance,
    Accounts,
    Settings,
}

impl Route {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Instance { .. } => "Instance",
            Self::Installation(_) => "Installation",
            Self::NewInstance => "New Instance",
            Self::Accounts => "Accounts",
            Self::Settings => "App Settings",
        }
    }

    pub(crate) const fn instance(id: Uuid) -> Self {
        Self::Instance {
            id,
            tab: InstanceTab::Overview,
        }
    }

    pub(crate) const fn installed_instance(self) -> Option<(Uuid, InstanceTab)> {
        match self {
            Self::Instance { id, tab } => Some((id, tab)),
            _ => None,
        }
    }

    pub(crate) const fn target_id(self) -> Option<Uuid> {
        match self {
            Self::Instance { id, .. } | Self::Installation(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn after_instance_deleted(self, deleted_id: Uuid) -> Self {
        match self {
            Self::Instance { id, .. } if id == deleted_id => Self::Home,
            _ => self,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum NewInstanceTab {
    #[default]
    Minecraft,
    Modpacks,
}

impl NewInstanceTab {
    pub(crate) const ALL: [Self; 2] = [Self::Minecraft, Self::Modpacks];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Minecraft => "Minecraft",
            Self::Modpacks => "Modpacks",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum ModpackTab {
    #[default]
    Browse,
    Import,
}

impl ModpackTab {
    pub(crate) const ALL: [Self; 2] = [Self::Browse, Self::Import];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Browse => "Browse Online",
            Self::Import => "Import File",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum InstanceTab {
    #[default]
    Overview,
    Worlds,
    Mods,
    ResourcePacks,
    ShaderPacks,
    DataPacks,
    Screenshots,
    Settings,
}

impl InstanceTab {
    pub(crate) const ALL: [Self; 8] = [
        Self::Overview,
        Self::Worlds,
        Self::Mods,
        Self::ResourcePacks,
        Self::ShaderPacks,
        Self::DataPacks,
        Self::Screenshots,
        Self::Settings,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Worlds => "Worlds",
            Self::Mods => "Mods",
            Self::ResourcePacks => "Resource Packs",
            Self::ShaderPacks => "Shaders",
            Self::DataPacks => "Data Packs",
            Self::Screenshots => "Screenshots",
            Self::Settings => "Settings",
        }
    }

    pub(crate) fn content_kind(self) -> Option<ContentKind> {
        match self {
            Self::Worlds => Some(ContentKind::Worlds),
            Self::Mods => Some(ContentKind::Mods),
            Self::ResourcePacks => Some(ContentKind::ResourcePacks),
            Self::ShaderPacks => Some(ContentKind::ShaderPacks),
            Self::DataPacks => Some(ContentKind::DataPacks),
            Self::Screenshots => Some(ContentKind::Screenshots),
            Self::Overview | Self::Settings => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum SettingsTab {
    #[default]
    Downloads,
    Java,
    About,
}

impl SettingsTab {
    pub(crate) const ALL: [Self; 3] = [Self::Downloads, Self::Java, Self::About];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Downloads => "Downloads",
            Self::Java => "Java",
            Self::About => "About",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum WizardStep {
    #[default]
    Version,
    Loader,
    Details,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum VersionFilter {
    #[default]
    Release,
    Snapshot,
    Old,
    AprilFools,
}

impl VersionFilter {
    pub(crate) const ALL: [Self; 4] = [Self::Release, Self::Snapshot, Self::Old, Self::AprilFools];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Release => "Releases",
            Self::Snapshot => "Snapshots",
            Self::Old => "Legacy",
            Self::AprilFools => "April Fools",
        }
    }

    pub(crate) const fn color(self) -> InstanceColor {
        match self {
            Self::Release => InstanceColor::Lavender,
            Self::Snapshot => InstanceColor::Sky,
            Self::Old => InstanceColor::Amber,
            Self::AprilFools => InstanceColor::Rose,
        }
    }

    pub(crate) fn for_version(version: &minecraft::VersionEntry) -> Self {
        if version.release_time.contains("04-01") {
            Self::AprilFools
        } else {
            match version.kind.as_str() {
                "release" => Self::Release,
                "snapshot" => Self::Snapshot,
                _ => Self::Old,
            }
        }
    }

    pub(crate) fn matches(self, version: &minecraft::VersionEntry) -> bool {
        Self::for_version(version) == self
    }
}

#[cfg(test)]
mod tests {
    use super::{InstanceTab, Route, VersionFilter};
    use crate::{domain::InstanceColor, services::minecraft};
    use uuid::Uuid;

    fn version(kind: &str, release_time: &str) -> minecraft::VersionEntry {
        minecraft::VersionEntry {
            id: "test".into(),
            kind: kind.into(),
            release_time: release_time.into(),
            url: "https://example.invalid/version.json".into(),
            sha1: String::new(),
        }
    }

    #[test]
    fn version_channels_have_stable_instance_colors() {
        assert_eq!(
            VersionFilter::for_version(&version("release", "2026-09-04T00:00:00Z")).color(),
            InstanceColor::Lavender
        );
        assert_eq!(
            VersionFilter::for_version(&version("snapshot", "2026-09-04T00:00:00Z")).color(),
            InstanceColor::Sky
        );
        assert_eq!(
            VersionFilter::for_version(&version("old_beta", "2011-01-01T00:00:00Z")).color(),
            InstanceColor::Amber
        );
        assert_eq!(
            VersionFilter::for_version(&version("snapshot", "2026-04-01T00:00:00Z")).color(),
            InstanceColor::Rose
        );
    }

    #[test]
    fn instance_routes_carry_their_own_identity_and_tab() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);

        let route = Route::Instance {
            id: first,
            tab: InstanceTab::Mods,
        };

        assert_eq!(route.installed_instance(), Some((first, InstanceTab::Mods)));
        assert_eq!(route.target_id(), Some(first));
        assert_ne!(route, Route::instance(second));
        assert_eq!(
            Route::instance(first).installed_instance(),
            Some((first, InstanceTab::Overview))
        );
    }

    #[test]
    fn installation_routes_are_distinct_from_installed_instances() {
        let id = Uuid::from_u128(7);

        assert_eq!(Route::Installation(id).target_id(), Some(id));
        assert_eq!(Route::Installation(id).installed_instance(), None);
        assert_ne!(Route::Installation(id), Route::instance(id));
    }

    #[test]
    fn deleting_the_routed_instance_goes_home_without_moving_background_routes() {
        let current = Uuid::from_u128(11);
        let background = Uuid::from_u128(12);
        let route = Route::Instance {
            id: current,
            tab: InstanceTab::Settings,
        };

        assert_eq!(route.after_instance_deleted(current), Route::Home);
        assert_eq!(route.after_instance_deleted(background), route);
        assert_eq!(
            Route::Accounts.after_instance_deleted(current),
            Route::Accounts
        );
    }
}

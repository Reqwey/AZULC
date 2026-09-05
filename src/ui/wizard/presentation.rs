use crate::{domain::LoaderKind, services::minecraft::VersionEntry};

pub(super) fn version_badge(entry: &VersionEntry) -> &'static str {
    if entry.release_time.contains("04-01") {
        "APRIL"
    } else {
        match entry.kind.as_str() {
            "release" => "RELEASE",
            "snapshot" => "SNAPSHOT",
            _ => "LEGACY",
        }
    }
}

pub(super) fn loader_copy(loader: LoaderKind) -> &'static str {
    match loader {
        LoaderKind::Vanilla => "Pure Minecraft / no loader",
        LoaderKind::Fabric => "Lightweight and fast modding",
        LoaderKind::Forge => "Classic mod ecosystem",
        LoaderKind::NeoForge => "Modern Forge-family platform",
    }
}

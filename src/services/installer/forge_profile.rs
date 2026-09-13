//! Reads modern Forge installer data without invoking the installer entry point.

use super::{InstallError, read_installer_json, safe_profile_id, zip_io_error};
use crate::services::{download::file_ops, minecraft, path_safety};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs::File,
    path::{Component, Path, PathBuf},
};

#[derive(Deserialize)]
pub(super) struct Profile {
    pub spec: u32,
    pub minecraft: String,
    #[serde(default)]
    pub data: HashMap<String, Data>,
    #[serde(default)]
    pub processors: Vec<Processor>,
    #[serde(default)]
    pub libraries: Vec<minecraft::Library>,
    #[serde(default = "version_entry")]
    pub json: String,
}

fn version_entry() -> String {
    "/version.json".into()
}

#[derive(Deserialize)]
pub(super) struct Data {
    pub client: String,
}

#[derive(Deserialize)]
pub(super) struct Processor {
    pub jar: String,
    pub sides: Option<Vec<String>>,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub outputs: Option<HashMap<String, String>>,
}

impl Processor {
    pub fn is_client(&self) -> bool {
        self.sides
            .as_ref()
            .is_none_or(|sides| sides.iter().any(|side| side == "client"))
    }
}

pub(super) struct PreparedProfile {
    pub profile: Profile,
    pub version: minecraft::VersionJson,
    pub version_json: Vec<u8>,
    pub variables: HashMap<String, String>,
}

pub(super) fn invalid(message: impl Into<String>) -> InstallError {
    InstallError::ForgeProfile(message.into())
}

pub(super) fn library_path(root: &Path, coordinate: &str) -> Result<PathBuf, InstallError> {
    let relative = minecraft::maven_path(coordinate)
        .and_then(|path| path_safety::relative_path(&path))
        .ok_or_else(|| invalid(format!("unsafe Maven coordinate {coordinate:?}")))?;
    Ok(root.join("libraries").join(relative))
}

/// Paths written by the launcher must remain below its shared Minecraft root.
pub(super) fn output_path(root: &Path, value: &str) -> Result<PathBuf, InstallError> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || !path.starts_with(root)
        || path == root
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid(format!(
            "processor output escapes Minecraft root: {value:?}"
        )));
    }
    Ok(path)
}

/// Replace tokens once, so braces in a user's directory name stay literal.
pub(super) fn expand(
    value: &str,
    variables: &HashMap<String, String>,
    root: &Path,
) -> Result<String, InstallError> {
    if let Some(coordinate) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        return Ok(library_path(root, coordinate)?
            .to_string_lossy()
            .into_owned());
    }
    let mut result = String::new();
    let mut remaining = value;
    while let Some(start) = remaining.find('{') {
        result.push_str(&remaining[..start]);
        let rest = &remaining[start + 1..];
        let end = rest
            .find('}')
            .ok_or_else(|| invalid(format!("unclosed token in {value:?}")))?;
        let key = &rest[..end];
        result.push_str(
            variables
                .get(key)
                .ok_or_else(|| invalid(format!("unknown token {{{key}}}")))?,
        );
        remaining = &rest[end + 1..];
    }
    result.push_str(remaining);
    Ok(result)
}

pub(super) fn prepare(
    installer: &Path,
    root: &Path,
    minecraft_version: &str,
    work: &Path,
) -> Result<PreparedProfile, InstallError> {
    let mut archive = zip::ZipArchive::new(File::open(installer)?).map_err(zip_io_error)?;
    let profile: Profile = read_installer_json(&mut archive, "install_profile.json")?;
    if profile.spec > 1 || profile.minecraft != minecraft_version {
        return Err(invalid(format!(
            "unsupported spec {} or Minecraft version {}",
            profile.spec, profile.minecraft
        )));
    }
    let entry = profile.json.trim_start_matches('/');
    path_safety::relative_path(entry).ok_or_else(|| invalid("unsafe version JSON entry"))?;
    let mut raw: serde_json::Value = read_installer_json(&mut archive, entry)?;
    if raw
        .get("inheritsFrom")
        .and_then(|value| value.as_str())
        .is_none()
    {
        raw["inheritsFrom"] = minecraft_version.into();
    }
    let version: minecraft::VersionJson = serde_json::from_value(raw.clone())?;
    safe_profile_id(&version.id)?;
    if version.id == minecraft_version
        || version.inherits_from.as_deref() != Some(minecraft_version)
        || version
            .main_class
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(invalid(
            "version profile has invalid id, inheritance, or main class",
        ));
    }

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_io_error)?;
        let Some(relative) = entry.name().strip_prefix("maven/") else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let relative = path_safety::relative_path(relative)
            .ok_or_else(|| invalid("unsafe embedded Maven path"))?;
        reject_symlink(&entry)?;
        file_ops::copy_reader_atomic(&root.join("libraries").join(relative), &mut entry)?;
    }

    let mut variables = HashMap::from([
        ("SIDE".into(), "client".into()),
        ("MINECRAFT_VERSION".into(), minecraft_version.into()),
        (
            "MINECRAFT_JAR".into(),
            root.join("versions")
                .join(minecraft_version)
                .join(format!("{minecraft_version}.jar"))
                .to_string_lossy()
                .into_owned(),
        ),
        ("ROOT".into(), root.to_string_lossy().into_owned()),
        ("INSTALLER".into(), installer.to_string_lossy().into_owned()),
        (
            "LIBRARY_DIR".into(),
            root.join("libraries").to_string_lossy().into_owned(),
        ),
    ]);
    for (key, data) in &profile.data {
        if variables.contains_key(key) {
            continue;
        }
        let value = &data.client;
        let resolved =
            if let Some(literal) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
                literal.to_owned()
            } else if value.starts_with('[') && value.ends_with(']') {
                expand(value, &variables, root)?
            } else {
                let entry_name = value.trim_start_matches('/');
                let relative = path_safety::relative_path(entry_name)
                    .ok_or_else(|| invalid(format!("unsafe data entry {value:?}")))?;
                let destination = work.join(&relative);
                let mut entry = archive.by_name(entry_name).map_err(zip_io_error)?;
                reject_symlink(&entry)?;
                file_ops::copy_reader_atomic(&destination, &mut entry)?;
                destination.to_string_lossy().into_owned()
            };
        variables.insert(key.clone(), resolved);
    }
    Ok(PreparedProfile {
        profile,
        version,
        version_json: serde_json::to_vec_pretty(&raw)?,
        variables,
    })
}

fn reject_symlink(entry: &zip::read::ZipFile<'_, File>) -> Result<(), InstallError> {
    if entry
        .unix_mode()
        .is_some_and(|mode| mode & 0o170000 == 0o120000)
    {
        return Err(invalid("installer contains a symbolic link"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_coordinates_and_tokens_without_reinterpreting_inserted_paths() {
        let root = std::env::temp_dir().join("forge {example}");
        let vars = HashMap::from([
            ("ROOT".into(), root.to_string_lossy().into_owned()),
            ("SIDE".into(), "client".into()),
        ]);
        assert_eq!(
            expand("{ROOT}/{SIDE}", &vars, &root).unwrap(),
            format!("{}/client", root.display())
        );
        assert_eq!(
            PathBuf::from(expand("[a.b:tool:1:client@zip]", &vars, &root).unwrap()),
            root.join("libraries/a/b/tool/1/tool-1-client.zip")
        );
        assert!(expand("{MISSING}", &vars, &root).is_err());
        assert!(expand("{ROOT", &vars, &root).is_err());
    }

    #[test]
    fn client_side_filter_preserves_empty_argument_processors() {
        let processors: Vec<Processor> = serde_json::from_str(
            r#"[
            {"jar":"common"}, {"jar":"null","sides":null},
            {"jar":"client","sides":["client","server"]},
            {"jar":"server","sides":["server"]}, {"jar":"empty","sides":[]}
        ]"#,
        )
        .unwrap();
        assert_eq!(
            processors
                .iter()
                .filter(|p| p.is_client())
                .map(|p| p.jar.as_str())
                .collect::<Vec<_>>(),
            ["common", "null", "client"]
        );
    }

    #[test]
    fn rejects_escaping_output_and_maven_paths() {
        let root = std::env::temp_dir().join("forge-safe");
        assert!(output_path(&root, &root.join("libraries/output.jar").to_string_lossy()).is_ok());
        assert!(output_path(&root, &root.join("../escaped.jar").to_string_lossy()).is_err());
        assert!(output_path(&root, "relative.jar").is_err());
        assert!(library_path(&root, "../evil:tool:1").is_err());
    }

    #[test]
    fn reads_realistic_profile_and_keeps_unknown_version_fields() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("forge-profile-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let installer = root.join("installer.jar");
        let mut archive = zip::ZipWriter::new(File::create(&installer).unwrap());
        for (name, contents) in [
            (
                "install_profile.json",
                r#"{"spec":0,"minecraft":"1.20.1","data":{"BINPATCH":{"client":"/data/client.lzma"},"HASH":{"client":"'abc'"},"OUTPUT":{"client":"[g:a:1:client]"}}}"#,
            ),
            (
                "version.json",
                r#"{"id":"1.20.1-forge-test","mainClass":"example.Main","customField":true}"#,
            ),
            ("data/client.lzma", "patch"),
            ("maven/g/a/1/a-1.jar", "embedded"),
        ] {
            archive
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(contents.as_bytes()).unwrap();
        }
        archive.finish().unwrap();
        let prepared = prepare(&installer, &root, "1.20.1", &root.join("work")).unwrap();
        assert_eq!(prepared.variables["HASH"], "abc");
        assert_eq!(
            std::fs::read(&prepared.variables["BINPATCH"]).unwrap(),
            b"patch"
        );
        assert_eq!(
            std::fs::read(root.join("libraries/g/a/1/a-1.jar")).unwrap(),
            b"embedded"
        );
        let json: serde_json::Value = serde_json::from_slice(&prepared.version_json).unwrap();
        assert_eq!(json["customField"], true);
        assert_eq!(json["inheritsFrom"], "1.20.1");
        assert!(prepare(&installer, &root, "1.19", &root.join("work")).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}

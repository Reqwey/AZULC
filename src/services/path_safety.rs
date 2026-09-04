use std::path::{Component, Path, PathBuf};

/// Returns a portable file name that is safe as one path component.
///
/// The validation deliberately follows the strictest supported platform so a
/// downloaded instance can be moved between Windows, macOS, and Linux.
pub(crate) fn file_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 255
        || value.ends_with(['.', ' '])
        || value.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return None;
    }

    let mut components = Path::new(value).components();
    let Component::Normal(name) = components.next()? else {
        return None;
    };
    if name.is_empty() || components.next().is_some() {
        return None;
    }

    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem.strip_prefix("COM").is_some_and(is_reserved_port)
        || stem.strip_prefix("LPT").is_some_and(is_reserved_port);
    (!reserved).then(|| name.to_string_lossy().into_owned())
}

/// Validates an identifier without silently normalizing it.
pub(crate) fn exact_component(value: &str) -> Option<&str> {
    file_name(value)
        .is_some_and(|normalized| normalized == value)
        .then_some(value)
}

/// Parses a provider-owned portable relative path without allowing it to
/// escape the caller's chosen root.
pub(crate) fn relative_path(value: &str) -> Option<PathBuf> {
    if value.is_empty() || value.trim() != value || value.contains('\\') {
        return None;
    }
    let mut output = PathBuf::new();
    for component in value.split('/') {
        output.push(exact_component(component)?);
    }
    (!output.as_os_str().is_empty()).then_some(output)
}

fn is_reserved_port(suffix: &str) -> bool {
    matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_accepts_one_normal_component() {
        assert_eq!(file_name(" example.jar "), Some("example.jar".to_owned()));
    }

    #[test]
    fn file_name_rejects_parent_traversal() {
        assert_eq!(file_name("../example.jar"), None);
    }

    #[test]
    fn file_name_rejects_nested_paths() {
        assert_eq!(file_name("mods/example.jar"), None);
    }

    #[test]
    fn file_name_rejects_absolute_paths() {
        assert_eq!(file_name("/example.jar"), None);
    }

    #[test]
    fn file_name_rejects_windows_reserved_names_on_every_platform() {
        assert_eq!(file_name("CON.jar"), None);
        assert_eq!(file_name("lpt1.zip"), None);
    }

    #[test]
    fn file_name_rejects_non_portable_characters_and_suffixes() {
        assert_eq!(file_name("bad?.jar"), None);
        assert_eq!(file_name("trailing. "), None);
    }

    #[test]
    fn exact_component_rejects_names_that_require_trimming() {
        assert_eq!(exact_component("profile-id"), Some("profile-id"));
        assert_eq!(exact_component(" profile-id "), None);
    }

    #[test]
    fn relative_path_accepts_portable_nested_paths() {
        assert_eq!(
            relative_path("org/example/library.jar"),
            Some(PathBuf::from("org/example/library.jar"))
        );
    }

    #[test]
    fn relative_path_rejects_traversal_and_foreign_separators() {
        assert_eq!(relative_path("../outside.jar"), None);
        assert_eq!(relative_path("org\\example\\library.jar"), None);
        assert_eq!(relative_path("org/CON/library.jar"), None);
    }
}

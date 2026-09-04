use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const CURSEFORGE_API_KEY_ENV: &str = "AZULC_CURSEFORGE_API_KEY";

/// Loads process configuration before Iced starts any worker threads.
///
/// The CurseForge key is read literally first because its `$` characters are
/// part of the credential, while regular dotenv interpolation treats them as
/// variable references. Other dotenv values retain normal `dotenvy` behavior.
pub fn load() {
    if env::var_os(CURSEFORGE_API_KEY_ENV).is_none()
        && let Some(path) = find_dotenv()
        && let Ok(contents) = fs::read_to_string(path)
        && let Some(value) = literal_value(&contents, CURSEFORGE_API_KEY_ENV)
    {
        // SAFETY: this runs at the beginning of `main`, before Iced or any
        // application worker thread is created.
        unsafe { env::set_var(CURSEFORGE_API_KEY_ENV, value) };
    }

    let _ = dotenvy::dotenv();
}

fn find_dotenv() -> Option<PathBuf> {
    if let Ok(directory) = env::current_dir()
        && let Some(path) = find_in_ancestors(&directory)
    {
        return Some(path);
    }

    env::current_exe()
        .ok()
        .and_then(|path| path.parent().and_then(find_in_ancestors))
}

fn find_in_ancestors(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|directory| directory.join(".env"))
        .find(|path| path.is_file())
}

fn literal_value(contents: &str, expected_key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let line = line.trim_start_matches('\u{feff}').trim_start();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        let line = line
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(line);
        let (key, value) = line.split_once('=')?;
        if key.trim() != expected_key {
            return None;
        }

        let value = value.trim();
        let value = match (value.as_bytes().first(), value.as_bytes().last()) {
            (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"')) if value.len() >= 2 => {
                &value[1..value.len() - 1]
            }
            _ => value,
        };
        Some(value.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curseforge_style_keys_are_loaded_without_interpolation() {
        let input = "AZULC_CURSEFORGE_API_KEY=$2a$10$example/value";
        assert_eq!(
            literal_value(input, CURSEFORGE_API_KEY_ENV).as_deref(),
            Some("$2a$10$example/value")
        );
    }

    #[test]
    fn matching_quotes_are_removed_but_the_secret_stays_literal() {
        let input = "export AZULC_CURSEFORGE_API_KEY='$2a$10$example/value'";
        assert_eq!(
            literal_value(input, CURSEFORGE_API_KEY_ENV).as_deref(),
            Some("$2a$10$example/value")
        );
    }
}

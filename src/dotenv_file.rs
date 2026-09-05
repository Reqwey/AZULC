pub(crate) fn literal_value(contents: &str, expected_key: &str) -> Option<String> {
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
            literal_value(input, "AZULC_CURSEFORGE_API_KEY").as_deref(),
            Some("$2a$10$example/value")
        );
    }

    #[test]
    fn matching_quotes_are_removed_but_the_value_stays_literal() {
        let input = "export AZULC_CURSEFORGE_API_KEY='$2a$10$example/value'";
        assert_eq!(
            literal_value(input, "AZULC_CURSEFORGE_API_KEY").as_deref(),
            Some("$2a$10$example/value")
        );
    }
}

use std::{io, path::PathBuf};

pub async fn reveal(path: PathBuf) -> io::Result<()> {
    tokio::task::spawn_blocking(move || reveal_blocking(path))
        .await
        .map_err(io::Error::other)?
}

#[cfg(target_os = "windows")]
fn reveal_blocking(path: PathBuf) -> io::Result<()> {
    use std::os::windows::process::CommandExt;

    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} does not exist", path.display()),
        ));
    }
    std::process::Command::new("explorer.exe")
        .raw_arg(explorer_selection_argument(&path)?)
        .spawn()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn explorer_selection_argument(path: &std::path::Path) -> io::Result<std::ffi::OsString> {
    let path = std::path::absolute(path)?;
    // Explorer requires /select, outside the quotes around a path with spaces.
    let mut argument = std::ffi::OsString::from("/select,\"");
    argument.push(path);
    argument.push("\"");
    Ok(argument)
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::{ffi::OsString, path::Path};

    #[test]
    fn explorer_quotes_only_the_path_with_spaces_commas_and_unicode() {
        let path = Path::new(r"C:\游戏实例\My Instance\resourcepacks\高清材质, v2.zip");
        assert_eq!(
            explorer_selection_argument(path).unwrap(),
            OsString::from(r#"/select,"C:\游戏实例\My Instance\resourcepacks\高清材质, v2.zip""#)
        );
    }

    #[test]
    fn explorer_resolves_relative_shader_paths_before_launching() {
        let path = Path::new(r"instances\test\shaderpacks\shader.zip");
        let expected = std::env::current_dir().unwrap().join(path);
        assert_eq!(
            explorer_selection_argument(path).unwrap(),
            OsString::from(format!("/select,\"{}\"", expected.display()))
        );
    }

    #[test]
    fn reveal_rejects_missing_files() {
        let path = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        assert_eq!(
            reveal_blocking(path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
}

#[cfg(target_os = "macos")]
fn reveal_blocking(path: PathBuf) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} does not exist", path.display()),
        ));
    }
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_blocking(path: PathBuf) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} does not exist", path.display()),
        ));
    }
    let directory = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(&path).to_path_buf()
    };
    open::that(directory)
}

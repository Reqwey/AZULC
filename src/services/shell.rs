use std::{io, path::PathBuf};

pub async fn reveal(path: PathBuf) -> io::Result<()> {
    tokio::task::spawn_blocking(move || reveal_blocking(path))
        .await
        .map_err(io::Error::other)?
}

#[cfg(target_os = "windows")]
fn reveal_blocking(path: PathBuf) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} does not exist", path.display()),
        ));
    }
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()?;
    Ok(())
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

use dirs_2;

use std::path::PathBuf;

use crate::fail::HResult;

pub fn home_path() -> HResult<PathBuf> {
    let home = dirs_2::home_dir()?;
    Ok(home)
}

pub fn ranger_path() -> HResult<PathBuf> {
    let mut ranger_path = dirs_2::config_dir()?;
    ranger_path.push("ranger/");
    Ok(ranger_path)
}

pub fn hunter_path() -> HResult<PathBuf> {
    let mut hunter_path = if cfg!(mac_os) {
        PathBuf::from("~/.config/").canonicalize()?
    } else {
        dirs_2::config_dir()?
    };
    hunter_path.push("hunter/");
    Ok(hunter_path)
}

pub fn config_path() -> HResult<PathBuf> {
    let mut config_path = hunter_path()?;
    config_path.push("config");
    Ok(config_path)
}

pub fn bookmark_path() -> HResult<PathBuf> {
    let mut bookmark_path = hunter_path()?;
    bookmark_path.push("bookmarks");
    Ok(bookmark_path)
}

pub fn tagfile_path() -> HResult<PathBuf> {
    let mut tagfile_path = hunter_path()?;
    tagfile_path.push("tags");
    Ok(tagfile_path)
}

pub fn history_path() -> HResult<PathBuf> {
    let mut history_path = hunter_path()?;
    history_path.push("history");
    Ok(history_path)
}

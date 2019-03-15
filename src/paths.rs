use dirs_2;

use std::path::PathBuf;

use crate::fail::HResult;

pub fn hunter_path() -> HResult<PathBuf> {
    let mut config_dir = dirs_2::config_dir()?;
    config_dir.push("hunter/");
    Ok(config_dir)
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

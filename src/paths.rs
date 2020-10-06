use dirs_2;

use std::path::PathBuf;

use crate::fail::{HError, HResult};

pub fn home_path() -> HResult<PathBuf> {
    let home = dirs_2::home_dir().ok_or_else(|| HError::NoneError)?;
    Ok(home)
}

pub fn ranger_path() -> HResult<PathBuf> {
    let mut ranger_path = dirs_2::config_dir().ok_or_else(|| HError::NoneError)?;
    ranger_path.push("ranger/");
    Ok(ranger_path)
}

#[cfg(not(target_os = "macos"))]
pub fn hunter_path() -> HResult<PathBuf> {
    let mut hunter_path = dirs_2::config_dir().ok_or_else(|| HError::NoneError)?;
    hunter_path.push("hunter/");
    Ok(hunter_path)
}

#[cfg(target_os = "macos")]
pub fn hunter_path() -> HResult<PathBuf> {
    let mut hunter_path = home_path()?;
    hunter_path.push(".config/");
    hunter_path.push("hunter/");
    Ok(hunter_path)
}

pub fn config_path() -> HResult<PathBuf> {
    let mut config_path = hunter_path()?;
    config_path.push("config");
    Ok(config_path)
}

pub fn bindings_path() -> HResult<PathBuf> {
    let mut config_path = hunter_path()?;
    config_path.push("keys");
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

pub fn actions_path() -> HResult<PathBuf> {
    let mut actions_path = hunter_path()?;
    actions_path.push("actions");
    Ok(actions_path)
}

pub fn previewers_path() -> HResult<PathBuf> {
    let mut previewers_path = hunter_path()?;
    previewers_path.push("previewers");
    Ok(previewers_path)
}

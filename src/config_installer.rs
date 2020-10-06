use std::fs::*;
use std::io::Write;
use std::process::Command;
use std::ffi::OsStr;
use std::path::Path;

use crate::fail::{HError, HResult, ErrorLog};
use crate::widget::WidgetCore;


pub fn ensure_config(core: WidgetCore) -> HResult<()> {
    if has_config()? {
        let previewers_path = crate::paths::previewers_path()?;
        let actions_path = crate::paths::actions_path()?;

        if !previewers_path.exists() {
            core.show_status("Coulnd't find previewers in config dir! Adding!")?;
            install_config_previewers()
                .or_else(|_|
                         core.show_status("Error installing previewers! Check log!"))?;
        }

        if !actions_path.exists() {
            core.show_status("Coulnd't find actions in config dir! Adding!")?;
            install_config_actions()
                .or_else(|_|
                         core.show_status("Error installing actions! Check log!"))?;
        }

        return Ok(());
    }

    let msg = match install_config_all() {
        Ok(_) => format!("Config installed in: {}",
                         crate::paths::hunter_path()?.to_string_lossy()),
        Err(_) => format!("{}Problems with installation of default configuration! Look inside log.",
                          crate::term::color_red()),
    };
    core.show_status(&msg)?;

    Ok(())
}


fn default_config_archive() -> &'static [u8] {
    let default_config = include_bytes!("../config.tar.gz");
    default_config
}

fn has_config() -> HResult<bool> {
    let config_dir = crate::paths::hunter_path()?;

    if config_dir.exists() {
        return Ok(true);
    } else {
        return Ok(false);
    }
}


fn install_config_all() -> HResult<()> {
    let hunter_dir = crate::paths::hunter_path()?;
    let config_dir = hunter_dir.parent().ok_or_else(|| HError::NoneError)?;

    if !hunter_dir.exists() {
        // create if non-existing
        std::fs::create_dir(&hunter_dir)
            .or_else(|_| HError::log(&format!("Couldn't create directory: {}",
                                              hunter_dir.as_os_str()
                                                        .to_string_lossy())))?;
    }

    let archive_path = create_archive()?;
    extract_archive(config_dir, &archive_path)?;
    delete_archive(archive_path)?;

    Ok(())
}

fn copy(from: &Path, to: &Path) -> HResult<()> {
    // Uses -a flag to preserve symlinks
    let success = Command::new("cp")
        .arg("-a")
        .arg(from)
        .arg(to.as_os_str())
        .status()
        .map(|s| s.success());

    if success.is_err() || !success.unwrap() {
        HError::log(&format!("Couldn't copy {} to {} !",
                             from.to_string_lossy(),
                             to.to_string_lossy()))
    } else {
        Ok(())
    }
}

fn install_config_previewers() -> HResult<()> {
    let hunter_dir = crate::paths::hunter_path()?;
    let archive_path = create_archive()?;
    extract_archive(Path::new("/tmp"), &archive_path)?;
    copy(Path::new("/tmp/hunter/previewers"), &hunter_dir)?;
    delete_archive(&archive_path)
}

fn install_config_actions() -> HResult<()> {
    let hunter_dir = crate::paths::hunter_path()?;
    let archive_path = create_archive()?;
    extract_archive(Path::new("/tmp"), &archive_path)?;
    copy(Path::new("/tmp/hunter/actions"), &hunter_dir)?;
    delete_archive(&archive_path)
}

fn update_previewer() -> HResult<()> {
    let previewer_dir = crate::paths::previewers_path()?;
    let archive_path = create_archive()?;

    extract_archive(Path::new("/tmp"), &archive_path)?;

    update_dir(Path::new("/tmp/hunter/previewers"), &previewer_dir).log();

    delete_archive(&archive_path)?;

    Ok(())
}

fn update_actions() -> HResult<()> {
    let actions_dir = crate::paths::actions_path()?;
    let archive_path = create_archive()?;

    extract_archive(Path::new("/tmp"), &archive_path)?;

    update_dir(Path::new("/tmp/hunter/actions"), &actions_dir).log();

    delete_archive(&archive_path)?;

    Ok(())
}

pub fn update_config(core: WidgetCore, force: bool) -> HResult<()> {
    // First install whatever might be missing, makes sure all dirs are there
    ensure_config(core).log();

    // Just overwrite everything except core config/keys with the latest version
    if force {
        install_config_previewers().log();
        install_config_actions().log();
        return Ok(())
    }

    let archive_path = create_archive()?;
    extract_archive(Path::new("/tmp"), &archive_path)?;
    Ok(())
}

fn update_dir<P: AsRef<Path>>(source: P, target: P) -> HResult<()> {
    for file in std::fs::read_dir(source)? {
        let file_path = file?.path();
        let file_name = file_path.file_name().ok_or_else(|| HError::NoneError)?;
        let target_path = target.as_ref().join(file_name);

        if file_path.is_dir() {
            // Check subdirectories recursively
            update_dir(&file_path, &target_path).log();
        } else {
            if !target_path.exists() {
                HError::log::<()>(&format!("Installing additional config file: {}",
                                           file_path.to_string_lossy())).ok();
                copy(&file_path, &target_path).log();
            }
        }
    }

    Ok(())
}


fn create_archive() -> HResult<&'static str> {
    let archive_path = "/tmp/hunter-config.tar.gz";
    let def_config = default_config_archive();

    File::create(archive_path)
        .and_then(|mut f| {
            f.write_all(def_config).map(|_| f)
        })
        .and_then(|mut f| f.flush())
        .or_else(|_| {
            HError::log(&format!("Failed to write config archive to: {}",
                                 archive_path))
        })?;
    Ok(archive_path)
}


fn extract_archive(to: &Path, archive_path: &str) -> HResult<()> {
    let success = Command::new("tar")
        .args(&[OsStr::new("-C"),
                to.as_os_str(),
                OsStr::new("-xf"),
                OsStr::new(archive_path)])
        .status()
        .or_else(|_| HError::log(&format!("Couldn't run tar!")))
        .map(|s| s.success())?;

    if !success {
        HError::log(&format!("Extraction of archive failed! Archive: {}",
                             archive_path))?
    }

    Ok(())
}

fn delete_archive(archive_path: &str) -> HResult<()> {
    std::fs::remove_file(archive_path)
        .or_else(|_| HError::log(&format!("Deletion of archive failed! Archive: {}",
                                          archive_path)))
}
